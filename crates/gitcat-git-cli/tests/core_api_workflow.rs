use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use gitcat_contracts::{
    ApiError, ChangeKind, CommitOptions, CommitSearchQuery, DiffRequest, DiffTarget, ErrorCode,
    ExpectedState, HeadState, HistoryQuery, HistoryScope, MutationResult, PushForce, PushOptions,
    RepositoryId, RepositorySnapshot,
};
use gitcat_core::CoreApi;
use gitcat_git_cli::GitCliBackend;
use tempfile::{TempDir, tempdir};
use tokio_util::sync::CancellationToken;

struct TestRepository {
    _directory: TempDir,
    path: PathBuf,
    api: CoreApi,
    id: RepositoryId,
}

async fn initialized_repository() -> TestRepository {
    let directory = tempdir().expect("create temporary test directory");
    let path = directory.path().join("repository with spaces");
    let api = CoreApi::new(Arc::new(GitCliBackend::default()));

    let version = api.probe().await.expect("probe system Git");
    assert!(version.major >= 2);

    let (initial_id, initialized) = api
        .init_repository(&path, "main")
        .await
        .expect("initialize repository through CoreApi");
    assert!(Path::new(&initialized.root).is_absolute());

    configure_local_repository(&path, "user.name", "GitCat Integration Tests");
    configure_local_repository(&path, "user.email", "gitcat-tests@example.invalid");
    configure_local_repository(&path, "commit.gpgSign", "false");
    configure_local_repository(&path, "core.autocrlf", "false");

    api.close_repository(&initial_id)
        .await
        .expect("close initialized repository");
    let (id, reopened) = api
        .open_repository(&path)
        .await
        .expect("reopen repository through CoreApi");
    assert_eq!(reopened.name, "repository with spaces");

    TestRepository {
        _directory: directory,
        path,
        api,
        id,
    }
}

fn configure_local_repository(repository: &Path, key: &str, value: &str) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["config", "--local", key, value])
        .output()
        .expect("run local git config subprocess");

    assert!(
        output.status.success(),
        "git config {key} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn commit_file(
    repository: &TestRepository,
    relative_path: &str,
    contents: &str,
    message: &str,
) -> String {
    let file_path = repository.path.join(relative_path);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).expect("create committed file parent");
    }
    fs::write(&file_path, contents).expect("write committed file");

    repository
        .api
        .stage_paths(&repository.id, &[relative_path.to_owned()])
        .await
        .expect("stage file through CoreApi");
    repository
        .api
        .create_commit(
            &repository.id,
            &CommitOptions {
                message: message.to_owned(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit staged file through CoreApi")
        .after_oid
        .expect("commit returns new HEAD")
}

fn expected_state(snapshot: &RepositorySnapshot) -> ExpectedState {
    let head_oid = match &snapshot.head {
        HeadState::Branch { oid, .. } | HeadState::Detached { oid } => Some(oid.clone()),
        HeadState::Unborn { .. } => None,
    };
    ExpectedState {
        head_oid,
        generation: snapshot.generation.clone(),
    }
}

#[tokio::test]
async fn commit_is_searchable_and_readable_through_public_core_api() {
    let repository = initialized_repository().await;
    let oid = commit_file(
        &repository,
        "src/notes.txt",
        "alpha\nbeta\n",
        "feat: searchable subject\n\nBody marker: body-only-needle.\n",
    )
    .await;

    let subject_search = repository
        .api
        .search_commits(
            &repository.id,
            &CommitSearchQuery {
                query: "searchable subject".into(),
                scope: HistoryScope::CurrentBranch,
                limit: 10,
            },
        )
        .await
        .expect("search commit subject");
    assert_eq!(subject_search.total, 1);
    assert_eq!(subject_search.hits[0].oid, oid);
    assert!(subject_search.hits[0].matched_subject);

    let body_search = repository
        .api
        .search_commits(
            &repository.id,
            &CommitSearchQuery {
                query: "body-only-needle".into(),
                scope: HistoryScope::CurrentBranch,
                limit: 10,
            },
        )
        .await
        .expect("search commit body");
    assert_eq!(body_search.total, 1);
    assert_eq!(body_search.hits[0].oid, oid);
    assert!(body_search.hits[0].matched_body);

    let history = repository
        .api
        .history(
            &repository.id,
            &HistoryQuery {
                scope: HistoryScope::CurrentBranch,
                cursor: None,
                limit: 20,
            },
        )
        .await
        .expect("read commit history");
    assert_eq!(history.commits.len(), 1);
    assert_eq!(history.commits[0].oid, oid);
    assert_eq!(history.commits[0].subject, "feat: searchable subject");
    assert_eq!(history.commits[0].graph.lane, 0);

    let details = repository
        .api
        .commit_details(&repository.id, &oid, 0)
        .await
        .expect("read commit details");
    assert_eq!(details.subject, "feat: searchable subject");
    assert_eq!(details.body, "Body marker: body-only-needle.");
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].new_path, "src/notes.txt");
    assert_eq!(details.files[0].status, ChangeKind::Added);
    assert_eq!(details.stats.additions, 2);

    let diff = repository
        .api
        .diff(
            &repository.id,
            &DiffRequest {
                target: DiffTarget::Commit {
                    oid: oid.clone(),
                    parent_index: 0,
                },
                path: "src/notes.txt".into(),
                context_lines: 3,
                ignore_whitespace: false,
                max_bytes: 1024 * 1024,
                whole_file: false,
                old_path: None,
            },
        )
        .await
        .expect("read committed file diff");
    assert_eq!(diff.status, ChangeKind::Added);
    assert_eq!(diff.stats.additions, 2);
    assert!(
        diff.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .any(|line| line.content == "alpha")
    );

    let snapshot = repository
        .api
        .snapshot(&repository.id)
        .await
        .expect("read clean repository snapshot");
    assert!(snapshot.status.clean);
}

