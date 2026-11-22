//! Conflict preflight, the conflict editor and resolution of conflicted paths.

use super::*;

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
