use crate::conflict::{
    classify_conflict_content, encode_edited_conflict_text, missing_worktree_identity,
    supports_merge_tree_preflight,
};
use crate::parse::StashCommit;
use crate::validate::{sparse_patterns, validate_mainline_parent};

use std::{fs, process::Command};

use tempfile::tempdir;

use super::*;

fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("run fixture git command");
    assert!(
        output.status.success(),
        "fixture git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("run fixture git command");
    assert!(
        output.status.success(),
        "fixture git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("fixture git output is UTF-8")
        .trim()
        .to_owned()
}

fn git_with_date(path: &Path, args: &[&str], date: &str) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .output()
        .expect("run dated fixture git command");
    assert!(
        output.status.success(),
        "dated fixture git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn committed_repository() -> (tempfile::TempDir, GitCliBackend, String) {
    let directory = tempdir().expect("temp repository");
    let backend = GitCliBackend::default();
    backend
        .init_repository(directory.path(), "main")
        .await
        .expect("initialize repository");
    git(directory.path(), &["config", "user.name", "GitCat Test"]);
    git(
        directory.path(),
        &["config", "user.email", "gitcat@example.test"],
    );
    fs::write(directory.path().join("hello.txt"), "first\n").expect("write fixture");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage fixture");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "initial subject\n\nsearchable body text\nárvíztűrő tükörfúrógép".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit fixture");
    let oid = backend
        .head_oid(directory.path())
        .await
        .expect("read HEAD")
        .expect("HEAD exists");
    (directory, backend, oid)
}

async fn preflight_conflict_repository() -> (tempfile::TempDir, GitCliBackend, String, String) {
    let (directory, backend, base_oid) = committed_repository().await;
    backend
        .create_branch(directory.path(), "conflicting", &base_oid, true)
        .await
        .expect("create preflight target branch");
    fs::write(directory.path().join("hello.txt"), "target version\n")
        .expect("write target version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage target version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "target change".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit target version");
    let target_oid = backend
        .head_oid(directory.path())
        .await
        .expect("read target HEAD")
        .expect("target HEAD exists");

    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main for preflight");
    fs::write(directory.path().join("hello.txt"), "current version\n")
        .expect("write current version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage current version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "current change".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit current version");
    let head_oid = backend
        .head_oid(directory.path())
        .await
        .expect("read current HEAD")
        .expect("current HEAD exists");

    (directory, backend, target_oid, head_oid)
}

fn preflight_observable_state(path: &Path) -> (String, String, String, Vec<u8>) {
    (
        git_stdout(path, &["rev-parse", "HEAD"]),
        git_stdout(path, &["status", "--porcelain=v2"]),
        git_stdout(path, &["diff", "--cached", "--binary"]),
        fs::read(path.join("hello.txt")).expect("read working copy"),
    )
}

async fn conflicted_repository() -> (tempfile::TempDir, GitCliBackend) {
    let (directory, backend, base_oid) = committed_repository().await;
    backend
        .create_branch(directory.path(), "conflicting", &base_oid, true)
        .await
        .expect("create conflicting branch");
    fs::write(directory.path().join("hello.txt"), "branch version\n")
        .expect("write branch version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage branch version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "branch change".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit branch version");

    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main");
    fs::write(directory.path().join("hello.txt"), "main version\n").expect("write main version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage main version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "main change".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit main version");

    let result = backend
        .merge_branch(directory.path(), "conflicting")
        .await
        .expect("conflict is returned as resumable state");
    assert!(result.needs_user_action);
    assert_eq!(result.conflicts.len(), 1);
    (directory, backend)
}

async fn binary_conflicted_repository() -> (tempfile::TempDir, GitCliBackend, Vec<String>) {
    let (directory, backend, base_oid) = committed_repository().await;
    let paths: Vec<String> = vec![
        "plugins/alpha.jar".into(),
        "plugins/beta.jar".into(),
        "plugins/gamma.jar".into(),
    ];
    fs::create_dir_all(directory.path().join("plugins")).expect("create plugins directory");
    let write_all = |bytes: [u8; 4]| {
        for path in &paths {
            fs::write(directory.path().join(path), bytes).expect("write binary fixture");
        }
    };
    async fn commit(backend: &GitCliBackend, path: &Path, message: &str) {
        backend
            .create_commit(
                path,
                &CommitOptions {
                    message: message.into(),
                    amend: false,
                    signoff: false,
                },
            )
            .await
            .expect("commit binary fixture");
    }

    write_all([0, 1, 2, 3]);
    backend
        .stage_paths(directory.path(), &paths)
        .await
        .expect("stage base binaries");
    commit(&backend, directory.path(), "base plugins").await;
    let plugin_base = backend
        .head_oid(directory.path())
        .await
        .expect("read HEAD")
        .expect("HEAD exists");
    assert_ne!(plugin_base, base_oid);

    backend
        .create_branch(directory.path(), "incoming", &plugin_base, true)
        .await
        .expect("create incoming branch");
    write_all([9, 9, 9, 9]);
    backend
        .stage_paths(directory.path(), &paths)
        .await
        .expect("stage incoming binaries");
    commit(&backend, directory.path(), "incoming plugins").await;

    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main");
    write_all([5, 5, 5, 5]);
    backend
        .stage_paths(directory.path(), &paths)
        .await
        .expect("stage current binaries");
    commit(&backend, directory.path(), "current plugins").await;

    let result = backend
        .merge_branch(directory.path(), "incoming")
        .await
        .expect("binary conflicts are a resumable state");
    assert_eq!(result.conflicts.len(), paths.len());
    (directory, backend, paths)
}

async fn modify_delete_conflicted_repository() -> (tempfile::TempDir, GitCliBackend) {
    let (directory, backend, base_oid) = committed_repository().await;
    backend
        .create_branch(directory.path(), "deleting", &base_oid, true)
        .await
        .expect("create deleting branch");
    git(directory.path(), &["rm", "--", "hello.txt"]);
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "delete hello".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit deletion");

    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main");
    fs::write(directory.path().join("hello.txt"), "main version\n").expect("write main version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage main version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "modify hello".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit modification");
    backend
        .merge_branch(directory.path(), "deleting")
        .await
        .expect("modify/delete conflict is resumable");
    (directory, backend)
}

async fn delete_modify_conflicted_repository() -> (tempfile::TempDir, GitCliBackend) {
    let (directory, backend, base_oid) = committed_repository().await;
    backend
        .create_branch(directory.path(), "modifying", &base_oid, true)
        .await
        .expect("create modifying branch");
    fs::write(directory.path().join("hello.txt"), "branch version\n")
        .expect("write branch version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage branch version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "modify hello".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit modification");

    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main");
    git(directory.path(), &["rm", "--", "hello.txt"]);
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "delete hello".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit deletion");
    backend
        .merge_branch(directory.path(), "modifying")
        .await
        .expect("delete/modify conflict is resumable");
    (directory, backend)
}

async fn conflict_expected(
    backend: &GitCliBackend,
    path: &Path,
    conflict_path: &str,
) -> ConflictExpectedState {
    backend
        .conflict_details(path, conflict_path)
        .await
        .expect("read conflict details")
        .expected_state
}

#[tokio::test]
async fn reword_head_updates_message_without_folding_staged_changes() {
    let (directory, backend, _oid) = committed_repository().await;
    // A staged change on a different path must stay out of the amended commit.
    fs::write(directory.path().join("staged.txt"), "staged\n").expect("write staged");
    backend
        .stage_paths(directory.path(), &["staged.txt".into()])
        .await
        .expect("stage extra file");

    backend
        .reword_commit(directory.path(), "HEAD", "reworded head\n\nnew body")
        .await
        .expect("reword head");

    assert_eq!(
        git_stdout(directory.path(), &["log", "-1", "--format=%B"]).trim(),
        "reworded head\n\nnew body"
    );
    assert_eq!(
        git_stdout(directory.path(), &["ls-tree", "--name-only", "HEAD"]),
        "hello.txt"
    );
    assert_eq!(
        git_stdout(directory.path(), &["diff", "--cached", "--name-only"]),
        "staged.txt"
    );
}

#[tokio::test]
async fn reword_older_commit_preserves_author_and_replays_descendants() {
    let (directory, backend, base_oid) = committed_repository().await;
    let author_before = git_stdout(
        directory.path(),
        &["show", "-s", "--format=%an <%ae> %aI", &base_oid],
    );
    fs::write(directory.path().join("hello.txt"), "first\nsecond\n").expect("write second");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage second");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "child commit".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit child");

    backend
        .reword_commit(directory.path(), &base_oid, "reworded base")
        .await
        .expect("reword base commit");

    let subjects = git_stdout(directory.path(), &["log", "--reverse", "--format=%s"]);
    assert_eq!(subjects, "reworded base\nchild commit");
    // Authorship (including the author date) survives the rewrite.
    let root_oid = git_stdout(directory.path(), &["rev-list", "--max-parents=0", "HEAD"]);
    assert_eq!(
        git_stdout(
            directory.path(),
            &["show", "-s", "--format=%an <%ae> %aI", &root_oid]
        ),
        author_before
    );
    // The working tree is unchanged by the replay (ignore platform CRLF).
    let worktree = fs::read_to_string(directory.path().join("hello.txt"))
        .expect("read worktree")
        .replace("\r\n", "\n");
    assert_eq!(worktree, "first\nsecond\n");
}

#[tokio::test]
async fn reword_rejects_commit_off_the_current_branch() {
    let (directory, backend, base_oid) = committed_repository().await;
    // A commit that only lives on another branch is not reachable from HEAD.
    backend
        .create_branch(directory.path(), "side", &base_oid, true)
        .await
        .expect("create side branch");
    fs::write(directory.path().join("side.txt"), "side\n").expect("write side");
    backend
        .stage_paths(directory.path(), &["side.txt".into()])
        .await
        .expect("stage side");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "side only".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit side");
    let side_oid = backend
        .head_oid(directory.path())
        .await
        .expect("read side HEAD")
        .expect("side HEAD exists");
    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main");

    let error = backend
        .reword_commit(directory.path(), &side_oid, "nope")
        .await
        .expect_err("reword off-branch commit is rejected");
    assert_eq!(error.code, ErrorCode::UnsupportedOperation);
}

#[tokio::test]
async fn conflict_preflight_reports_clean_without_touching_repository_state() {
    let (directory, backend, head_oid) = committed_repository().await;
    git(directory.path(), &["branch", "clean-target"]);
    let before = preflight_observable_state(directory.path());

    let result = backend
        .conflict_preflight(directory.path(), "clean-target")
        .await
        .expect("run clean conflict preflight");

    assert_eq!(result.target, "clean-target");
    assert_eq!(result.target_oid, head_oid);
    assert_eq!(result.state, ConflictPreflightState::Clean);
    assert!(result.conflicting_paths.is_empty());
    assert_eq!(result.unavailable_reason, None);
    assert_eq!(preflight_observable_state(directory.path()), before);
}

