//! Renames in the working tree, the index and history, matched against the
//! GitKraken observations in `scripts/rename-lab/FINDINGS.md`.

use super::*;

/// Twenty equal-length lines, so changing k of them leaves the file exactly
/// (20 - k) / 20 similar to the original.
fn rows(tag: &str, changed: &[usize]) -> String {
    (1..=20)
        .map(|number| {
            let value = if changed.contains(&number) {
                "CHANGED!"
            } else {
                "original"
            };
            format!("<Row Id=\"{number:02}\" Key=\"{tag}-{number:02}\" Value=\"{value}\" />\n")
        })
        .collect()
}

/// A repository whose last commit holds `src/old.txt`.
async fn repository_with_source() -> (tempfile::TempDir, GitCliBackend) {
    let (directory, backend, _) = committed_repository().await;
    fs::create_dir_all(directory.path().join("src")).expect("create src");
    fs::write(directory.path().join("src/old.txt"), rows("old", &[])).expect("write source");
    git(directory.path(), &["add", "--", "src/old.txt"]);
    git(directory.path(), &["commit", "-q", "-m", "add source"]);
    (directory, backend)
}

fn rename_on_disk(directory: &Path, contents: &str) {
    fs::remove_file(directory.join("src/old.txt")).expect("remove source");
    fs::write(directory.join("src/new.txt"), contents).expect("write destination");
}

fn entry<'a>(snapshot: &'a RepositorySnapshot, path: &str) -> Option<&'a StatusEntry> {
    snapshot
        .status
        .entries
        .iter()
        .find(|entry| entry.path == path)
}

fn rename_request(target: DiffTarget) -> DiffRequest {
    DiffRequest {
        target,
        path: "src/new.txt".into(),
        context_lines: 3,
        ignore_whitespace: false,
        max_bytes: 1 << 20,
        whole_file: false,
        old_path: Some("src/old.txt".into()),
    }
}

#[tokio::test]
async fn unstaged_rename_with_an_added_line_is_one_renamed_entry() {
    let (directory, backend) = repository_with_source().await;
    rename_on_disk(
        directory.path(),
        &format!("{}extra line\n", rows("old", &[])),
    );

    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");

    let renamed = entry(&snapshot, "src/new.txt").expect("destination entry");
    assert_eq!(renamed.index, None);
    assert_eq!(renamed.worktree, Some(ChangeKind::Renamed));
    assert_eq!(renamed.old_path.as_deref(), Some("src/old.txt"));
    assert_eq!(
        renamed.worktree_stats,
        Some(LineStats {
            additions: 1,
            deletions: 0
        })
    );
    assert!(entry(&snapshot, "src/old.txt").is_none());
    // The pairing is worked out in a scratch index; the real one is untouched.
    assert_eq!(
        git_stdout(directory.path(), &["status", "--porcelain"]),
        "D src/old.txt\n?? src/new.txt"
    );
}

#[tokio::test]
async fn rename_threshold_matches_git_at_fifty_percent() {
    let (directory, backend) = repository_with_source().await;
    rename_on_disk(
        directory.path(),
        &rows("old", &[1, 3, 5, 7, 9, 11, 13, 15, 17]),
    );
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("55% snapshot");
    assert_eq!(
        entry(&snapshot, "src/new.txt").and_then(|entry| entry.worktree),
        Some(ChangeKind::Renamed)
    );

    rename_on_disk_again(
        directory.path(),
        &rows("old", &[1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 20]),
    );
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("45% snapshot");
    assert_eq!(
        entry(&snapshot, "src/new.txt").and_then(|entry| entry.worktree),
        Some(ChangeKind::Untracked)
    );
    assert_eq!(
        entry(&snapshot, "src/old.txt").and_then(|entry| entry.worktree),
        Some(ChangeKind::Deleted)
    );
}

fn rename_on_disk_again(directory: &Path, contents: &str) {
    fs::write(directory.join("src/new.txt"), contents).expect("rewrite destination");
}

#[tokio::test]
async fn staged_deletion_is_not_paired_with_an_untracked_file() {
    let (directory, backend) = repository_with_source().await;
    fs::copy(
        directory.path().join("src/old.txt"),
        directory.path().join("src/new.txt"),
    )
    .expect("copy source");
    git(directory.path(), &["rm", "-q", "--", "src/old.txt"]);

    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");

    assert_eq!(
        entry(&snapshot, "src/old.txt").and_then(|entry| entry.index),
        Some(ChangeKind::Deleted)
    );
    assert_eq!(
        entry(&snapshot, "src/new.txt").and_then(|entry| entry.worktree),
        Some(ChangeKind::Untracked)
    );
}

