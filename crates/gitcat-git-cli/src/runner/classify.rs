//! Turning a failed Git command into an `ApiError`: its code, the wording the
//! UI shows, the recovery steps it offers, and output with secrets redacted.

use super::*;

pub(super) fn git_failure_error(output: &GitCommandOutput) -> ApiError {
    let exit = output.status.code().map_or_else(
        || "terminated".to_owned(),
        |code| format!("exit code {code}"),
    );
    classify_failure(
        &output.stderr_lossy_redacted(),
        &redact_sensitive(&output.stdout_lossy()),
        &exit,
    )
}

fn classify_failure(stderr: &str, stdout: &str, exit: &str) -> ApiError {
    let lower = format!("{stderr}\n{stdout}").to_ascii_lowercase();
    let (code, message) = if lower.contains("not a git repository") {
        (
            ErrorCode::InvalidRepository,
            "The selected folder is not a Git repository",
        )
    } else if lower.contains("index.lock") || lower.contains("another git process") {
        (
            ErrorCode::RepositoryBusy,
            "The repository is locked by another Git process",
        )
    } else if lower.contains("failed to remove")
        && (lower.contains("invalid argument")
            || lower.contains("access is denied")
            || lower.contains("permission denied")
            || lower.contains("being used by another process"))
    {
        (
            ErrorCode::RepositoryBusy,
            "Some files could not be removed because another process is using them",
        )
    } else if lower.contains("no tracking information")
        || lower.contains("has no upstream branch")
        || lower.contains("no upstream configured")
    {
        (
            ErrorCode::UpstreamMissing,
            "The branch has no upstream configured",
        )
    } else if lower.contains("authentication failed")
        || lower.contains("could not read username")
        || lower.contains("permission denied (publickey)")
        || lower.contains("terminal prompts disabled")
        || lower.contains("the requested url returned error: 401")
    {
        (
            ErrorCode::AuthenticationRequired,
            "Remote authentication failed",
        )
    } else if lower.contains("stale info") || lower.contains("remote ref updated since checkout") {
        // The lease on the remote did not hold: it moved after the last fetch,
        // or it moved into work this branch never had. Either way something is
        // there that a forced push would have discarded unseen.
        (
            ErrorCode::NonFastForward,
            "The remote moved since the last fetch, so the force push was refused",
        )
    } else if lower.contains("non-fast-forward") || lower.contains("fetch first") {
        (
            ErrorCode::NonFastForward,
            "The remote rejected a non-fast-forward update",
        )
    } else if lower.contains("without 'workflow' scope") {
        (
            ErrorCode::ProtectedOperation,
            "The remote rejected the push because the sign-in may not change workflow files",
        )
    } else if remote_access_denied(&lower) {
        (
            ErrorCode::ProtectedOperation,
            "The remote denied access; check your account and repository permissions",
        )
    } else if lower.contains("remote rejected") || lower.contains("protected branch") {
        (
            ErrorCode::ProtectedOperation,
            "The remote rejected the push",
        )
    } else if lower.contains("would be overwritten") || lower.contains("local changes") {
        (
            ErrorCode::DirtyWorktree,
            "Local changes prevent this Git operation",
        )
    } else if lower.contains("conflict") || lower.contains("unmerged") {
        (
            ErrorCode::ConflictsPresent,
            "Git stopped because conflicts require attention",
        )
    } else if lower.contains("could not resolve host")
        || lower.contains("failed to connect")
        || lower.contains("unable to access")
        || lower.contains("connection timed out")
        || lower.contains("connection reset")
    {
        (ErrorCode::NetworkFailed, "The remote could not be reached")
    } else if lower.contains("bad revision")
        || lower.contains("unknown revision")
        || lower.contains("needed a single revision")
        || lower.contains("ambiguous argument")
    {
        (
            ErrorCode::InvalidRevision,
            "The selected Git revision is invalid",
        )
    } else {
        (ErrorCode::GitCommandFailed, "Git command failed")
    };

    // `push --porcelain` reports why a ref was rejected on stdout, so stderr
    // alone ends at "failed to push some refs" with the reason missing. Stdout
    // goes last because the cap keeps the tail.
    let details = [collapse_progress(stderr), collapse_progress(stdout)]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let details = tail_chars(&details, STDERR_DETAILS_CAP);
    let details = if details.trim().is_empty() {
        exit.to_owned()
    } else {
        format!("{exit}: {}", details.trim())
    };
    recovery_actions(code, &lower)
        .into_iter()
        .fold(ApiError::new(code, message), |error, (kind, label)| {
            error.with_recovery_action(kind, label)
        })
        .with_details(details)
}

fn remote_access_denied(lower: &str) -> bool {
    lower.contains("the requested url returned error: 403")
        || (lower.contains("remote: permission to ") && lower.contains(" denied to "))
        || lower.contains("remote: write access to repository not granted")
}