#[tokio::test]
async fn conflict_preflight_reports_paths_without_touching_repository_state() {
    let (directory, backend, target_oid, head_oid) = preflight_conflict_repository().await;
    let before = preflight_observable_state(directory.path());

    let result = backend
        .conflict_preflight(directory.path(), "conflicting")
        .await
        .expect("run conflicting preflight");

    assert_eq!(result.target, "conflicting");
    assert_eq!(result.target_oid, target_oid);
    assert_eq!(result.state, ConflictPreflightState::Conflicting);
    assert_eq!(result.conflicting_paths, vec!["hello.txt"]);
    assert_eq!(result.unavailable_reason, None);
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "HEAD"]),
        head_oid
    );
    assert_eq!(preflight_observable_state(directory.path()), before);

    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot after conflict preflight");
    assert_eq!(snapshot.operation_state, RepositoryOperationState::Normal);
    assert!(snapshot.status.clean);
}

#[tokio::test]
async fn conflict_preflight_rejects_unresolvable_target() {
    let (directory, backend, _) = committed_repository().await;
    let error = backend
        .conflict_preflight(directory.path(), "missing-target")
        .await
        .expect_err("target must resolve before preflight");
    assert_eq!(error.code, ErrorCode::InvalidRevision);
}

#[test]
fn conflict_preflight_version_gate_is_stable() {
    let version = |major, minor| GitVersion {
        major,
        minor,
        patch: 0,
        raw: format!("git version {major}.{minor}.0"),
    };
    assert!(!supports_merge_tree_preflight(&version(2, 37)));
    assert!(supports_merge_tree_preflight(&version(2, 38)));
    assert!(supports_merge_tree_preflight(&version(3, 0)));
}

#[tokio::test]
async fn repository_read_workflow_uses_machine_formats() {
    let (directory, backend, oid) = committed_repository().await;
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot repository");
    assert!(snapshot.status.clean);
    assert_eq!(snapshot.local_branches.len(), 1);
    assert_eq!(snapshot.local_branches[0].name, "main");

    let history = backend
        .history(directory.path(), &HistoryQuery::default())
        .await
        .expect("history");
    assert_eq!(history.commits.len(), 1);
    assert_eq!(history.commits[0].oid, oid);
    assert_eq!(history.commits[0].subject, "initial subject");

    let search = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "BODY TEXT".into(),
                scope: HistoryScope::AllRefs,
                limit: 20,
            },
        )
        .await
        .expect("search body");
    assert_eq!(search.total, 1);
    assert!(search.hits[0].matched_body);

    let unicode_search = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "ÁRVÍZTŰRŐ".into(),
                scope: HistoryScope::AllRefs,
                limit: 20,
            },
        )
        .await
        .expect("search Unicode body");
    assert_eq!(unicode_search.total, 1);
    assert!(unicode_search.hits[0].matched_body);

    let details = backend
        .commit_details(directory.path(), &oid, 0)
        .await
        .expect("commit details");
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].new_path, "hello.txt");
    assert_eq!(details.files[0].additions, Some(1));
}

#[tokio::test]
async fn all_refs_history_ignores_foreign_refs_to_pruned_objects() {
    let (directory, backend, oid) = committed_repository().await;
    // Another tool's checkpoint ref whose object was pruned, as Codex leaves behind.
    let foreign = directory.path().join(".git/refs/codex/checkpoints");
    fs::create_dir_all(&foreign).expect("create foreign ref namespace");
    fs::write(
        foreign.join("gone"),
        "d733a34e5816afc70ebbcccd0ad5852848dd2fe6\n",
    )
    .expect("write broken ref");
    // A detached HEAD on a commit no branch reaches must still be walked.
    git(directory.path(), &["checkout", "--detach"]);
    fs::write(directory.path().join("hello.txt"), "detached\n").expect("write change");
    git(directory.path(), &["commit", "-am", "detached subject"]);
    let detached_oid = git_stdout(directory.path(), &["rev-parse", "HEAD"]);

    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("history despite a broken foreign ref");
    let oids: Vec<&str> = page.commits.iter().map(|c| c.oid.as_str()).collect();
    assert_eq!(oids, [detached_oid.as_str(), oid.as_str()]);

    let search = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "subject".into(),
                scope: HistoryScope::AllRefs,
                limit: 20,
            },
        )
        .await
        .expect("search despite a broken foreign ref");
    assert_eq!(search.total, 2);
}

#[tokio::test]
async fn history_shows_one_row_per_stash() {
    let (directory, backend, _) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "stashed\n").expect("write tracked change");
    fs::write(directory.path().join("extra.txt"), "untracked\n").expect("write untracked file");
    git(directory.path(), &["stash", "push", "--include-untracked"]);
    let outer_stash_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let base_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^1"]);
    let visible_index_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);
    let untracked_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^3"]);

    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("history page");

    let stash_rows: Vec<&CommitSummary> = page
        .commits
        .iter()
        .filter(|commit| commit.stash.is_some())
        .collect();
    assert_eq!(stash_rows.len(), 1);
    assert_eq!(stash_rows[0].subject, "WIP on main");
    assert_eq!(stash_rows[0].oid, visible_index_oid);
    assert_eq!(stash_rows[0].parent_oids, vec![base_oid]);
    assert_eq!(stash_rows[0].parent_oids.len(), 1);
    assert_eq!(stash_rows[0].graph.edges.len(), 1);
    assert_eq!(stash_rows[0].graph.lane, 0);
    assert_eq!(stash_rows[0].stash.as_ref().unwrap().index, 0);
    let first_stash_oid = stash_rows[0].oid.clone();
    assert!(
        !page
            .commits
            .iter()
            .any(|commit| commit.oid == outer_stash_oid)
    );
    assert!(
        !page
            .commits
            .iter()
            .any(|commit| commit.oid == untracked_oid)
    );
    assert!(
        !page
            .commits
            .iter()
            .any(|commit| commit.subject.starts_with("index on")
                || commit.subject.starts_with("untracked files on"))
    );

    fs::write(directory.path().join("hello.txt"), "named\n").expect("write second change");
    backend
        .stash_push(directory.path(), Some("layout tweaks"), true)
        .await
        .expect("stash with message");
    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("history page after named stash");
    let mut labels: Vec<&str> = page
        .commits
        .iter()
        .filter(|commit| commit.stash.is_some())
        .map(|commit| commit.subject.as_str())
        .collect();
    labels.sort_unstable();
    assert_eq!(labels, vec!["WIP on main", "layout tweaks"]);

    let details = backend
        .commit_details(directory.path(), &first_stash_oid, 0)
        .await
        .expect("stash commit details");
    assert_eq!(details.subject, "WIP on main");
}

#[tokio::test]
async fn shared_stash_index_parent_uses_oldest_label_and_newest_action() {
    let (directory, backend, base_oid) = committed_repository().await;
    let collision_date = "2030-01-01T00:00:00 +0000";

    fs::write(directory.path().join("hello.txt"), "first\ncollision A\n")
        .expect("write collision A");
    git_with_date(
        directory.path(),
        &["stash", "push", "--message", "oracle: collision A"],
        collision_date,
    );
    let older_outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let visible_index_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);

    fs::write(directory.path().join("hello.txt"), "first\ncollision B\n")
        .expect("write collision B");
    git_with_date(
        directory.path(),
        &["stash", "push", "--message", "oracle: collision B"],
        collision_date,
    );
    let newest_outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    assert_ne!(newest_outer_oid, older_outer_oid);
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]),
        visible_index_oid
    );
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "stash@{1}^2"]),
        visible_index_oid
    );

    let listed = backend
        .stash_list(directory.path())
        .await
        .expect("list collapsed collision");
    assert_eq!(
        listed,
        vec![StashEntry {
            index: 0,
            oid: newest_outer_oid.clone(),
            message: "oracle: collision A".into(),
        }]
    );
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot collapsed collision");
    assert_eq!(snapshot.status.stash_count, 1);

    let query = HistoryQuery {
        scope: HistoryScope::AllRefs,
        cursor: None,
        limit: 50,
    };
    let page = backend
        .history(directory.path(), &query)
        .await
        .expect("history with collapsed collision");
    let stash_rows = page
        .commits
        .iter()
        .filter(|commit| commit.stash.is_some())
        .collect::<Vec<_>>();
    assert_eq!(stash_rows.len(), 1);
    let stash_row = stash_rows[0];
    assert_eq!(stash_row.oid, visible_index_oid);
    assert_eq!(stash_row.subject, "oracle: collision A");
    assert_eq!(stash_row.parent_oids, vec![base_oid]);
    assert_eq!(stash_row.graph.lane, 0);
    assert_eq!(
        stash_row.stash.as_ref(),
        Some(&StashRef {
            index: 0,
            selector: "stash@{0}".into(),
            oid: newest_outer_oid.clone(),
        })
    );

    let newest_tree_oid = git_stdout(
        directory.path(),
        &["rev-parse", &format!("{newest_outer_oid}^{{tree}}")],
    );
    for lookup_oid in [&visible_index_oid, &newest_outer_oid, &older_outer_oid] {
        let details = backend
            .commit_details(directory.path(), lookup_oid, 0)
            .await
            .expect("collapsed stash details resolve through every identity");
        assert_eq!(details.subject, "oracle: collision A");
        assert_eq!(details.tree_oid, newest_tree_oid);

        let diff = backend
            .diff(
                directory.path(),
                &DiffRequest {
                    target: DiffTarget::Commit {
                        oid: lookup_oid.clone(),
                        parent_index: 0,
                    },
                    path: "hello.txt".into(),
                    context_lines: 3,
                    ignore_whitespace: false,
                    max_bytes: 1024 * 1024,
                    whole_file: false,
                },
            )
            .await
            .expect("collapsed stash diff resolves through every identity");
        assert!(
            diff.hunks.iter().flat_map(|hunk| &hunk.lines).any(|line| {
                line.kind == DiffLineKind::Addition && line.content == "collision B"
            })
        );
        assert!(
            !diff.hunks.iter().flat_map(|hunk| &hunk.lines).any(|line| {
                line.kind == DiffLineKind::Addition && line.content == "collision A"
            })
        );
    }

    for outer_oid in [&newest_outer_oid, &older_outer_oid] {
        let by_outer = backend
            .history(
                directory.path(),
                &HistoryQuery {
                    scope: HistoryScope::Ref(outer_oid.clone()),
                    cursor: None,
                    limit: 1,
                },
            )
            .await
            .expect("history resolves every colliding outer stash");
        assert_eq!(by_outer.commits.len(), 1);
        assert_eq!(by_outer.commits[0].oid, visible_index_oid);
        assert_eq!(by_outer.commits[0].subject, "oracle: collision A");
    }

    let action_oid = stash_row.stash.as_ref().expect("stash action").oid.clone();
    backend
        .stash_apply(directory.path(), &action_oid, true)
        .await
        .expect("pop displayed collision row");
    assert_eq!(
        fs::read_to_string(directory.path().join("hello.txt"))
            .expect("read popped worktree")
            .replace("\r\n", "\n"),
        "first\ncollision B\n"
    );
    assert_eq!(
        git_stdout(directory.path(), &["stash", "list", "--format=%gs"]),
        "On main: oracle: collision A"
    );
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "stash@{0}"]),
        older_outer_oid
    );

    let after = backend
        .history(directory.path(), &query)
        .await
        .expect("history after popping newest collision member");
    let remaining = after
        .commits
        .iter()
        .find(|commit| commit.stash.is_some())
        .expect("older stash remains visible");
    assert_eq!(remaining.subject, "oracle: collision A");
    assert_eq!(remaining.stash.as_ref().expect("remaining stash").index, 0);
    assert_eq!(remaining.graph.lane, 1);
}

