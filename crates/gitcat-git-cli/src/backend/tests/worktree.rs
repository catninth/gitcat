//! Working tree diffs, line stats and single-line staging.

use super::*;

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
        old_path: None,
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
                old_path: None,
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
                old_path: None,
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
                old_path: None,
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
                old_path: None,
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

fn worktree_diff_request(path: &str, target: DiffTarget) -> DiffRequest {
    DiffRequest {
        target,
        path: path.into(),
        context_lines: 3,
        ignore_whitespace: false,
        max_bytes: 1 << 20,
        whole_file: false,
        old_path: None,
    }
}

fn changed_lines(diff: &FileDiff, wanted: &[&str]) -> Vec<DiffLine> {
    diff.hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .filter(|line| {
            matches!(line.kind, DiffLineKind::Addition | DiffLineKind::Deletion)
                && wanted.contains(&line.content.as_str())
        })
        .cloned()
        .collect()
}

#[tokio::test]
async fn single_lines_stage_unstage_and_discard() {
    let (directory, backend, _) = committed_repository().await;
    let root = directory.path();
    git(root, &["config", "core.autocrlf", "false"]);
    fs::write(root.join("lines.txt"), "a\nb\nc\nd\n").expect("write base");
    backend
        .stage_paths(root, &["lines.txt".into()])
        .await
        .expect("stage base");
    backend
        .create_commit(
            root,
            &CommitOptions {
                message: "lines".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("commit base");
    fs::write(root.join("lines.txt"), "a\nB\nc\nD\ne\n").expect("edit");

    let unstaged = backend
        .diff(
            root,
            &worktree_diff_request("lines.txt", DiffTarget::Worktree),
        )
        .await
        .expect("worktree diff");
    backend
        .apply_diff_lines(
            root,
            &LinePatchRequest {
                path: "lines.txt".into(),
                action: LinePatchAction::Stage,
                lines: changed_lines(&unstaged, &["b", "B"]),
            },
        )
        .await
        .expect("stage one change");
    assert_eq!(git_stdout(root, &["show", ":lines.txt"]), "a\nB\nc\nd");

    let staged = backend
        .diff(
            root,
            &worktree_diff_request("lines.txt", DiffTarget::Staged),
        )
        .await
        .expect("staged diff");
    backend
        .apply_diff_lines(
            root,
            &LinePatchRequest {
                path: "lines.txt".into(),
                action: LinePatchAction::Unstage,
                lines: changed_lines(&staged, &["b"]),
            },
        )
        .await
        .expect("unstage the deletion only");
    assert_eq!(git_stdout(root, &["show", ":lines.txt"]), "a\nb\nB\nc\nd");

    let unstaged = backend
        .diff(
            root,
            &worktree_diff_request("lines.txt", DiffTarget::Worktree),
        )
        .await
        .expect("worktree diff again");
    backend
        .apply_diff_lines(
            root,
            &LinePatchRequest {
                path: "lines.txt".into(),
                action: LinePatchAction::Discard,
                lines: changed_lines(&unstaged, &["e"]),
            },
        )
        .await
        .expect("discard the appended line");
    assert_eq!(
        fs::read_to_string(root.join("lines.txt")).expect("read worktree"),
        "a\nB\nc\nD\n"
    );
    assert_eq!(git_stdout(root, &["show", ":lines.txt"]), "a\nb\nB\nc\nd");
}

#[tokio::test]
async fn single_lines_of_an_untracked_file_stage_and_discard() {
    let (directory, backend, _) = committed_repository().await;
    let root = directory.path();
    fs::write(root.join("new.txt"), "one\ntwo\nthree\n").expect("write untracked");

    let diff = backend
        .diff(
            root,
            &worktree_diff_request("new.txt", DiffTarget::Worktree),
        )
        .await
        .expect("untracked diff");
    backend
        .apply_diff_lines(
            root,
            &LinePatchRequest {
                path: "new.txt".into(),
                action: LinePatchAction::Discard,
                lines: changed_lines(&diff, &["two"]),
            },
        )
        .await
        .expect("discard from untracked");
    assert_eq!(
        fs::read_to_string(root.join("new.txt")).expect("read worktree"),
        "one\nthree\n"
    );

    let diff = backend
        .diff(
            root,
            &worktree_diff_request("new.txt", DiffTarget::Worktree),
        )
        .await
        .expect("untracked diff again");
    backend
        .apply_diff_lines(
            root,
            &LinePatchRequest {
                path: "new.txt".into(),
                action: LinePatchAction::Stage,
                lines: changed_lines(&diff, &["three"]),
            },
        )
        .await
        .expect("stage from untracked");
    assert_eq!(git_stdout(root, &["show", ":new.txt"]), "three");
}

#[tokio::test]
async fn a_stale_line_is_refused_and_leaves_an_untracked_file_untracked() {
    let (directory, backend, _) = committed_repository().await;
    let root = directory.path();
    fs::write(root.join("new.txt"), "one\n").expect("write untracked");
    let error = backend
        .apply_diff_lines(
            root,
            &LinePatchRequest {
                path: "new.txt".into(),
                action: LinePatchAction::Stage,
                lines: vec![DiffLine {
                    kind: DiffLineKind::Addition,
                    old_line: None,
                    new_line: Some(1),
                    content: "something else".into(),
                }],
            },
        )
        .await
        .expect_err("stale line");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
    assert_eq!(git_stdout(root, &["ls-files", "--", "new.txt"]), "");
}