/// What to offer doing next about a failure, where the next step follows from
/// the failure itself.
///
/// The output of one Git command is all there is to go on, so an action is
/// offered only where the wording says which command failed: "has no upstream
/// branch" is a push, while a pull's "no tracking information" leaves the user
/// a choice this cannot make for them.
fn recovery_actions(code: ErrorCode, lower: &str) -> Vec<(&'static str, &'static str)> {
    match code {
        // A refused lease is the one place where overwriting is a step the user
        // may legitimately want next -- they already asked to force once -- so
        // it is offered beside the safe answer, never instead of it.
        ErrorCode::NonFastForward
            if lower.contains("stale info")
                || lower.contains("remote ref updated since checkout") =>
        {
            vec![
                ("pull", "Pull, then push again"),
                ("push_force", "Force push, overwriting the remote"),
            ]
        }
        ErrorCode::NonFastForward => vec![("pull", "Pull, then push again")],
        ErrorCode::UpstreamMissing if lower.contains("has no upstream branch") => {
            vec![("push_set_upstream", "Push and set the upstream branch")]
        }
        ErrorCode::AuthenticationRequired => {
            vec![("open_settings", "Check the hosting service connection")]
        }
        ErrorCode::ProtectedOperation if lower.contains("without 'workflow' scope") => {
            vec![("open_settings", "Sign in again to allow workflow files")]
        }
        ErrorCode::ProtectedOperation if remote_access_denied(lower) => {
            vec![("open_settings", "Check the hosting service connection")]
        }
        _ => Vec::new(),
    }
}

pub(crate) fn redact_sensitive(input: &str) -> String {
    let mut result = input.to_owned();
    for scheme in ["https://", "http://", "ssh://"] {
        let mut search_from = 0;
        while let Some(relative_start) = result[search_from..].find(scheme) {
            let credentials_start = search_from + relative_start + scheme.len();
            let tail = &result[credentials_start..];
            let authority_end = tail
                .find(['/', '\\', ' ', '\n', '\r', '\t'])
                .unwrap_or(tail.len());
            let authority = &tail[..authority_end];
            let Some(at) = authority.rfind('@') else {
                search_from = credentials_start + authority_end;
                continue;
            };
            let credentials_end = credentials_start + at;
            result.replace_range(credentials_start..credentials_end, "***");
            search_from = credentials_start + 3 + 1;
        }
    }
    for marker in [
        "token=",
        "access_token=",
        "password=",
        "passwd=",
        "secret=",
        "token:",
        "password:",
        "authorization: bearer ",
        "bearer ",
    ] {
        redact_after_marker(&mut result, marker);
    }
    result
}

fn redact_after_marker(value: &mut String, marker: &str) {
    let mut search_from = 0;
    loop {
        let lower = value.to_ascii_lowercase();
        let Some(relative) = lower[search_from..].find(marker) else {
            break;
        };
        let secret_start = search_from + relative + marker.len();
        let secret_end = value[secret_start..]
            .find(|character: char| {
                character.is_whitespace()
                    || matches!(character, '&' | ';' | ',' | '\'' | '"' | '<' | '>')
            })
            .map_or(value.len(), |relative_end| secret_start + relative_end);
        if secret_start == secret_end {
            search_from = secret_start;
            continue;
        }
        value.replace_range(secret_start..secret_end, "***");
        search_from = secret_start + 3;
    }
}