#[tokio::test]
async fn stash_ref_history_starts_with_the_visible_row_at_limit_one() {
    let (directory, backend, _) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "stashed\n").expect("write stash change");
    git(
        directory.path(),
        &["stash", "push", "--message", "visible stash"],
    );
    let outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let visible_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);

    for reference in ["stash", "stash@{0}", outer_oid.as_str()] {
        let page = backend
            .history(
                directory.path(),
                &HistoryQuery {
                    scope: HistoryScope::Ref(reference.into()),
                    cursor: None,
                    limit: 1,
                },
            )
            .await
            .unwrap_or_else(|error| panic!("history for {reference:?} failed: {error}"));

        assert_eq!(page.commits.len(), 1, "history for {reference:?}");
        assert_eq!(
            page.commits[0].oid, visible_oid,
            "history for {reference:?}"
        );
        assert_eq!(page.commits[0].subject, "visible stash");
        assert_eq!(
            page.commits[0]
                .stash
                .as_ref()
                .map(|stash| stash.selector.as_str()),
            Some("stash@{0}")
        );
        assert!(page.has_more);
        assert!(page.next_cursor.is_some());
    }
}

#[tokio::test]
async fn commit_search_virtualizes_latest_and_older_stash_rows() {
    let (directory, backend, base_oid) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "older stash\n")
        .expect("write older stash change");
    git_with_date(
        directory.path(),
        &["stash", "push", "--message", "searchable older stash"],
        "2030-01-01T00:00:00 +0000",
    );
    let older_outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let older_visible_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);

    fs::write(directory.path().join("hello.txt"), "latest stash\n")
        .expect("write latest stash change");
    git_with_date(
        directory.path(),
        &["stash", "push", "--message", "searchable latest stash"],
        "2031-01-01T00:00:00 +0000",
    );
    let latest_outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let latest_visible_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);

    let all = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "SEARCHABLE".into(),
                scope: HistoryScope::AllRefs,
                limit: 10,
            },
        )
        .await
        .expect("search both stash labels");
    assert_eq!(all.total, 3);
    assert!(!all.truncated);
    assert_eq!(
        all.hits
            .iter()
            .map(|hit| (hit.oid.as_str(), hit.subject.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (latest_visible_oid.as_str(), "searchable latest stash"),
            (older_visible_oid.as_str(), "searchable older stash"),
            (base_oid.as_str(), "initial subject"),
        ]
    );
    assert!(all.hits[..2].iter().all(|hit| hit.matched_subject));
    assert!(all.hits[..2].iter().all(|hit| !hit.matched_body));
    assert!(all.hits[2].matched_body);
    assert!(
        all.hits
            .iter()
            .all(|hit| hit.oid != latest_outer_oid && hit.oid != older_outer_oid)
    );

    let limited = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "searchable".into(),
                scope: HistoryScope::AllRefs,
                limit: 1,
            },
        )
        .await
        .expect("search stash labels with a one-row limit");
    assert_eq!(limited.total, 3);
    assert!(limited.truncated);
    assert_eq!(limited.hits.len(), 1);
    assert_eq!(limited.hits[0].oid, latest_visible_oid);

    let older = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "older stash".into(),
                scope: HistoryScope::AllRefs,
                limit: 10,
            },
        )
        .await
        .expect("search an older stash label");
    assert_eq!(older.total, 1);
    assert_eq!(older.hits.len(), 1);
    assert_eq!(older.hits[0].oid, older_visible_oid);
}

#[test]
fn applying_the_stash_view_preserves_git_log_order() {
    let summary = |oid: &str| CommitSummary {
        oid: oid.into(),
        short_oid: oid.into(),
        parent_oids: Vec::new(),
        subject: oid.into(),
        body_preview: String::new(),
        author: Identity {
            name: "Test".into(),
            email: "test@example.invalid".into(),
        },
        authored_at: CommitTime {
            seconds: 0,
            offset_minutes: 0,
        },
        committed_at: CommitTime {
            seconds: 0,
            offset_minutes: 0,
        },
        decorations: Vec::new(),
        stash: None,
        graph: GraphCell::default(),
    };
    let mut commits = vec![
        summary("before"),
        summary("stash-1"),
        summary("stash-3"),
        summary("stash-2"),
        summary("after"),
    ];
    let mut stashes = StashGraph::default();
    for index in [1, 3, 2] {
        stashes.commits.insert(
            format!("stash-{index}"),
            StashCommit {
                stash_oid: format!("outer-{index}"),
                base_oid: Some("base".into()),
                reference: StashRef {
                    index,
                    selector: format!("stash@{{{index}}}"),
                    oid: format!("outer-{index}"),
                },
                label: format!("stash {index}"),
            },
        );
    }

    apply_stash_view(&mut commits, &stashes);

    assert_eq!(
        commits
            .iter()
            .map(|commit| commit.oid.as_str())
            .collect::<Vec<_>>(),
        vec!["before", "stash-1", "stash-3", "stash-2", "after"]
    );
}

#[tokio::test]
async fn history_cursor_tracks_refs_and_wip_visibility() {
    let (directory, backend, first_oid) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "second\n").expect("write second version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage second version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "second commit".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("second commit");
    let first_page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .expect("first history page");
    let cursor = first_page.next_cursor.expect("next cursor");

    fs::write(directory.path().join("uncommitted.txt"), "worktree only\n")
        .expect("write worktree-only file");
    let error = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(cursor),
                limit: 1,
            },
        )
        .await
        .expect_err("clean-to-dirty transition invalidates history cursor");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);

    let dirty_page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .expect("dirty first history page");
    let dirty_cursor = dirty_page.next_cursor.expect("dirty next cursor");

    fs::write(
        directory.path().join("uncommitted.txt"),
        "different worktree contents\n",
    )
    .expect("change dirty worktree without making it clean");
    backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(dirty_cursor.clone()),
                limit: 1,
            },
        )
        .await
        .expect("dirty-to-dirty edit keeps history cursor valid");

    fs::remove_file(directory.path().join("uncommitted.txt")).expect("restore clean worktree");
    let error = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(dirty_cursor),
                limit: 1,
            },
        )
        .await
        .expect_err("dirty-to-clean transition invalidates history cursor");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);

    let clean_page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .expect("new clean history page");
    let clean_cursor = clean_page.next_cursor.expect("new clean cursor");

    backend
        .create_branch(directory.path(), "new-ref", &first_oid, false)
        .await
        .expect("create ref");
    let error = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(clean_cursor),
                limit: 1,
            },
        )
        .await
        .expect_err("ref change invalidates cursor");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
}

#[tokio::test]
async fn history_layout_follows_wip_and_checkout_state() {
    let (directory, backend, base_oid) = committed_repository().await;
    backend
        .create_branch(directory.path(), "feature", &base_oid, false)
        .await
        .expect("create sibling feature branch");

    fs::write(directory.path().join("main.txt"), "main\n").expect("write main change");
    git(directory.path(), &["add", "--", "main.txt"]);
    git_with_date(
        directory.path(),
        &["commit", "--quiet", "--message", "fixture: main tip"],
        "2030-01-01T00:00:00 +0000",
    );
    let main_oid = git_stdout(directory.path(), &["rev-parse", "HEAD"]);

    git(directory.path(), &["checkout", "--quiet", "feature"]);
    fs::write(directory.path().join("feature.txt"), "feature\n").expect("write feature change");
    git(directory.path(), &["add", "--", "feature.txt"]);
    git_with_date(
        directory.path(),
        &["commit", "--quiet", "--message", "fixture: feature tip"],
        "2031-01-01T00:00:00 +0000",
    );
    let feature_oid = git_stdout(directory.path(), &["rev-parse", "HEAD"]);
    git(directory.path(), &["checkout", "--quiet", "main"]);

    let query = HistoryQuery {
        scope: HistoryScope::AllRefs,
        cursor: None,
        limit: 50,
    };
    let clean = backend
        .history(directory.path(), &query)
        .await
        .expect("clean sibling history");
    assert_eq!(
        clean
            .commits
            .iter()
            .map(|commit| commit.oid.as_str())
            .collect::<Vec<_>>(),
        vec![feature_oid.as_str(), main_oid.as_str(), base_oid.as_str()]
    );
    assert_eq!(
        clean
            .commits
            .iter()
            .map(|commit| commit.graph.lane)
            .collect::<Vec<_>>(),
        vec![0, 1, 0]
    );
    assert_eq!(clean.commits[0].graph.edges[0].to_lane, 0);
    assert_eq!(clean.commits[1].graph.edges[0].to_lane, 0);

    fs::write(directory.path().join("wip.txt"), "untracked WIP\n").expect("write WIP");
    let main_wip = backend
        .history(directory.path(), &query)
        .await
        .expect("main WIP sibling history");
    assert_eq!(
        main_wip
            .commits
            .iter()
            .map(|commit| commit.graph.lane)
            .collect::<Vec<_>>(),
        vec![1, 0, 0]
    );
    assert_eq!(main_wip.commits[0].graph.edges[0].to_lane, 0);
    assert_eq!(main_wip.commits[1].graph.edges[0].to_lane, 0);

    backend
        .checkout_branch(directory.path(), "feature")
        .await
        .expect("checkout feature with WIP");
    let feature_wip = backend
        .history(directory.path(), &query)
        .await
        .expect("feature WIP sibling history");
    assert_eq!(
        feature_wip
            .commits
            .iter()
            .map(|commit| commit.graph.lane)
            .collect::<Vec<_>>(),
        vec![0, 1, 0]
    );
    assert_eq!(feature_wip.commits[0].graph.edges[0].to_lane, 0);
    assert_eq!(feature_wip.commits[1].graph.edges[0].to_lane, 0);
}

#[tokio::test]
async fn history_preserves_git_log_order_for_stash_rows() {
    let (directory, backend, _) = committed_repository().await;
    for (label, date, contents) in [
        ("oracle: stash A", "2000-01-01T03:00:00 +0000", "stash A\n"),
        ("oracle: stash B", "2000-01-01T01:00:00 +0000", "stash B\n"),
        ("oracle: stash C", "2000-01-01T02:00:00 +0000", "stash C\n"),
    ] {
        fs::write(directory.path().join("hello.txt"), contents).expect("write stash fixture");
        git_with_date(
            directory.path(),
            &["stash", "push", "--message", label],
            date,
        );
    }

    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("history with dated stashes");
    let stashes = page
        .commits
        .iter()
        .filter_map(|commit| {
            commit
                .stash
                .as_ref()
                .map(|stash| (commit.subject.as_str(), stash.selector.as_str()))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        stashes,
        vec![
            ("oracle: stash A", "stash@{2}"),
            ("oracle: stash C", "stash@{0}"),
            ("oracle: stash B", "stash@{1}"),
        ]
    );
}

#[tokio::test]
async fn history_cursor_tracks_stash_identity_and_selectors() {
    let (directory, backend, _) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "second\n").expect("write second version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage second version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "second commit".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("second commit");

    for (label, contents) in [("stash A", "stash A\n"), ("stash B", "stash B\n")] {
        fs::write(directory.path().join("hello.txt"), contents).expect("write stash change");
        backend
            .stash_push(directory.path(), Some(label), false)
            .await
            .expect("create stash");
    }

    let first_page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .expect("first page with stashes");
    let cursor = first_page.next_cursor.expect("stash history cursor");

    git(directory.path(), &["stash", "drop", "stash@{0}"]);
    let error = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(cursor),
                limit: 1,
            },
        )
        .await
        .expect_err("stash deletion and selector shift invalidate cursor");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
}