#[tokio::test]
async fn branch_lifecycle_uses_public_core_api_and_expected_state() {
    let repository = initialized_repository().await;
    let oid = commit_file(
        &repository,
        "seed.txt",
        "seed\n",
        "chore: seed repository\n",
    )
    .await;

    repository
        .api
        .create_branch(&repository.id, "feature/workflow", &oid, true)
        .await
        .expect("create and checkout branch");
    let feature_snapshot = repository
        .api
        .snapshot(&repository.id)
        .await
        .expect("read feature branch snapshot");
    assert!(matches!(
        feature_snapshot.head,
        HeadState::Branch { ref name, .. } if name == "feature/workflow"
    ));

    repository
        .api
        .rename_branch(&repository.id, "feature/workflow", "feature/renamed")
        .await
        .expect("rename current branch");
    repository
        .api
        .checkout_branch(&repository.id, "main")
        .await
        .expect("checkout main branch");

    let before_delete = repository
        .api
        .snapshot(&repository.id)
        .await
        .expect("prepare branch deletion");
    assert!(
        before_delete
            .local_branches
            .iter()
            .any(|branch| branch.name == "feature/renamed")
    );
    let expected = expected_state(&before_delete);
    repository
        .api
        .delete_branch(&repository.id, "feature/renamed", false, false, &expected)
        .await
        .expect("delete fully merged branch");

    let after_delete = repository
        .api
        .snapshot(&repository.id)
        .await
        .expect("read branches after deletion");
    assert!(matches!(
        after_delete.head,
        HeadState::Branch { ref name, .. } if name == "main"
    ));
    assert!(
        after_delete
            .local_branches
            .iter()
            .all(|branch| branch.name != "feature/renamed")
    );
}