/// Git draws progress by rewriting one line with carriage returns, so a
/// captured stream holds every intermediate percentage. Only the last segment
/// of a line was ever meant to be read.
fn collapse_progress(input: &str) -> String {
    input
        .split('\n')
        .filter_map(|line| {
            line.rsplit('\r')
                .find(|segment| !segment.trim().is_empty())
                .map(str::trim_end)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tail_chars(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_owned();
    }
    let mut start = input.len() - max_bytes;
    while !input.is_char_boundary(start) {
        start += 1;
    }
    input[start..].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_failure_keeps_the_porcelain_rejection_reason() {
        // Git sends the progress meter and the summary to stderr, while
        // --porcelain puts the per-ref status on stdout.
        let stderr = concat!(
            "Counting objects:  50% (1/2)\rCounting objects: 100% (2/2), done.\n",
            "error: failed to push some refs to 'https://example.test/repo.git'\n"
        );
        let stdout = concat!(
            "To https://example.test/repo.git\n",
            "!\trefs/heads/main:refs/heads/main\t[remote rejected] ",
            "(refusing to allow an OAuth App to create or update workflow ",
            "'.github/workflows/ci.yml' without 'workflow' scope)\nDone\n"
        );

        let error = classify_failure(stderr, stdout, "exit code 1");

        assert_eq!(error.code, ErrorCode::ProtectedOperation);
        let details = error.details.unwrap_or_default();
        assert!(details.contains("without 'workflow' scope"), "{details}");
        // The progress meter collapses to its final state.
        assert!(details.contains("Counting objects: 100% (2/2), done."));
        assert!(!details.contains("Counting objects:  50%"));
    }

    #[test]
    fn non_fast_forward_still_wins_over_the_generic_rejection() {
        let error = classify_failure(
            "error: failed to push some refs",
            "!\trefs/heads/main:refs/heads/main\t[rejected] (fetch first)",
            "exit code 1",
        );

        assert_eq!(error.code, ErrorCode::NonFastForward);
    }

    #[test]
    fn remote_permission_denials_offer_connection_settings() {
        for stderr in [
            concat!(
                "remote: Permission to ikoliHU/clipcat.git denied to ikoliHU.\n",
                "fatal: unable to access 'https://github.com/ikoliHU/clipcat.git/': ",
                "The requested URL returned error: 403",
            ),
            "remote: Permission to owner/repo.git denied to user.",
            "remote: Write access to repository not granted.",
            "fatal: unable to access 'https://example.test/repo.git/': The requested URL returned error: 403",
        ] {
            let error = classify_failure(stderr, "", "exit code 128");
            // An access denial does not imply an expired credential. Keep it
            // out of the automatic authentication-renewal path.
            assert_eq!(error.code, ErrorCode::ProtectedOperation, "{stderr}");
            assert!(error.message.contains("denied access"));
            assert_eq!(error.recovery_actions.len(), 1);
            assert_eq!(error.recovery_actions[0].kind, "open_settings");
            assert!(error.details.as_deref().unwrap().contains(stderr));
        }
    }

    #[test]
    fn http_authentication_failure_is_distinct_from_network_failure() {
        let error = classify_failure(
            "fatal: unable to access 'https://example.test/repo.git/': The requested URL returned error: 401",
            "",
            "exit code 128",
        );
        assert_eq!(error.code, ErrorCode::AuthenticationRequired);
        assert_eq!(error.recovery_actions[0].kind, "open_settings");

        for reason in [
            "Could not resolve host: example.test",
            "Failed to connect to example.test port 443",
            "The requested URL returned error: 500",
        ] {
            let error = classify_failure(
                &format!("fatal: unable to access 'https://example.test/repo.git/': {reason}"),
                "",
                "exit code 128",
            );
            assert_eq!(error.code, ErrorCode::NetworkFailed, "{reason}");
            assert!(error.recovery_actions.is_empty());
        }
    }

    #[test]
    fn a_rejected_push_offers_the_step_that_unblocks_it() {
        let non_fast_forward = classify_failure(
            "error: failed to push some refs",
            "!	refs/heads/main:refs/heads/main	[rejected] (fetch first)",
            "exit code 1",
        );
        assert_eq!(
            non_fast_forward
                .recovery_actions
                .iter()
                .map(|action| action.kind.as_str())
                .collect::<Vec<_>>(),
            ["pull"]
        );

        let no_upstream = classify_failure(
            "fatal: The current branch work has no upstream branch.",
            "",
            "exit code 128",
        );
        assert_eq!(no_upstream.code, ErrorCode::UpstreamMissing);
        assert_eq!(
            no_upstream
                .recovery_actions
                .iter()
                .map(|action| action.kind.as_str())
                .collect::<Vec<_>>(),
            ["push_set_upstream"]
        );

        // A pull without tracking information is the same code, but setting an
        // upstream is not the only sensible answer, so nothing is offered.
        let pull = classify_failure(
            "There is no tracking information for the current branch.",
            "",
            "exit code 1",
        );
        assert_eq!(pull.code, ErrorCode::UpstreamMissing);
        assert!(pull.recovery_actions.is_empty());
    }

    #[test]
    fn a_missing_workflow_scope_points_at_signing_in_again() {
        let error = classify_failure(
            "error: failed to push some refs",
            "!	refs/heads/main:refs/heads/main	[remote rejected] (refusing to allow an OAuth App to create or update workflow '.github/workflows/ci.yml' without 'workflow' scope)",
            "exit code 1",
        );

        assert_eq!(error.code, ErrorCode::ProtectedOperation);
        let action = error.recovery_actions.first().expect("recovery action");
        assert_eq!(action.kind, "open_settings");
        assert_eq!(action.label, "Sign in again to allow workflow files");

        // A protected branch has no step GitCat can offer.
        let protected = classify_failure(
            "! [remote rejected] main -> main (protected branch hook declined)",
            "",
            "exit code 1",
        );
        assert_eq!(protected.code, ErrorCode::ProtectedOperation);
        assert!(protected.recovery_actions.is_empty());
    }

    #[test]
    fn redacts_url_userinfo() {
        let value = redact_sensitive(
            "fatal: unable to access 'https://alice:secret@example.test/repo': denied",
        );
        assert_eq!(
            value,
            "fatal: unable to access 'https://***@example.test/repo': denied"
        );
    }

    #[test]
    fn keeps_safe_urls() {
        assert_eq!(
            redact_sensitive("https://example.test/repo"),
            "https://example.test/repo"
        );
    }

    #[test]
    fn redacts_tokens_passwords_and_bearer_values() {
        let value =
            redact_sensitive("Authorization: Bearer abc.def token=secret&password=hunter2 next");
        assert_eq!(
            value,
            "Authorization: Bearer *** token=***&password=*** next"
        );
    }
}