#[tokio::test]
async fn stash_details_and_diff_compare_the_base_to_the_outer_stash() {
    let (directory, backend, base_oid) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "unstaged-only change\n")
        .expect("write unstaged-only stash change");
    backend
        .stash_push(directory.path(), Some("unstaged-only stash"), false)
        .await
        .expect("stash unstaged-only change");
    let visible_index_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);
    let outer_stash_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let visible_short_oid = git_stdout(
        directory.path(),
        &["rev-parse", "--short", &visible_index_oid],
    );
    let outer_tree_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^{tree}"]);
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "stash@{0}^1"]),
        base_oid
    );
    assert!(
        git_stdout(
            directory.path(),
            &["diff", "--name-only", &base_oid, &visible_index_oid]
        )
        .is_empty(),
        "the visible index parent intentionally has no staged diff"
    );
    assert_eq!(
        git_stdout(
            directory.path(),
            &["diff", "--name-only", &base_oid, &outer_stash_oid]
        ),
        "hello.txt"
    );

    let details = backend
        .commit_details(directory.path(), &visible_index_oid, 0)
        .await
        .expect("stash details");
    assert_eq!(details.oid, visible_index_oid);
    assert_eq!(details.short_oid, visible_short_oid);
    assert_eq!(details.tree_oid, outer_tree_oid);
    assert_eq!(details.parent_oids, vec![base_oid.clone()]);
    assert_eq!(details.subject, "unstaged-only stash");
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].new_path, "hello.txt");
    assert_eq!(details.files[0].status, ChangeKind::Modified);

    let diff = backend
        .diff(
            directory.path(),
            &DiffRequest {
                target: DiffTarget::Commit {
                    oid: visible_index_oid,
                    parent_index: 0,
                },
                path: "hello.txt".into(),
                context_lines: 3,
                ignore_whitespace: false,
                max_bytes: 1024 * 1024,
                whole_file: false,
            },
        )
        .await
        .expect("stash file diff");
    assert_eq!(diff.status, ChangeKind::Modified);
    assert_eq!(diff.stats.files, 1);
    assert!(!diff.hunks.is_empty());
}

#[tokio::test]
async fn whole_file_diff_keeps_every_line_as_context() {
    let (directory, backend, _oid) = committed_repository().await;
    let mut lines: Vec<String> = (1..=40).map(|number| format!("line {number}")).collect();
    fs::write(
        directory.path().join("hello.txt"),
        format!("{}\n", lines.join("\n")),
    )
    .expect("write long fixture");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage long fixture");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "long fixture".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit long fixture");

    lines[19] = "line 20 changed".into();
    fs::write(
        directory.path().join("hello.txt"),
        format!("{}\n", lines.join("\n")),
    )
    .expect("modify long fixture");

    let request = |whole_file| DiffRequest {
        target: DiffTarget::Worktree,
        path: "hello.txt".into(),
        context_lines: 3,
        ignore_whitespace: false,
        max_bytes: 1024 * 1024,
        whole_file,
    };

    let hunked = backend
        .diff(directory.path(), &request(false))
        .await
        .expect("hunk diff");
    let hunked_lines: usize = hunked.hunks.iter().map(|hunk| hunk.lines.len()).sum();
    assert!(hunked_lines < 20);

    let whole = backend
        .diff(directory.path(), &request(true))
        .await
        .expect("whole file diff");
    assert_eq!(whole.hunks.len(), 1);
    assert_eq!(whole.hunks[0].lines.len(), 41);
    assert_eq!(whole.hunks[0].old_start, 1);
    assert_eq!(whole.hunks[0].new_start, 1);
}

#[tokio::test]
async fn conflicted_worktree_file_diffs_against_the_ours_stage() {
    let (directory, backend) = conflicted_repository().await;

    let diff = backend
        .diff(
            directory.path(),
            &DiffRequest {
                target: DiffTarget::Worktree,
                path: "hello.txt".into(),
                context_lines: 3,
                ignore_whitespace: false,
                max_bytes: 1024 * 1024,
                whole_file: false,
            },
        )
        .await
        .expect("an unmerged path diffs instead of tripping the single-file guard");

    assert_eq!(diff.new_path, "hello.txt");
    assert!(!diff.hunks.is_empty());
    assert!(
        diff.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .any(|line| line.kind == DiffLineKind::Addition && line.content.starts_with("<<<<<<<")),
        "the ours-side patch shows the conflict markers the working copy gained",
    );
}

#[tokio::test]
async fn worktree_diff_branch_and_stash_workflow() {
    let (directory, backend, oid) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "first\nsecond\n").expect("modify fixture");
    let diff = backend
        .diff(
            directory.path(),
            &DiffRequest {
                target: DiffTarget::Worktree,
                path: "hello.txt".into(),
                context_lines: 3,
                ignore_whitespace: false,
                max_bytes: 1024 * 1024,
                whole_file: false,
            },
        )
        .await
        .expect("worktree diff");
    assert_eq!(diff.stats.additions, 1);
    assert_eq!(diff.hunks.len(), 1);
    let error = backend
        .diff(
            directory.path(),
            &DiffRequest {
                target: DiffTarget::Worktree,
                path: ".".into(),
                context_lines: 3,
                ignore_whitespace: false,
                max_bytes: 1024 * 1024,
                whole_file: false,
            },
        )
        .await
        .expect_err("directory path rejected by single-file diff API");
    assert_eq!(error.code, ErrorCode::InvalidRequest);

    backend
        .stash_push(directory.path(), Some("test stash"), false)
        .await
        .expect("stash changes");
    let stashes = backend
        .stash_list(directory.path())
        .await
        .expect("list stashes");
    assert_eq!(stashes.len(), 1);
    backend
        .stash_apply(directory.path(), &stashes[0].oid, true)
        .await
        .expect("pop stash");

    backend
        .create_branch(directory.path(), "feature/test", &oid, false)
        .await
        .expect("create branch");
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("branch snapshot");
    assert!(
        snapshot
            .local_branches
            .iter()
            .any(|branch| branch.name == "feature/test")
    );
    let error = backend
        .delete_branch(directory.path(), "feature/test", true, false)
        .await
        .expect_err("force delete needs confirmation");
    assert_eq!(error.code, ErrorCode::ProtectedOperation);
    let error = backend
        .reset_to_commit(directory.path(), &oid, ResetMode::Hard, false)
        .await
        .expect_err("hard reset needs confirmation");
    assert_eq!(error.code, ErrorCode::ProtectedOperation);
}

#[tokio::test]
async fn worktree_diff_reports_untracked_file_contents() {
    let (directory, backend, _) = committed_repository().await;
    fs::write(directory.path().join("fresh.txt"), "alpha\nbeta\n").expect("write new file");
    let diff = backend
        .diff(
            directory.path(),
            &DiffRequest {
                target: DiffTarget::Worktree,
                path: "fresh.txt".into(),
                context_lines: 3,
                ignore_whitespace: false,
                max_bytes: 1024 * 1024,
                whole_file: false,
            },
        )
        .await
        .expect("untracked worktree diff");
    assert_eq!(diff.status, ChangeKind::Added);
    assert_eq!(diff.stats.additions, 2);
    assert_eq!(diff.stats.deletions, 0);
    assert_eq!(diff.hunks.len(), 1);
    assert_eq!(diff.new_path, "fresh.txt");
}

#[tokio::test]
async fn snapshot_reports_line_stats_for_staged_worktree_and_untracked_changes() {
    let directory = tempdir().expect("temp repository");
    let backend = GitCliBackend::default();
    backend
        .init_repository(directory.path(), "main")
        .await
        .expect("initialize repository");
    git(directory.path(), &["config", "user.name", "GitCat Test"]);
    git(
        directory.path(),
        &["config", "user.email", "gitcat@example.test"],
    );
    fs::write(directory.path().join("tracked.txt"), "one\ntwo\n").expect("write tracked file");
    backend
        .stage_paths(directory.path(), &["tracked.txt".into()])
        .await
        .expect("stage tracked file");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "chore: seed".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("seed commit");

    fs::write(directory.path().join("staged.txt"), "alpha\nbeta\n").expect("write staged file");
    backend
        .stage_paths(directory.path(), &["staged.txt".into()])
        .await
        .expect("stage new file");
    fs::write(directory.path().join("tracked.txt"), "one\ntwo\nthree\n")
        .expect("modify tracked file");
    fs::write(directory.path().join("fresh.txt"), "new\nlines\nhere\n")
        .expect("write untracked file");

    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot with stats");
    let stats = |path: &str| {
        snapshot
            .status
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .cloned()
            .unwrap_or_else(|| panic!("status entry for {path}"))
    };
    assert_eq!(
        stats("staged.txt").index_stats,
        Some(LineStats {
            additions: 2,
            deletions: 0
        })
    );
    assert_eq!(
        stats("tracked.txt").worktree_stats,
        Some(LineStats {
            additions: 1,
            deletions: 0
        })
    );
    assert_eq!(
        stats("fresh.txt").worktree_stats,
        Some(LineStats {
            additions: 3,
            deletions: 0
        })
    );
}

#[tokio::test]
async fn unstage_in_unborn_repository_preserves_modified_worktree_file() {
    let directory = tempdir().expect("temp repository");
    let backend = GitCliBackend::default();
    backend
        .init_repository(directory.path(), "main")
        .await
        .expect("initialize repository");
    fs::write(directory.path().join("draft.txt"), "staged\n").expect("write staged version");
    backend
        .stage_paths(directory.path(), &["draft.txt".into()])
        .await
        .expect("stage draft");
    fs::write(directory.path().join("draft.txt"), "worktree changed\n")
        .expect("change worktree version");

    backend
        .unstage_paths(directory.path(), &["draft.txt".into()])
        .await
        .expect("unstage draft");
    assert_eq!(
        fs::read_to_string(directory.path().join("draft.txt")).expect("read worktree file"),
        "worktree changed\n"
    );
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("unborn snapshot");
    assert_eq!(snapshot.status.entries.len(), 1);
    assert_eq!(
        snapshot.status.entries[0].worktree,
        Some(ChangeKind::Untracked)
    );
}

// The offer to make the first commit is drawn where the graph would be, so it
// only appears if an empty repository pages as an empty history rather than as
// an error.
#[tokio::test]
async fn history_of_an_unborn_repository_is_an_empty_page() {
    let directory = tempdir().expect("temp repository");
    let backend = GitCliBackend::default();
    backend
        .init_repository(directory.path(), "main")
        .await
        .expect("initialize repository");

    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                limit: 50,
                cursor: None,
            },
        )
        .await
        .expect("history of an unborn repository");

    assert!(page.commits.is_empty());
    assert!(!page.has_more);
}

#[tokio::test]
async fn initial_commit_seeds_a_readme_when_nothing_is_staged() {
    let directory = tempdir().expect("temp repository");
    let backend = GitCliBackend::default();
    backend
        .init_repository(directory.path(), "main")
        .await
        .expect("initialize repository");
    git(directory.path(), &["config", "user.name", "GitCat Test"]);
    git(
        directory.path(),
        &["config", "user.email", "gitcat@example.test"],
    );

    backend
        .create_initial_commit(directory.path(), "Initial commit")
        .await
        .expect("create the initial commit");

    let name = directory
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .expect("temp directory name");
    assert_eq!(
        fs::read_to_string(directory.path().join("README.md")).expect("read seeded README"),
        format!("# {name}\n")
    );
    assert_eq!(
        git_stdout(directory.path(), &["log", "-1", "--format=%s"]),
        "Initial commit"
    );
    assert_eq!(
        git_stdout(
            directory.path(),
            &["show", "--name-only", "--format=", "HEAD"]
        ),
        "README.md"
    );
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot after the initial commit");
    assert!(snapshot.status.clean);
    assert!(matches!(snapshot.head, HeadState::Branch { .. }));
}

