//! Commit creation, rewording, reset and the commit action menu.

use super::*;

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
async fn initial_commit_rejects_a_repository_whose_remote_has_commits() {
    let (remote, _, _) = committed_repository().await;
    let directory = tempdir().expect("temp repository");
    let backend = GitCliBackend::default();
    backend
        .init_repository(directory.path(), "main")
        .await
        .expect("initialize repository");
    let remote_path = remote.path().to_string_lossy().into_owned();
    git(directory.path(), &["remote", "add", "origin", &remote_path]);
    git(directory.path(), &["fetch", "origin"]);

    let error = backend
        .create_initial_commit(directory.path(), "Initial commit")
        .await
        .expect_err("a remote with history is refused");

    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert!(!directory.path().join("README.md").exists());
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