/// Pushing what the remote already has costs nothing.
///
/// The remote directory is removed before the second push: a command that
/// actually reached it would fail, so a successful result is proof the answer
/// came from the remote-tracking ref alone.
#[tokio::test]
async fn a_push_with_nothing_to_send_never_contacts_the_remote() {
    let repository = initialized_repository().await;
    commit_file(&repository, "pushed.txt", "one\n", "feat: first").await;

    let remote = repository
        .path
        .parent()
        .expect("parent directory")
        .join("remote.git");
    run_git(
        &repository.path,
        &["init", "--bare", &remote.to_string_lossy()],
    );
    run_git(
        &repository.path,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    run_git(
        &repository.path,
        &["push", "--set-upstream", "origin", "main"],
    );

    fs::remove_dir_all(&remote).expect("remove the bare remote");

    let result = repository
        .api
        .push(
            &repository.id,
            &PushOptions {
                skip_submodules: false,
                remote: None,
                branch: None,
                set_upstream: false,
                force: PushForce::None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("push reports up to date without reaching the remote");
    assert_eq!(result.notice.as_deref(), Some("Everything up-to-date"));
}

/// A commit the remote does not have sends the command, remote or no remote.
#[tokio::test]
async fn a_push_with_something_to_send_still_runs() {
    let repository = initialized_repository().await;
    commit_file(&repository, "pushed.txt", "one\n", "feat: first").await;

    let remote = repository
        .path
        .parent()
        .expect("parent directory")
        .join("ahead.git");
    run_git(
        &repository.path,
        &["init", "--bare", &remote.to_string_lossy()],
    );
    run_git(
        &repository.path,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    run_git(
        &repository.path,
        &["push", "--set-upstream", "origin", "main"],
    );

    commit_file(&repository, "pushed.txt", "two\n", "feat: second").await;

    let result = repository
        .api
        .push(
            &repository.id,
            &PushOptions {
                skip_submodules: false,
                remote: None,
                branch: None,
                set_upstream: false,
                force: PushForce::None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("push the new commit");
    assert_eq!(result.notice, None);
}

fn run_git(repository: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .expect("run git subprocess");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn push(repository: &TestRepository, force: PushForce) -> Result<MutationResult, ApiError> {
    repository
        .api
        .push(
            &repository.id,
            &PushOptions {
                skip_submodules: false,
                remote: None,
                branch: None,
                set_upstream: false,
                force,
            },
            CancellationToken::new(),
        )
        .await
}

fn remote_tip(remote: &Path) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(remote)
        .args(["rev-parse", "refs/heads/main"])
        .output()
        .expect("read the bare remote tip");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A rewritten branch is what force push exists for: the ordinary push is
/// refused, and the lease -- held against a tracking ref nothing has moved --
/// lets it through.
#[tokio::test]
async fn a_leased_force_push_replaces_a_rewritten_branch() {
    let repository = initialized_repository().await;
    commit_file(
        &repository,
        "pushed.txt",
        "one
",
        "feat: first",
    )
    .await;

    let remote = repository
        .path
        .parent()
        .expect("parent directory")
        .join("leased.git");
    run_git(
        &repository.path,
        &["init", "--bare", &remote.to_string_lossy()],
    );
    run_git(
        &repository.path,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    run_git(
        &repository.path,
        &["push", "--set-upstream", "origin", "main"],
    );

    run_git(
        &repository.path,
        &["commit", "--amend", "-m", "feat: reworded"],
    );
    let rewritten = remote_tip(&repository.path);

    let refused = push(&repository, PushForce::None)
        .await
        .expect_err("a rewritten branch is not a fast-forward");
    assert_eq!(refused.code, ErrorCode::NonFastForward);

    push(&repository, PushForce::WithLease)
        .await
        .expect("the lease holds, so the force push goes through");
    assert_eq!(remote_tip(&remote), rewritten);
}

/// A lease is refused when the remote moved where this branch never looked,
/// and the failure says overwriting is the step that would get past it.
#[tokio::test]
async fn a_lease_refuses_a_remote_that_moved_unseen() {
    let repository = initialized_repository().await;
    commit_file(
        &repository,
        "pushed.txt",
        "one
",
        "feat: first",
    )
    .await;

    let parent = repository.path.parent().expect("parent directory");
    let remote = parent.join("unseen.git");
    run_git(
        &repository.path,
        &["init", "--bare", &remote.to_string_lossy()],
    );
    run_git(
        &repository.path,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    run_git(
        &repository.path,
        &["push", "--set-upstream", "origin", "main"],
    );

    // Somebody else pushes. This repository never fetches it, so its
    // remote-tracking ref still names the commit the lease is held against.
    let other = parent.join("other");
    run_git(
        &repository.path,
        &["clone", &remote.to_string_lossy(), &other.to_string_lossy()],
    );
    // The bare repository was initialized with whatever default branch name
    // this Git uses, so its HEAD need not be the branch that was pushed to it.
    run_git(&other, &["checkout", "-B", "main", "origin/main"]);
    fs::write(
        other.join("theirs.txt"),
        "theirs
",
    )
    .expect("write their file");
    run_git(&other, &["add", "theirs.txt"]);
    run_git(
        &other,
        &[
            "-c",
            "user.name=Other",
            "-c",
            "user.email=other@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "-m",
            "feat: theirs",
        ],
    );
    run_git(&other, &["push", "origin", "main"]);
    let theirs = remote_tip(&remote);

    run_git(
        &repository.path,
        &["commit", "--amend", "-m", "feat: reworded"],
    );
    let refused = push(&repository, PushForce::WithLease)
        .await
        .expect_err("the lease does not cover a commit this branch never saw");
    assert_eq!(refused.code, ErrorCode::NonFastForward);
    assert!(
        refused
            .recovery_actions
            .iter()
            .any(|action| action.kind == "push_force"),
        "a refused lease offers overwriting as the next step: {:?}",
        refused.recovery_actions,
    );
    assert_eq!(remote_tip(&remote), theirs, "nothing was overwritten");

    push(&repository, PushForce::Force)
        .await
        .expect("an outright force overwrites what the lease protected");
    assert_ne!(remote_tip(&remote), theirs);
}