#[tokio::test]
async fn initial_commit_takes_the_staged_files_and_seeds_no_readme() {
    let directory = tempdir().expect("temp repository");
    let backend = GitCliBackend::default();
    backend
        .init_repository(directory.path(), "main")
        .await
        .expect("initialize repository");
    git(directory.path(), &["config", "user.name", "GitCat Test"]);
    git(
        directory.path(),
        &["config", "user.email", "gitcat@example.test"],
    );
    fs::write(directory.path().join("main.rs"), "fn main() {}\n").expect("write staged file");
    fs::write(directory.path().join("notes.txt"), "later\n").expect("write untracked file");
    backend
        .stage_paths(directory.path(), &["main.rs".into()])
        .await
        .expect("stage the file");

    backend
        .create_initial_commit(directory.path(), "Initial commit")
        .await
        .expect("create the initial commit");

    assert!(!directory.path().join("README.md").exists());
    assert_eq!(
        git_stdout(
            directory.path(),
            &["show", "--name-only", "--format=", "HEAD"]
        ),
        "main.rs"
    );
    // The rest of the working tree is the user's to stage in their own order.
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot after the initial commit");
    assert_eq!(snapshot.status.entries.len(), 1);
    assert_eq!(snapshot.status.entries[0].path, "notes.txt");
}

#[tokio::test]
async fn initial_commit_keeps_an_existing_readme() {
    let directory = tempdir().expect("temp repository");
    let backend = GitCliBackend::default();
    backend
        .init_repository(directory.path(), "main")
        .await
        .expect("initialize repository");
    git(directory.path(), &["config", "user.name", "GitCat Test"]);
    git(
        directory.path(),
        &["config", "user.email", "gitcat@example.test"],
    );
    fs::write(directory.path().join("README.md"), "# Written by hand\n").expect("write README");

    backend
        .create_initial_commit(directory.path(), "Initial commit")
        .await
        .expect("create the initial commit");

    assert_eq!(
        fs::read_to_string(directory.path().join("README.md")).expect("read README"),
        "# Written by hand\n"
    );
}

#[tokio::test]
async fn initial_commit_rejects_a_repository_that_has_one() {
    let (directory, backend, _) = committed_repository().await;

    let error = backend
        .create_initial_commit(directory.path(), "Initial commit")
        .await
        .expect_err("a repository with a commit is refused");

    assert_eq!(error.code, ErrorCode::InvalidRequest);
}

#[tokio::test]
async fn reset_rejects_detached_head() {
    let (directory, backend, oid) = committed_repository().await;
    backend
        .checkout_commit(directory.path(), &oid)
        .await
        .expect("detach HEAD");
    let error = backend
        .reset_to_commit(directory.path(), &oid, ResetMode::Mixed, true)
        .await
        .expect_err("detached reset rejected");
    assert_eq!(error.code, ErrorCode::UnsupportedOperation);
}

#[tokio::test]
async fn commit_action_availability_matches_current_head_context() {
    let (directory, backend, base_oid) = committed_repository().await;
    backend
        .create_branch(directory.path(), "side", &base_oid, true)
        .await
        .expect("create side branch");
    fs::write(directory.path().join("side.txt"), "side\n").expect("write side file");
    backend
        .stage_paths(directory.path(), &["side.txt".into()])
        .await
        .expect("stage side file");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "side commit".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit side file");
    let side_oid = backend
        .head_oid(directory.path())
        .await
        .expect("read side HEAD")
        .expect("side HEAD exists");
    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main");

    let main_actions = backend
        .commit_action_availability(directory.path(), &base_oid)
        .await
        .expect("main commit actions");
    fn action(
        actions: &[CommitActionAvailability],
        kind: CommitActionKind,
    ) -> &CommitActionAvailability {
        actions
            .iter()
            .find(|action| action.kind == kind)
            .expect("action exists")
    }
    assert!(action(&main_actions, CommitActionKind::Checkout).enabled);
    assert!(action(&main_actions, CommitActionKind::CreateBranch).enabled);
    assert!(action(&main_actions, CommitActionKind::CreateTag).enabled);
    assert!(action(&main_actions, CommitActionKind::Reset).enabled);
    assert!(!action(&main_actions, CommitActionKind::CherryPick).enabled);
    assert!(
        action(&main_actions, CommitActionKind::CherryPick)
            .disabled_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("already in the current HEAD history"))
    );
    assert!(action(&main_actions, CommitActionKind::Revert).enabled);
    assert!(action(&main_actions, CommitActionKind::Reword).enabled);

    let side_actions = backend
        .commit_action_availability(directory.path(), &side_oid)
        .await
        .expect("side commit actions");
    assert!(action(&side_actions, CommitActionKind::CherryPick).enabled);
    assert!(!action(&side_actions, CommitActionKind::Revert).enabled);
    assert!(
        action(&side_actions, CommitActionKind::Revert)
            .disabled_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("not in the current HEAD history"))
    );
    assert!(!action(&side_actions, CommitActionKind::Reword).enabled);
    assert!(
        action(&side_actions, CommitActionKind::Reword)
            .disabled_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("reachable from the current branch"))
    );

    fs::write(directory.path().join("dirty.txt"), "dirty\n").expect("write dirty file");
    let dirty_actions = backend
        .commit_action_availability(directory.path(), &side_oid)
        .await
        .expect("dirty commit actions");
    assert!(!action(&dirty_actions, CommitActionKind::Checkout).enabled);
    assert!(!action(&dirty_actions, CommitActionKind::CherryPick).enabled);
    assert!(!action(&dirty_actions, CommitActionKind::Revert).enabled);
    assert!(action(&dirty_actions, CommitActionKind::CreateBranch).enabled);
    assert!(action(&dirty_actions, CommitActionKind::CreateTag).enabled);
    assert!(action(&dirty_actions, CommitActionKind::Reset).enabled);
    assert!(action(&dirty_actions, CommitActionKind::CopySha).enabled);
    // Still the off-branch commit, so rewording stays unavailable.
    assert!(!action(&dirty_actions, CommitActionKind::Reword).enabled);
}