#[tokio::test]
async fn unstaged_rename_diff_is_taken_against_the_old_path() {
    let (directory, backend) = repository_with_source().await;
    rename_on_disk(
        directory.path(),
        &format!("{}extra line\n", rows("old", &[])),
    );

    let diff = backend
        .diff(directory.path(), &rename_request(DiffTarget::Worktree))
        .await
        .expect("rename diff");

    assert_eq!(diff.status, ChangeKind::Renamed);
    assert_eq!(diff.old_path.as_deref(), Some("src/old.txt"));
    assert_eq!(diff.new_path, "src/new.txt");
    assert_eq!(diff.stats.additions, 1);
    assert_eq!(diff.stats.deletions, 0);
}

#[tokio::test]
async fn identical_unstaged_rename_has_no_hunks() {
    let (directory, backend) = repository_with_source().await;
    rename_on_disk(directory.path(), &rows("old", &[]));

    let diff = backend
        .diff(directory.path(), &rename_request(DiffTarget::Worktree))
        .await
        .expect("rename diff");

    assert_eq!(diff.status, ChangeKind::Renamed);
    assert!(diff.hunks.is_empty());
}

#[tokio::test]
async fn unstaged_rename_diff_falls_back_to_the_new_file_when_no_longer_paired() {
    let (directory, backend) = repository_with_source().await;
    rename_on_disk(directory.path(), "nothing alike\n");

    let diff = backend
        .diff(directory.path(), &rename_request(DiffTarget::Worktree))
        .await
        .expect("fallback diff");

    assert_eq!(diff.status, ChangeKind::Added);
    assert_eq!(diff.new_path, "src/new.txt");
}

#[tokio::test]
async fn staged_rename_diff_is_taken_against_the_old_path() {
    let (directory, backend) = repository_with_source().await;
    git(directory.path(), &["mv", "src/old.txt", "src/new.txt"]);
    let mut contents = fs::read_to_string(directory.path().join("src/new.txt")).expect("read");
    contents.push_str("extra line\n");
    fs::write(directory.path().join("src/new.txt"), contents).expect("edit destination");

    // Staged side: the rename alone, nothing changed.
    let staged = backend
        .diff(directory.path(), &rename_request(DiffTarget::Staged))
        .await
        .expect("staged diff");
    assert_eq!(staged.status, ChangeKind::Renamed);
    assert_eq!(staged.old_path.as_deref(), Some("src/old.txt"));
    assert!(staged.hunks.is_empty());

    // Unstaged side: an ordinary edit of the new path.
    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");
    let renamed = entry(&snapshot, "src/new.txt").expect("destination entry");
    assert_eq!(renamed.index, Some(ChangeKind::Renamed));
    assert_eq!(renamed.worktree, Some(ChangeKind::Modified));
}

#[tokio::test]
async fn staging_both_paths_stages_the_rename_and_discarding_restores_the_source() {
    let (directory, backend) = repository_with_source().await;
    rename_on_disk(directory.path(), &rows("old", &[]));
    let paths = vec!["src/new.txt".to_owned(), "src/old.txt".to_owned()];

    backend
        .stage_paths(directory.path(), &paths)
        .await
        .expect("stage rename");
    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");
    let renamed = entry(&snapshot, "src/new.txt").expect("staged entry");
    assert_eq!(renamed.index, Some(ChangeKind::Renamed));
    assert_eq!(renamed.old_path.as_deref(), Some("src/old.txt"));
    assert_eq!(renamed.worktree, None);

    backend
        .unstage_paths(directory.path(), &paths)
        .await
        .expect("unstage rename");
    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");
    assert_eq!(
        entry(&snapshot, "src/new.txt").and_then(|entry| entry.worktree),
        Some(ChangeKind::Renamed)
    );

    backend
        .discard_paths(directory.path(), &paths)
        .await
        .expect("discard rename");
    assert!(directory.path().join("src/old.txt").is_file());
    assert!(!directory.path().join("src/new.txt").exists());
    let snapshot = backend.snapshot(directory.path()).await.expect("snapshot");
    assert!(snapshot.status.clean);
}

#[tokio::test]
async fn committed_rename_diff_is_taken_against_the_old_path() {
    let (directory, backend) = repository_with_source().await;
    git(directory.path(), &["mv", "src/old.txt", "src/new.txt"]);
    git(directory.path(), &["commit", "-q", "-m", "rename"]);
    let oid = git_stdout(directory.path(), &["rev-parse", "HEAD"])
        .trim()
        .to_owned();

    let diff = backend
        .diff(
            directory.path(),
            &rename_request(DiffTarget::Commit {
                oid,
                parent_index: 0,
            }),
        )
        .await
        .expect("commit diff");

    assert_eq!(diff.status, ChangeKind::Renamed);
    assert!(diff.hunks.is_empty());
}
