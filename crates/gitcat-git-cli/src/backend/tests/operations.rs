//! Continuing, skipping and aborting merge, rebase, cherry-pick and revert.

use super::*;

async fn commit_hello(backend: &GitCliBackend, path: &Path, contents: &str, message: &str) {
    fs::write(path.join("hello.txt"), contents).expect("write hello");
    backend
        .stage_paths(path, &["hello.txt".into()])
        .await
        .expect("stage hello");
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
        .expect("commit hello");
}

/// `main` and `side` both rewrite hello.txt from the shared first commit;
/// returns the tip of `side`.
async fn diverged_repository() -> (tempfile::TempDir, GitCliBackend, String) {
    let (directory, backend, base_oid) = committed_repository().await;
    backend
        .create_branch(directory.path(), "side", &base_oid, true)
        .await
        .expect("create side branch");
    commit_hello(&backend, directory.path(), "side version\n", "side change").await;
    let side = backend
        .head_oid(directory.path())
        .await
        .expect("read side")
        .expect("side exists");
    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main");
    commit_hello(&backend, directory.path(), "main version\n", "main change").await;
    (directory, backend, side)
}

fn head_subject(path: &Path) -> String {
    git_stdout(path, &["log", "-1", "--format=%B"])
}

async fn take_ours(backend: &GitCliBackend, path: &Path) {
    let expected = conflict_expected(backend, path, "hello.txt").await;
    backend
        .resolve_conflict(path, "hello.txt", ConflictResolution::Ours, &expected)
        .await
        .expect("take the current side");
}

#[tokio::test]
async fn snapshot_reports_conflict_kind_and_merge_message() {
    let (directory, backend) = conflicted_repository().await;
    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");
    let entry = snapshot
        .status
        .entries
        .iter()
        .find(|entry| entry.path == "hello.txt")
        .expect("conflicted entry");
    assert_eq!(entry.conflict_kind, Some(ConflictKind::BothModified));
    assert_eq!(
        snapshot.operation_message.as_deref(),
        Some("Merge branch 'conflicting'")
    );
    assert!(!snapshot.operation_autostash);

    let git_dir = backend.git_dir(directory.path()).await.expect("git dir");
    fs::write(git_dir.join("MERGE_AUTOSTASH"), "0000\n").expect("fake autostash");
    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");
    assert!(snapshot.operation_autostash);
    fs::remove_file(git_dir.join("MERGE_AUTOSTASH")).expect("drop fake autostash");

    let (modify_delete, md_backend) = modify_delete_conflicted_repository().await;
    let snapshot = md_backend
        .snapshot(modify_delete.path())
        .await
        .expect("snapshot");
    assert_eq!(
        snapshot.status.entries[0].conflict_kind,
        Some(ConflictKind::DeletedByThem)
    );
    let (delete_modify, dm_backend) = delete_modify_conflicted_repository().await;
    let snapshot = dm_backend
        .snapshot(delete_modify.path())
        .await
        .expect("snapshot");
    assert_eq!(
        snapshot.status.entries[0].conflict_kind,
        Some(ConflictKind::DeletedByUs)
    );
}

#[tokio::test]
async fn continue_merge_commits_the_message_it_was_given() {
    let (directory, backend) = conflicted_repository().await;
    take_ours(&backend, directory.path()).await;
    backend
        .continue_operation(
            directory.path(),
            ContinueOperation::Merge,
            Some("Merge the conflicting work\n\nWith a body."),
        )
        .await
        .expect("continue the merge");
    assert_eq!(
        head_subject(directory.path()),
        "Merge the conflicting work\n\nWith a body."
    );
    let parents = git_stdout(directory.path(), &["log", "-1", "--format=%P"]);
    assert_eq!(parents.split_whitespace().count(), 2);
}