#[test]
fn mainline_parent_is_required_only_for_merge_commits() {
    assert!(validate_mainline_parent(2, Some(1)).is_ok());
    assert_eq!(
        validate_mainline_parent(2, None).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        validate_mainline_parent(1, Some(1)).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        validate_mainline_parent(2, Some(3)).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
}

#[tokio::test]
async fn clones_shallow_and_sparse_from_the_dialog_options() {
    let (source, backend, _) = committed_repository().await;
    fs::create_dir_all(source.path().join("apps/desktop")).expect("fixture directory");
    fs::create_dir_all(source.path().join("crates/core")).expect("fixture directory");
    fs::write(source.path().join("apps/desktop/app.txt"), "app\n").expect("write fixture");
    fs::write(source.path().join("crates/core/core.txt"), "core\n").expect("write fixture");
    git(source.path(), &["add", "-A"]);
    git(source.path(), &["commit", "-m", "second"]);

    let parent = tempdir().expect("clone parent");
    let destination = parent.path().join("clone");
    backend
        .clone_repository(
            &CloneOptions {
                // A local path clone silently ignores --depth; the file
                // transport is what a shallow clone really goes through.
                url: format!(
                    "file:///{}",
                    source.path().to_string_lossy().replace('\\', "/")
                ),
                destination: destination.to_string_lossy().into_owned(),
                branch: None,
                depth: Some(1),
                filter_blob_none: false,
                sparse_paths: Some(vec!["apps/desktop".into()]),
            },
            CancellationToken::new(),
        )
        .await
        .expect("clone with dialog options");

    let output = Command::new("git")
        .arg("-C")
        .arg(&destination)
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .expect("count cloned commits");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");
    assert!(destination.join("apps/desktop/app.txt").is_file());
    assert!(!destination.join("crates").exists());
    assert!(destination.join("hello.txt").is_file());
}

#[test]
fn sparse_paths_become_repository_rooted_cone_patterns() {
    assert_eq!(
        sparse_patterns(&[
            "apps/desktop".into(),
            r"  crates\gitcat-core  ".into(),
            "./docs/".into(),
            "   ".into(),
        ])
        .expect("patterns"),
        vec![
            "/apps/desktop".to_string(),
            "/crates/gitcat-core".to_string(),
            "/docs".to_string(),
        ]
    );
    assert!(sparse_patterns(&[]).expect("patterns").is_empty());
}

#[test]
fn sparse_paths_cannot_leave_the_repository() {
    // A leading slash is the repository root in a sparse pattern, not the
    // file system root, so it is normalised rather than refused.
    for path in ["../secrets", "apps/../../etc", r"C:\secrets"] {
        assert_eq!(
            sparse_patterns(&[path.into()]).unwrap_err().code,
            ErrorCode::InvalidPath
        );
    }
}

#[tokio::test]
async fn rejects_ext_remote_before_spawning_clone() {
    let destination = tempdir().expect("destination parent");
    let backend = GitCliBackend::default();
    let error = backend
        .clone_repository(
            &CloneOptions {
                url: "ext::sh -c bad".into(),
                destination: destination
                    .path()
                    .join("clone")
                    .to_string_lossy()
                    .into_owned(),
                branch: None,
                depth: None,
                filter_blob_none: false,
                sparse_paths: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect_err("ext transport rejected");
    assert_eq!(error.code, ErrorCode::ProtectedOperation);
}

#[tokio::test]
async fn rejects_custom_remote_helpers_and_ambiguous_push_selection() {
    let destination = tempdir().expect("destination parent");
    let backend = GitCliBackend::default();
    for url in [
        "helper::payload",
        "--upload-pack=bad",
        "https://host/repo\nnext",
    ] {
        let error = backend
            .clone_repository(
                &CloneOptions {
                    url: url.into(),
                    destination: destination
                        .path()
                        .join("clone")
                        .to_string_lossy()
                        .into_owned(),
                    branch: None,
                    depth: None,
                    filter_blob_none: false,
                    sparse_paths: None,
                },
                CancellationToken::new(),
            )
            .await
            .expect_err("unsafe transport rejected");
        assert_eq!(error.code, ErrorCode::ProtectedOperation);
    }

    let (directory, backend, _) = committed_repository().await;
    let error = backend
        .push(
            directory.path(),
            &PushOptions {
                remote: None,
                branch: Some("main".into()),
                set_upstream: false,
                force: PushForce::None,
            },
            CancellationToken::new(),
        )
        .await
        .expect_err("branch without remote rejected");
    assert_eq!(error.code, ErrorCode::InvalidRequest);
}

#[tokio::test]
async fn rejects_bare_repository_with_stable_error() {
    let bare = tempdir().expect("bare repository");
    git(bare.path(), &["init", "--bare", "--quiet"]);
    let error = GitCliBackend::default()
        .open_repository(bare.path())
        .await
        .expect_err("bare repository rejected");
    assert_eq!(error.code, ErrorCode::UnsupportedOperation);
}

/// Because each fetch command names one remote, it reaches a single host and
/// carries that host's token -- even in a repository whose remotes sit on
/// different hosts, where a combined `fetch --all` could offer no token at all.
#[tokio::test]
async fn fetch_authenticates_each_remote_by_its_own_host() {
    #[derive(Default)]
    struct RecordingCredentials {
        hosts: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl crate::credentials::GitCredentialSource for RecordingCredentials {
        async fn token_for(&self, host: &str) -> Option<String> {
            self.hosts
                .lock()
                .expect("record host")
                .push(host.to_owned());
            Some("token".into())
        }
    }

    let (directory, _, _) = committed_repository().await;
    let credentials = Arc::new(RecordingCredentials::default());
    let backend = GitCliBackend::default().with_credentials(credentials.clone());
    // Two hosts that refuse a connection at once: the fetch is expected to
    // fail, and what is under test is the credential it asked for first.
    git(
        directory.path(),
        &["remote", "add", "origin", "https://127.0.0.1:9/one.git"],
    );
    git(
        directory.path(),
        &["remote", "add", "mirror", "https://localhost:9/two.git"],
    );

    backend
        .fetch(
            directory.path(),
            &FetchOptions {
                remote: None,
                prune: false,
                tags: false,
            },
            CancellationToken::new(),
        )
        .await
        .expect_err("unreachable remote");

    let hosts = credentials.hosts.lock().expect("recorded hosts").clone();
    assert_eq!(
        hosts.len(),
        1,
        "a fetch asks for one host per remote, got {hosts:?}"
    );
    assert!(
        hosts[0] == "127.0.0.1" || hosts[0] == "localhost",
        "unexpected host {hosts:?}"
    );
}

/// A fetch with no remote selected runs one command per remote, so a
/// repository with several remotes updates all of them -- and each command
/// reaches a single host, which is what lets it carry that host's token.
#[tokio::test]
async fn fetch_without_a_remote_updates_every_remote() {
    let (directory, backend, _) = committed_repository().await;
    let bares: Vec<_> = ["origin", "mirror"]
        .into_iter()
        .map(|name| {
            let bare = tempdir().expect("bare remote");
            git(bare.path(), &["init", "--bare", "--quiet"]);
            let bare_path = bare.path().to_string_lossy().into_owned();
            git(directory.path(), &["remote", "add", name, &bare_path]);
            git(directory.path(), &["push", "--quiet", name, "main"]);
            bare
        })
        .collect();
    for name in ["origin", "mirror"] {
        git(
            directory.path(),
            &["update-ref", "-d", &format!("refs/remotes/{name}/main")],
        );
    }

    backend
        .fetch(
            directory.path(),
            &FetchOptions {
                remote: None,
                prune: false,
                tags: false,
            },
            CancellationToken::new(),
        )
        .await
        .expect("fetch every remote");

    for name in ["origin", "mirror"] {
        assert!(
            !git_stdout(
                directory.path(),
                &[
                    "rev-parse",
                    "--verify",
                    &format!("refs/remotes/{name}/main")
                ],
            )
            .is_empty(),
            "{name} was not fetched"
        );
    }
    drop(bares);
}

#[tokio::test]
async fn local_remote_fetch_pull_and_push_use_explicit_modes() {
    let (directory, backend, _) = committed_repository().await;
    let bare = tempdir().expect("bare remote");
    git(bare.path(), &["init", "--bare", "--quiet"]);
    let bare_path = bare.path().to_string_lossy().into_owned();
    git(directory.path(), &["remote", "add", "origin", &bare_path]);

    backend
        .push(
            directory.path(),
            &PushOptions {
                remote: Some("origin".into()),
                branch: Some("main".into()),
                set_upstream: true,
                force: PushForce::None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("initial push");
    backend
        .fetch(
            directory.path(),
            &FetchOptions {
                remote: None,
                prune: true,
                tags: false,
            },
            CancellationToken::new(),
        )
        .await
        .expect("fetch all");
    backend
        .pull(
            directory.path(),
            &PullOptions {
                remote: Some("origin".into()),
                branch: Some("main".into()),
                mode: PullMode::Rebase,
                prune: true,
                autostash: false,
            },
            CancellationToken::new(),
        )
        .await
        .expect("explicit rebase pull");

    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("remote snapshot");
    assert_eq!(snapshot.remotes.len(), 1);
    assert!(
        snapshot
            .remote_branches
            .iter()
            .any(|branch| branch.name == "origin/main")
    );
    assert_eq!(
        snapshot.default_conflict_target.as_deref(),
        Some("origin/main")
    );
    git(directory.path(), &["branch", "--unset-upstream"]);
    git(directory.path(), &["remote", "set-head", "origin", "main"]);
    let symbolic_default = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot with symbolic remote HEAD");
    assert_eq!(
        symbolic_default.default_conflict_target.as_deref(),
        Some("origin/main")
    );
    assert!(
        symbolic_default
            .remote_branches
            .iter()
            .all(|branch| branch.name != "origin/HEAD"),
        "symbolic remote HEAD must not render as a normal branch"
    );

    let updater_parent = tempdir().expect("updater parent");
    git(
        updater_parent.path(),
        &[
            "clone", "--quiet", "--branch", "main", &bare_path, "updater",
        ],
    );
    let updater = updater_parent.path().join("updater");
    git(&updater, &["config", "user.name", "GitCat Updater"]);
    git(&updater, &["config", "user.email", "updater@example.test"]);
    fs::write(updater.join("remote.txt"), "remote change\n").expect("write remote change");
    git(&updater, &["add", "--", "remote.txt"]);
    git(&updater, &["commit", "--quiet", "-m", "remote change"]);
    git(&updater, &["push", "--quiet", "origin", "main"]);
    fs::write(directory.path().join("hello.txt"), "local dirty change\n")
        .expect("write local dirty change");

    // Explicit Merge mode must override pull.ff=false and still fast-forward
    // when possible, matching GitKraken's "fast-forward if possible" choice.
    // Autostash lets the pull proceed without forcing the user to manually
    // stash and pop unrelated working-tree changes.
    git(directory.path(), &["config", "pull.ff", "false"]);
    backend
        .pull(
            directory.path(),
            &PullOptions {
                remote: Some("origin".into()),
                branch: Some("main".into()),
                mode: PullMode::Merge,
                prune: false,
                autostash: true,
            },
            CancellationToken::new(),
        )
        .await
        .expect("explicit merge-mode pull");
    let restored_dirty_change = fs::read_to_string(directory.path().join("hello.txt"))
        .expect("read restored dirty change")
        .replace("\r\n", "\n");
    assert_eq!(
        restored_dirty_change, "local dirty change\n",
        "autostash should restore the user's working-tree edit"
    );
    let head = backend
        .head_oid(directory.path())
        .await
        .expect("read pulled HEAD")
        .expect("pulled HEAD exists");
    let details = backend
        .commit_details(directory.path(), &head, 0)
        .await
        .expect("pulled commit details");
    assert_eq!(details.subject, "remote change");
    assert_eq!(details.parent_oids.len(), 1, "pull should fast-forward");
}

#[tokio::test]
async fn conflict_details_exposes_three_index_versions_and_worktree_result() {
    let (directory, backend) = conflicted_repository().await;
    let details = backend
        .conflict_details(directory.path(), "hello.txt")
        .await
        .expect("read three-way conflict");

    let base = details.base.as_ref().expect("base stage");
    let ours = details.ours.as_ref().expect("ours stage");
    let theirs = details.theirs.as_ref().expect("theirs stage");
    assert_eq!(base.content.kind, ConflictContentKind::Text);
    assert_eq!(base.content.text.as_deref(), Some("first\n"));
    assert_eq!(ours.content.text.as_deref(), Some("main version\n"));
    assert_eq!(theirs.content.text.as_deref(), Some("branch version\n"));
    assert_eq!(details.result.kind, ConflictContentKind::Text);
    assert!(
        details
            .result
            .text
            .as_deref()
            .is_some_and(|text| text.contains("<<<<<<<") && text.contains(">>>>>>>"))
    );
    assert_eq!(
        details.expected_state.ours.as_ref().map(|stage| &stage.oid),
        Some(&ours.oid)
    );
    assert_eq!(
        details
            .expected_state
            .theirs
            .as_ref()
            .map(|stage| &stage.mode),
        Some(&theirs.mode)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn oversized_conflict_result_is_hashed_off_executor_and_not_loaded_as_text() {
    let (directory, backend) = conflicted_repository().await;
    let oversized = vec![b'x'; MAX_CONFLICT_TEXT_BYTES + 64 * 1024];
    fs::write(directory.path().join("hello.txt"), &oversized)
        .expect("write oversized conflict result");

    let details = backend
        .conflict_details(directory.path(), "hello.txt")
        .await
        .expect("inspect oversized conflict result");
    assert_eq!(details.result.kind, ConflictContentKind::TooLarge);
    assert!(details.result.text.is_none());
    assert_eq!(
        details.expected_state.result.size,
        Some(oversized.len() as u64)
    );
    assert_eq!(
        details
            .expected_state
            .result
            .sha256
            .as_deref()
            .map(str::len),
        Some(64)
    );
}

#[tokio::test]
async fn saves_and_stages_edited_conflict_result() {
    let (directory, backend) = conflicted_repository().await;
    let expected = conflict_expected(&backend, directory.path(), "hello.txt").await;
    let result = backend
        .save_conflict_result(
            directory.path(),
            "hello.txt",
            "reviewed combined result\n",
            ConflictLineEndingPolicy::Preserve,
            &expected,
        )
        .await
        .expect("save edited conflict result");

    assert!(result.conflicts.is_empty());
    assert_eq!(
        fs::read_to_string(directory.path().join("hello.txt"))
            .expect("read edited working result")
            .replace("\r\n", "\n"),
        "reviewed combined result\n"
    );
    assert_eq!(
        git_stdout(directory.path(), &["show", ":hello.txt"]),
        "reviewed combined result"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn atomic_edited_save_preserves_existing_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (directory, backend) = conflicted_repository().await;
    let target = directory.path().join("hello.txt");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755))
        .expect("make conflict result executable");
    let details = backend
        .conflict_details(directory.path(), "hello.txt")
        .await
        .expect("read executable conflict result");

    backend
        .save_conflict_result(
            directory.path(),
            "hello.txt",
            "resolved executable\n",
            ConflictLineEndingPolicy::Preserve,
            &details.expected_state,
        )
        .await
        .expect("atomically save executable conflict result");
    assert_eq!(
        fs::metadata(target)
            .expect("read saved permissions")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}

#[cfg(unix)]
#[tokio::test]
async fn edited_save_rejects_mode_changes_after_editor_snapshot() {
    use std::os::unix::fs::PermissionsExt;

    let (directory, backend) = conflicted_repository().await;
    let target = directory.path().join("hello.txt");
    let details = backend
        .conflict_details(directory.path(), "hello.txt")
        .await
        .expect("read conflict before external chmod");
    let original_mode = fs::metadata(&target)
        .expect("read original conflict permissions")
        .permissions()
        .mode()
        & 0o7777;
    let changed_mode = if original_mode & 0o100 == 0 {
        original_mode | 0o100
    } else {
        original_mode & !0o100
    };
    fs::set_permissions(&target, fs::Permissions::from_mode(changed_mode))
        .expect("change executable bit outside the editor");

    let error = backend
        .save_conflict_result(
            directory.path(),
            "hello.txt",
            "must not overwrite chmod\n",
            ConflictLineEndingPolicy::Preserve,
            &details.expected_state,
        )
        .await
        .expect_err("external chmod must make the editor snapshot stale");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
    assert_eq!(
        fs::metadata(&target)
            .expect("read unchanged external permissions")
            .permissions()
            .mode()
            & 0o7777,
        changed_mode
    );
}

#[cfg(unix)]
#[tokio::test]
async fn new_edited_result_creation_respects_process_umask() {
    use std::os::unix::fs::PermissionsExt;

    let (directory, backend) = conflicted_repository().await;
    let mut probe_builder = tempfile::Builder::new();
    probe_builder.permissions(fs::Permissions::from_mode(0o777));
    let probe = probe_builder
        .tempfile_in(directory.path())
        .expect("create umask permission probe");
    let permissions_allowed_by_umask = probe
        .as_file()
        .metadata()
        .expect("read umask permission probe")
        .permissions()
        .mode()
        & 0o777;

    let target = directory.path().join("hello.txt");
    fs::remove_file(&target).expect("remove worktree conflict result");
    let details = backend
        .conflict_details(directory.path(), "hello.txt")
        .await
        .expect("read missing conflict result");
    assert_eq!(
        details.expected_state.result.kind,
        ConflictWorktreeKind::Missing
    );
    let desired_mode = details
        .ours
        .as_ref()
        .or(details.theirs.as_ref())
        .or(details.base.as_ref())
        .expect("regular conflict stage")
        .mode
        .as_str();
    let requested_permissions = if desired_mode == "100755" {
        0o777
    } else {
        0o666
    };

    backend
        .save_conflict_result(
            directory.path(),
            "hello.txt",
            "recreated result\n",
            ConflictLineEndingPolicy::Preserve,
            &details.expected_state,
        )
        .await
        .expect("save recreated conflict result");
    assert_eq!(
        fs::metadata(target)
            .expect("read recreated result permissions")
            .permissions()
            .mode()
            & 0o777,
        requested_permissions & permissions_allowed_by_umask
    );
}

#[tokio::test]
async fn crlf_result_survives_textarea_lf_normalization() {
    let (directory, backend) = conflicted_repository().await;
    git(directory.path(), &["config", "core.autocrlf", "true"]);
    fs::write(
        directory.path().join("hello.txt"),
        b"first line\r\nsecond line\r\n",
    )
    .expect("write CRLF conflict result");
    let details = backend
        .conflict_details(directory.path(), "hello.txt")
        .await
        .expect("read CRLF conflict result");
    assert_eq!(details.result.line_ending, Some(ConflictLineEnding::CrLf));

    backend
        .save_conflict_result(
            directory.path(),
            "hello.txt",
            "first line\nsecond line\n",
            ConflictLineEndingPolicy::Preserve,
            &details.expected_state,
        )
        .await
        .expect("save browser-normalized CRLF result");
    assert_eq!(
        fs::read(directory.path().join("hello.txt")).expect("read CRLF roundtrip"),
        b"first line\r\nsecond line\r\n"
    );
    let staged = Command::new("git")
        .arg("-C")
        .arg(directory.path())
        .args(["show", ":hello.txt"])
        .output()
        .expect("read staged CRLF result");
    assert!(staged.status.success());
    assert_eq!(
        staged.stdout, b"first line\nsecond line\n",
        "Git clean conversion may normalize CRLF in the index; working file must stay CRLF"
    );
}

#[tokio::test]
async fn unstaged_external_edit_makes_all_overwriting_resolutions_stale() {
    let (directory, backend) = conflicted_repository().await;
    let details = backend
        .conflict_details(directory.path(), "hello.txt")
        .await
        .expect("open conflict editor");
    fs::write(
        directory.path().join("hello.txt"),
        "external unstaged resolution\n",
    )
    .expect("external worktree edit");

    for resolution in [
        ConflictResolution::Ours,
        ConflictResolution::Theirs,
        ConflictResolution::MarkResolved,
        ConflictResolution::Delete,
    ] {
        let error = backend
            .resolve_conflict(
                directory.path(),
                "hello.txt",
                resolution,
                &details.expected_state,
            )
            .await
            .expect_err("stale resolution must fail");
        assert_eq!(error.code, ErrorCode::StaleSnapshot);
        assert_eq!(
            fs::read_to_string(directory.path().join("hello.txt"))
                .expect("external edit remains intact"),
            "external unstaged resolution\n"
        );
    }
}

#[tokio::test]
async fn stale_conflict_editor_state_cannot_overwrite_external_resolution() {
    let (directory, backend) = conflicted_repository().await;
    let expected = conflict_expected(&backend, directory.path(), "hello.txt").await;
    git(directory.path(), &["checkout", "--ours", "--", "hello.txt"]);
    git(directory.path(), &["add", "--", "hello.txt"]);

    let error = backend
        .save_conflict_result(
            directory.path(),
            "hello.txt",
            "stale overwrite\n",
            ConflictLineEndingPolicy::Preserve,
            &expected,
        )
        .await
        .expect_err("stale editor save must fail");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
    assert_eq!(
        fs::read_to_string(directory.path().join("hello.txt"))
            .expect("read externally resolved file")
            .replace("\r\n", "\n"),
        "main version\n"
    );
}

#[tokio::test]
async fn explicit_delete_resolves_modify_delete_in_both_directions() {
    let (ours_directory, ours_backend) = modify_delete_conflicted_repository().await;
    let ours_details = ours_backend
        .conflict_details(ours_directory.path(), "hello.txt")
        .await
        .expect("read modified-by-ours conflict");
    assert!(ours_details.ours.is_some());
    assert!(ours_details.theirs.is_none());
    let ours_result = ours_backend
        .resolve_conflict(
            ours_directory.path(),
            "hello.txt",
            ConflictResolution::Delete,
            &ours_details.expected_state,
        )
        .await
        .expect("choose deletion when theirs deleted");
    assert!(ours_result.conflicts.is_empty());
    assert!(!ours_directory.path().join("hello.txt").exists());

    let (theirs_directory, theirs_backend) = delete_modify_conflicted_repository().await;
    let theirs_details = theirs_backend
        .conflict_details(theirs_directory.path(), "hello.txt")
        .await
        .expect("read modified-by-theirs conflict");
    assert!(theirs_details.ours.is_none());
    assert!(theirs_details.theirs.is_some());
    let theirs_result = theirs_backend
        .resolve_conflict(
            theirs_directory.path(),
            "hello.txt",
            ConflictResolution::Delete,
            &theirs_details.expected_state,
        )
        .await
        .expect("choose deletion when ours deleted");
    assert!(theirs_result.conflicts.is_empty());
    assert!(!theirs_directory.path().join("hello.txt").exists());
}

#[test]
fn conflict_content_classification_blocks_binary_and_oversized_text() {
    let binary = classify_conflict_content(vec![b'a', 0, b'b'], 3);
    assert_eq!(binary.kind, ConflictContentKind::Binary);
    assert!(binary.text.is_none());

    let too_large = classify_conflict_content(
        vec![b'a'; MAX_CONFLICT_TEXT_BYTES + 1],
        (MAX_CONFLICT_TEXT_BYTES + 1) as u64,
    );
    assert_eq!(too_large.kind, ConflictContentKind::TooLarge);
    assert!(too_large.text.is_none());

    let mixed = classify_conflict_content(b"one\r\ntwo\n".to_vec(), 9);
    assert_eq!(mixed.line_ending, Some(ConflictLineEnding::Mixed));
    assert_eq!(
        encode_edited_conflict_text("one\ntwo\n", ConflictLineEndingPolicy::Preserve, &mixed,)
            .expect("logical no-op preserves exact mixed endings"),
        b"one\r\ntwo\n"
    );
    assert_eq!(
        encode_edited_conflict_text("one\nchanged\n", ConflictLineEndingPolicy::Preserve, &mixed,)
            .expect_err("mixed edit needs explicit policy")
            .code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        encode_edited_conflict_text("one\nchanged\n", ConflictLineEndingPolicy::Lf, &mixed,)
            .expect("normalize mixed to LF"),
        b"one\nchanged\n"
    );
    assert_eq!(
        encode_edited_conflict_text("one\nchanged\n", ConflictLineEndingPolicy::CrLf, &mixed,)
            .expect("normalize mixed to CRLF"),
        b"one\r\nchanged\r\n"
    );
}

#[tokio::test]
async fn resolves_both_modified_conflict_with_selected_index_side() {
    let (current_directory, current_backend) = conflicted_repository().await;
    let current_expected =
        conflict_expected(&current_backend, current_directory.path(), "hello.txt").await;
    let current_result = current_backend
        .resolve_conflict(
            current_directory.path(),
            "hello.txt",
            ConflictResolution::Ours,
            &current_expected,
        )
        .await
        .expect("resolve with current side");
    assert!(current_result.conflicts.is_empty());
    assert!(
        current_result.needs_user_action,
        "merge still needs completion"
    );
    assert_eq!(
        fs::read_to_string(current_directory.path().join("hello.txt"))
            .expect("read current resolution")
            .replace("\r\n", "\n"),
        "main version\n"
    );
    assert_eq!(
        git_stdout(current_directory.path(), &["show", ":hello.txt"]),
        "main version"
    );
    current_backend
        .abort_operation(current_directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort current-side fixture merge");

    let (incoming_directory, incoming_backend) = conflicted_repository().await;
    let incoming_expected =
        conflict_expected(&incoming_backend, incoming_directory.path(), "hello.txt").await;
    let incoming_result = incoming_backend
        .resolve_conflict(
            incoming_directory.path(),
            "hello.txt",
            ConflictResolution::Theirs,
            &incoming_expected,
        )
        .await
        .expect("resolve with incoming side");
    assert!(incoming_result.conflicts.is_empty());
    assert_eq!(
        fs::read_to_string(incoming_directory.path().join("hello.txt"))
            .expect("read incoming resolution")
            .replace("\r\n", "\n"),
        "branch version\n"
    );
    assert_eq!(
        git_stdout(incoming_directory.path(), &["show", ":hello.txt"]),
        "branch version"
    );
    incoming_backend
        .abort_operation(incoming_directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort incoming-side fixture merge");
}

#[tokio::test]
async fn mark_resolved_stages_exact_current_worktree_content() {
    let (directory, backend) = conflicted_repository().await;
    fs::write(directory.path().join("hello.txt"), "manual resolution\n")
        .expect("write manual resolution");
    let expected = conflict_expected(&backend, directory.path(), "hello.txt").await;

    let result = backend
        .resolve_conflict(
            directory.path(),
            "hello.txt",
            ConflictResolution::MarkResolved,
            &expected,
        )
        .await
        .expect("mark manual resolution resolved");

    assert!(result.conflicts.is_empty());
    assert_eq!(
        fs::read_to_string(directory.path().join("hello.txt")).expect("read manual resolution"),
        "manual resolution\n"
    );
    assert_eq!(
        git_stdout(directory.path(), &["show", ":hello.txt"]),
        "manual resolution"
    );
    backend
        .abort_operation(directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort manual fixture merge");
}

#[tokio::test]
async fn bulk_mark_resolved_stages_every_conflicted_binary_path() {
    let (directory, backend, paths) = binary_conflicted_repository().await;

    let result = backend
        .resolve_conflicts(directory.path(), &[], ConflictResolution::MarkResolved)
        .await
        .expect("mark every conflict resolved");

    assert!(result.conflicts.is_empty());
    assert!(result.needs_user_action, "merge still needs completion");
    for path in &paths {
        assert_eq!(
            fs::read(directory.path().join(path)).expect("read resolved binary"),
            [5, 5, 5, 5],
            "{path} keeps the working copy content"
        );
    }
    assert!(
        git_stdout(directory.path(), &["ls-files", "--unmerged"]).is_empty(),
        "no unmerged index entries remain"
    );
    backend
        .abort_operation(directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort bulk fixture merge");
}

#[tokio::test]
async fn bulk_resolution_takes_one_side_for_selected_paths_only() {
    let (directory, backend, paths) = binary_conflicted_repository().await;

    let result = backend
        .resolve_conflicts(directory.path(), &paths[..2], ConflictResolution::Theirs)
        .await
        .expect("take incoming side for the selected paths");

    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].path, paths[2]);
    for path in &paths[..2] {
        assert_eq!(
            fs::read(directory.path().join(path)).expect("read resolved binary"),
            [9, 9, 9, 9],
            "{path} takes the incoming content"
        );
    }
    backend
        .abort_operation(directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort partial bulk fixture merge");
}

#[tokio::test]
async fn bulk_resolution_rejects_paths_that_are_not_conflicted() {
    let (directory, backend, _) = binary_conflicted_repository().await;

    let error = backend
        .resolve_conflicts(
            directory.path(),
            &["hello.txt".into()],
            ConflictResolution::MarkResolved,
        )
        .await
        .expect_err("clean paths are rejected before any staging happens");

    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        backend
            .conflicted_paths(directory.path())
            .await
            .expect("conflicts are untouched")
            .len(),
        3
    );
    backend
        .abort_operation(directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort rejected bulk fixture merge");
}

#[tokio::test]
async fn rebase_reports_stopped_commit_progress_and_can_skip_it() {
    let (directory, backend, base_oid) = committed_repository().await;
    backend
        .create_branch(directory.path(), "topic", &base_oid, true)
        .await
        .expect("create topic branch");
    fs::write(directory.path().join("hello.txt"), "topic version\n").expect("write topic version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage topic version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "feat: update plugins".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit topic conflict");
    fs::write(directory.path().join("other.txt"), "topic extra\n").expect("write topic extra");
    backend
        .stage_paths(directory.path(), &["other.txt".into()])
        .await
        .expect("stage topic extra");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "chore: unrelated".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit topic extra");

    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main");
    fs::write(directory.path().join("hello.txt"), "main version\n").expect("write main version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage main version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "main change".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit main change");
    backend
        .checkout_branch(directory.path(), "topic")
        .await
        .expect("return to topic");
    let rebase = Command::new("git")
        .arg("-C")
        .arg(directory.path())
        .args(["rebase", "main"])
        .output()
        .expect("run fixture rebase");
    assert!(!rebase.status.success(), "fixture rebase stops on conflict");

    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot the stopped rebase");
    assert_eq!(snapshot.operation_state, RepositoryOperationState::Rebase);
    let progress = snapshot
        .operation_progress
        .expect("a stopped rebase reports progress");
    assert_eq!(progress.current, 1);
    assert_eq!(progress.total, 2);
    assert_eq!(progress.subject.as_deref(), Some("feat: update plugins"));
    let source = snapshot
        .operation_source
        .expect("a stopped rebase names the branch being replayed");
    assert_eq!(source.incoming, "topic");
    assert_eq!(
        source.onto.as_deref(),
        Some("main"),
        "the rebase target resolves to the branch that points at it"
    );

    let skipped = backend
        .skip_operation(directory.path(), ContinueOperation::Rebase)
        .await
        .expect("skip the conflicted commit");
    assert!(skipped.conflicts.is_empty());
    assert_eq!(
        backend
            .operation_state(directory.path())
            .await
            .expect("read operation state"),
        RepositoryOperationState::Normal
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("hello.txt"))
            .expect("read rebased file")
            .replace("\r\n", "\n"),
        "main version\n"
    );
}

#[tokio::test]
async fn stopped_merge_reports_the_branch_it_is_merging() {
    let (directory, backend) = conflicted_repository().await;
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("read snapshot during a conflicted merge");
    assert_eq!(snapshot.operation_state, RepositoryOperationState::Merge);
    let source = snapshot
        .operation_source
        .expect("a stopped merge names the incoming side");
    assert_eq!(source.incoming, "conflicting");
    assert_eq!(source.onto, None);
    assert_eq!(
        source.incoming_oid.as_deref(),
        Some(git_stdout(directory.path(), &["rev-parse", "conflicting"]).trim()),
        "the incoming side carries the oid MERGE_HEAD points at: the second          parent the working copy is about to commit"
    );
}

#[tokio::test]
async fn conflict_resolution_rejects_clean_path_and_absent_index_side() {
    let (clean_directory, clean_backend, _) = committed_repository().await;
    let empty_expected = ConflictExpectedState {
        base: None,
        ours: None,
        theirs: None,
        result: missing_worktree_identity(),
    };
    let clean_error = clean_backend
        .resolve_conflict(
            clean_directory.path(),
            "hello.txt",
            ConflictResolution::Ours,
            &empty_expected,
        )
        .await
        .expect_err("clean path must not resolve as conflict");
    assert_eq!(clean_error.code, ErrorCode::InvalidRequest);

    let (directory, backend) = modify_delete_conflicted_repository().await;
    let expected = conflict_expected(&backend, directory.path(), "hello.txt").await;
    let error = backend
        .resolve_conflict(
            directory.path(),
            "hello.txt",
            ConflictResolution::Theirs,
            &expected,
        )
        .await
        .expect_err("missing incoming side must not imply deletion");
    assert_eq!(error.code, ErrorCode::UnsupportedOperation);
    assert!(error.message.contains("theirs side"));
    assert!(
        error
            .details
            .as_deref()
            .is_some_and(|details| details.contains("explicit delete"))
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("hello.txt"))
            .expect("missing-side resolution leaves file untouched"),
        "main version\n"
    );
    assert!(
        backend
            .snapshot(directory.path())
            .await
            .expect("read unresolved modify/delete snapshot")
            .status
            .entries
            .iter()
            .any(|entry| entry.path == "hello.txt" && entry.conflicted)
    );
    backend
        .abort_operation(directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort modify/delete fixture merge");
}

#[tokio::test]
async fn auto_resolve_reuses_only_recorded_rerere_resolution_and_stages_it() {
    let (directory, backend) = conflicted_repository().await;
    git(directory.path(), &["-c", "rerere.enabled=true", "rerere"]);
    fs::write(
        directory.path().join("hello.txt"),
        "remembered resolution\n",
    )
    .expect("write remembered resolution");
    git(directory.path(), &["add", "--", "hello.txt"]);
    git(directory.path(), &["-c", "rerere.enabled=true", "rerere"]);
    backend
        .abort_operation(directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort trained rerere merge");
    git(
        directory.path(),
        &["config", "--local", "rerere.enabled", "false"],
    );

    let second_conflict = backend
        .merge_branch(directory.path(), "conflicting")
        .await
        .expect("recreate recorded conflict");
    assert_eq!(second_conflict.conflicts.len(), 1);
    assert!(
        fs::read_to_string(directory.path().join("hello.txt"))
            .expect("read unresolved second conflict")
            .contains("<<<<<<<")
    );

    let resolved = backend
        .auto_resolve_conflicts(directory.path())
        .await
        .expect("reuse recorded rerere resolution");
    assert!(resolved.conflicts.is_empty());
    assert!(resolved.needs_user_action, "merge still needs completion");
    assert_eq!(
        fs::read_to_string(directory.path().join("hello.txt")).expect("read reused resolution"),
        "remembered resolution\n"
    );
    assert_eq!(
        git_stdout(directory.path(), &["show", ":hello.txt"]),
        "remembered resolution"
    );
    backend
        .abort_operation(directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort rerere fixture merge");
}

#[tokio::test]
async fn merge_conflict_is_a_successful_transition_requiring_user_action() {
    let (directory, backend) = conflicted_repository().await;
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("conflict snapshot");
    assert_eq!(snapshot.operation_state, RepositoryOperationState::Merge);

    backend
        .abort_operation(directory.path(), ContinueOperation::Merge)
        .await
        .expect("abort merge");
}

/// One rejection is worth one renewal, and only a rejection.
#[test]
fn only_a_refused_credential_is_worth_renewing() {
    let mut options = GitRunOptions::network(1024);
    options.credential = Some(HostCredential {
        host: "github.com".into(),
        token: "spent".into(),
    });
    let refused = |code| -> ApiResult<MutationResult> { Err(ApiError::new(code, "failed")) };

    assert_eq!(
        renewal_candidate(&refused(ErrorCode::AuthenticationRequired), &options)
            .map(|credential| credential.host.as_str()),
        Some("github.com"),
    );
    assert!(renewal_candidate(&refused(ErrorCode::NonFastForward), &options).is_none());
    assert!(renewal_candidate(&refused(ErrorCode::NetworkFailed), &options).is_none());

    // A command left to the user's own credential helpers has nothing to renew.
    let mut anonymous = GitRunOptions::network(1024);
    anonymous.credential = None;
    assert!(renewal_candidate(&refused(ErrorCode::AuthenticationRequired), &anonymous).is_none());
}

/// The same token twice means the service dropped a live credential; running
/// the command again would spend another round trip on the same answer.
#[test]
fn a_renewal_that_changes_nothing_is_not_retried() {
    let previous = HostCredential {
        host: "github.com".into(),
        token: "spent".into(),
    };

    assert_eq!(
        renewed_token(&previous, Some("fresh".into())),
        Some("fresh".into())
    );
    assert_eq!(renewed_token(&previous, Some("spent".into())), None);
    assert_eq!(renewed_token(&previous, None), None);
}

/// A failure the credential had nothing to do with must not spend a renewal:
/// the refresh token is a limited resource and the retry would fail the same
/// way.
#[tokio::test]
async fn an_unreachable_remote_never_asks_for_a_renewal() {
    #[derive(Default)]
    struct CountingCredentials {
        renewals: std::sync::Mutex<u32>,
    }

    #[async_trait::async_trait]
    impl crate::credentials::GitCredentialSource for CountingCredentials {
        async fn token_for(&self, _host: &str) -> Option<String> {
            Some("token".into())
        }

        async fn renew_rejected(&self, _host: &str) -> Option<String> {
            *self.renewals.lock().expect("count renewals") += 1;
            Some("renewed".into())
        }
    }

    let (directory, _, _) = committed_repository().await;
    let credentials = Arc::new(CountingCredentials::default());
    let backend = GitCliBackend::default().with_credentials(credentials.clone());
    git(
        directory.path(),
        &["remote", "add", "origin", "https://127.0.0.1:9/one.git"],
    );

    backend
        .fetch(
            directory.path(),
            &FetchOptions {
                remote: None,
                prune: false,
                tags: false,
            },
            CancellationToken::new(),
        )
        .await
        .expect_err("unreachable remote");

    assert_eq!(*credentials.renewals.lock().expect("count renewals"), 0);
}
