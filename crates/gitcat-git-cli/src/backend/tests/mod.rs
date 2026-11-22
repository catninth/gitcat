use crate::conflict::{
    classify_conflict_content, encode_edited_conflict_text, missing_worktree_identity,
    supports_merge_tree_preflight,
};
use crate::parse::StashCommit;
use crate::validate::{sparse_patterns, validate_mainline_parent};

use std::{fs, process::Command};

use tempfile::tempdir;

use super::*;

mod commits;
mod conflicts;
mod history;
mod operations;
mod remotes;
mod worktree;

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

/// Fixture repositories are built through the backend, and every backend call
/// starts several Git processes -- the dominant cost of this suite on Windows.
/// The common fixtures are therefore built once per test process and each test
/// gets its own copy of the finished repository, which costs a directory copy.
///
/// The template directories are never dropped (statics are not), so each test
/// run leaves one small repository per fixture in the temporary directory.
async fn copy_of_template(template: &Path) -> tempfile::TempDir {
    let source = template.to_path_buf();
    let copy = tempdir().expect("temp repository");
    let target = copy.path().to_path_buf();
    tokio::task::spawn_blocking(move || copy_tree(&source, &target))
        .await
        .expect("copy fixture template");
    copy
}

fn copy_tree(source: &Path, target: &Path) {
    for entry in fs::read_dir(source).expect("read fixture template") {
        let entry = entry.expect("fixture template entry");
        let destination = target.join(entry.file_name());
        if entry.file_type().expect("fixture entry type").is_dir() {
            fs::create_dir(&destination).expect("create fixture directory");
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), &destination).expect("copy fixture file");
        }
    }
}

async fn committed_repository() -> (tempfile::TempDir, GitCliBackend, String) {
    static TEMPLATE: tokio::sync::OnceCell<(tempfile::TempDir, String)> =
        tokio::sync::OnceCell::const_new();
    let (template, oid) = TEMPLATE
        .get_or_init(|| async {
            let (directory, _, oid) = build_committed_repository().await;
            (directory, oid)
        })
        .await;
    (
        copy_of_template(template.path()).await,
        GitCliBackend::default(),
        oid.clone(),
    )
}

async fn build_committed_repository() -> (tempfile::TempDir, GitCliBackend, String) {
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
    static TEMPLATE: tokio::sync::OnceCell<tempfile::TempDir> = tokio::sync::OnceCell::const_new();
    let template = TEMPLATE
        .get_or_init(|| async { build_conflicted_repository().await.0 })
        .await;
    (
        copy_of_template(template.path()).await,
        GitCliBackend::default(),
    )
}

async fn build_conflicted_repository() -> (tempfile::TempDir, GitCliBackend) {
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