#[tokio::test]
async fn continue_cherry_pick_uses_prepared_or_given_message() {
    let (directory, backend, side) = diverged_repository().await;
    let result = backend
        .cherry_pick(directory.path(), &side, None, true)
        .await
        .expect("cherry-pick conflict is resumable");
    assert_eq!(result.conflicts.len(), 1);
    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");
    assert_eq!(
        snapshot.operation_state,
        RepositoryOperationState::CherryPick
    );
    assert_eq!(snapshot.operation_message.as_deref(), Some("side change"));
    take_ours(&backend, directory.path()).await;
    // Taking the current side leaves nothing to pick; keep a change so Git
    // has a commit to make.
    fs::write(directory.path().join("hello.txt"), "picked by hand\n").expect("edit");
    git(directory.path(), &["add", "hello.txt"]);
    backend
        .continue_operation(
            directory.path(),
            ContinueOperation::CherryPick,
            Some("picked with a new message"),
        )
        .await
        .expect("continue cherry-pick");
    assert_eq!(head_subject(directory.path()), "picked with a new message");
}

#[tokio::test]
async fn continue_revert_uses_given_message() {
    let (directory, backend, _base) = committed_repository().await;
    commit_hello(&backend, directory.path(), "second\n", "second").await;
    let second = backend
        .head_oid(directory.path())
        .await
        .expect("read HEAD")
        .expect("HEAD");
    commit_hello(&backend, directory.path(), "third\n", "third").await;
    backend
        .revert_commit(directory.path(), &second, None, true)
        .await
        .expect("revert conflict is resumable");
    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");
    assert_eq!(snapshot.operation_state, RepositoryOperationState::Revert);
    let message = snapshot.operation_message.expect("revert message");
    assert!(message.starts_with("Revert \"second\""), "{message}");
    assert!(message.contains(&format!("This reverts commit {second}.")));
    fs::write(directory.path().join("hello.txt"), "reverted\n").expect("edit");
    git(directory.path(), &["add", "hello.txt"]);
    backend
        .continue_operation(
            directory.path(),
            ContinueOperation::Revert,
            Some("Undo second by hand"),
        )
        .await
        .expect("continue revert");
    assert_eq!(head_subject(directory.path()), "Undo second by hand");
}

#[tokio::test]
async fn continue_rebase_rewrites_the_replayed_commit_message() {
    let (directory, backend, _side) = diverged_repository().await;
    backend
        .checkout_branch(directory.path(), "side")
        .await
        .expect("checkout side");
    let output = Command::new("git")
        .arg("-C")
        .arg(directory.path())
        .args(["rebase", "main"])
        .env("GIT_EDITOR", "true")
        .output()
        .expect("run rebase");
    assert!(!output.status.success(), "rebase stops on the conflict");
    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");
    assert_eq!(snapshot.operation_state, RepositoryOperationState::Rebase);
    assert_eq!(snapshot.operation_message.as_deref(), Some("side change"));
    fs::write(directory.path().join("hello.txt"), "rebased by hand\n").expect("edit");
    git(directory.path(), &["add", "hello.txt"]);
    backend
        .continue_operation(
            directory.path(),
            ContinueOperation::Rebase,
            Some("side change, reworded"),
        )
        .await
        .expect("continue rebase");
    assert_eq!(head_subject(directory.path()), "side change, reworded");
}

#[tokio::test]
async fn cherry_pick_and_revert_without_commit_leave_the_change_staged() {
    let (directory, backend, base) = committed_repository().await;
    backend
        .create_branch(directory.path(), "extra", &base, true)
        .await
        .expect("create branch");
    fs::write(directory.path().join("extra.txt"), "extra\n").expect("write extra");
    git(directory.path(), &["add", "extra.txt"]);
    git(directory.path(), &["commit", "-m", "add extra"]);
    let extra = git_stdout(directory.path(), &["rev-parse", "HEAD"]);
    backend
        .checkout_branch(directory.path(), "main")
        .await
        .expect("return to main");

    backend
        .cherry_pick(directory.path(), &extra, None, false)
        .await
        .expect("cherry-pick without commit");
    assert_eq!(git_stdout(directory.path(), &["rev-parse", "HEAD"]), base);
    assert_eq!(
        git_stdout(directory.path(), &["diff", "--cached", "--name-only"]),
        "extra.txt"
    );
    git(directory.path(), &["commit", "-m", "picked"]);
    let picked = git_stdout(directory.path(), &["rev-parse", "HEAD"]);

    backend
        .revert_commit(directory.path(), &picked, None, false)
        .await
        .expect("revert without commit");
    assert_eq!(git_stdout(directory.path(), &["rev-parse", "HEAD"]), picked);
    assert_eq!(
        git_stdout(directory.path(), &["diff", "--cached", "--name-status"]),
        "D\textra.txt"
    );
}

#[tokio::test]
async fn take_incoming_works_on_a_binary_conflict() {
    let (directory, backend, paths) = binary_conflicted_repository().await;
    let expected = conflict_expected(&backend, directory.path(), &paths[0]).await;
    let result = backend
        .resolve_conflict(
            directory.path(),
            &paths[0],
            ConflictResolution::Theirs,
            &expected,
        )
        .await
        .expect("take incoming binary");
    assert!(!result.conflicts.iter().any(|entry| entry.path == paths[0]));
    assert_eq!(
        fs::read(directory.path().join(&paths[0])).expect("read binary"),
        vec![9, 9, 9, 9]
    );
}

#[tokio::test]
async fn keep_base_version_restores_the_common_ancestor() {
    let (directory, backend) = modify_delete_conflicted_repository().await;
    let expected = conflict_expected(&backend, directory.path(), "hello.txt").await;
    let result = backend
        .resolve_conflict(
            directory.path(),
            "hello.txt",
            ConflictResolution::Base,
            &expected,
        )
        .await
        .expect("keep base version");
    assert!(result.conflicts.is_empty());
    assert_eq!(
        fs::read_to_string(directory.path().join("hello.txt"))
            .expect("read base")
            .replace("\r\n", "\n"),
        "first\n"
    );
    assert_eq!(
        git_stdout(directory.path(), &["show", ":hello.txt"]),
        "first"
    );

    // Both sides adding the path leaves no ancestor to keep.
    let (added, added_backend, base) = committed_repository().await;
    added_backend
        .create_branch(added.path(), "adder", &base, true)
        .await
        .expect("branch");
    fs::write(added.path().join("new.txt"), "theirs\n").expect("write");
    git(added.path(), &["add", "new.txt"]);
    git(added.path(), &["commit", "-m", "theirs adds"]);
    added_backend
        .checkout_branch(added.path(), "main")
        .await
        .expect("main");
    fs::write(added.path().join("new.txt"), "ours\n").expect("write");
    git(added.path(), &["add", "new.txt"]);
    git(added.path(), &["commit", "-m", "ours adds"]);
    added_backend
        .merge_branch(added.path(), "adder")
        .await
        .expect("add/add conflict is resumable");
    let snapshot = added_backend
        .snapshot(added.path())
        .await
        .expect("snapshot");
    assert_eq!(
        snapshot.status.entries[0].conflict_kind,
        Some(ConflictKind::BothAdded)
    );
    let expected = conflict_expected(&added_backend, added.path(), "new.txt").await;
    let error = added_backend
        .resolve_conflict(added.path(), "new.txt", ConflictResolution::Base, &expected)
        .await
        .expect_err("no base to keep");
    assert_eq!(error.code, ErrorCode::UnsupportedOperation);
}

#[tokio::test]
async fn resolution_warnings_name_leftover_markers_and_helper_copies() {
    let (directory, backend) = conflicted_repository().await;
    let warnings = backend
        .conflict_resolution_warnings(directory.path(), &[])
        .await
        .expect("warnings");
    assert_eq!(
        warnings,
        vec![ConflictResolutionWarning {
            path: "hello.txt".into(),
            reason: ConflictWarningReason::Markers,
        }]
    );
    fs::write(directory.path().join("hello.txt"), "resolved\n").expect("resolve by hand");
    assert!(
        backend
            .conflict_resolution_warnings(directory.path(), &["hello.txt".into()])
            .await
            .expect("warnings")
            .is_empty()
    );

    fs::create_dir_all(directory.path().join("fd")).expect("directory");
    assert!(crate::conflict::is_helper_copy(directory.path(), "fd~HEAD"));
    assert!(!crate::conflict::is_helper_copy(
        directory.path(),
        "notes~draft"
    ));
    assert!(!crate::conflict::is_helper_copy(directory.path(), "~HEAD"));
    assert!(crate::conflict::has_conflict_markers(
        "a\r\n<<<<<<< ours\r\nx\r\n=======\r\ny\r\n>>>>>>> theirs\r\n"
    ));
    assert!(!crate::conflict::has_conflict_markers("=======\nheading\n"));
}
