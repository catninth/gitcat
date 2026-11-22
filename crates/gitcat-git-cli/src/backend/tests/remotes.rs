//! Clone, fetch, pull, push and the credentials they authenticate with.

use super::*;

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
                skip_submodules: false,
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
async fn fetch_and_pull_ignore_foreign_refs_to_pruned_objects() {
    let (directory, backend, _) = committed_repository().await;
    let bare = tempdir().expect("bare remote");
    git(bare.path(), &["init", "--bare", "--quiet"]);
    let bare_path = bare.path().to_string_lossy().into_owned();
    git(directory.path(), &["remote", "add", "origin", &bare_path]);
    git(
        directory.path(),
        &["push", "--quiet", "-u", "origin", "main"],
    );

    let updater_parent = tempdir().expect("updater parent");
    git(
        updater_parent.path(),
        &[
            "clone", "--quiet", "--branch", "main", &bare_path, "updater",
        ],
    );
    let updater = updater_parent.path().join("updater");
    git(&updater, &["config", "user.name", "GitCat Test"]);
    git(&updater, &["config", "user.email", "gitcat@example.test"]);
    git(&updater, &["commit", "--allow-empty", "-m", "upstream"]);
    git(&updater, &["push", "--quiet", "origin", "main"]);
    let upstream_oid = git_stdout(&updater, &["rev-parse", "HEAD"]);

    // Another tool's checkpoint ref whose object was pruned, as Codex leaves behind.
    let foreign = directory.path().join(".git/refs/codex/checkpoints");
    fs::create_dir_all(&foreign).expect("create foreign ref namespace");
    fs::write(
        foreign.join("gone"),
        "d733a34e5816afc70ebbcccd0ad5852848dd2fe6\n",
    )
    .expect("write broken ref");

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
        .expect("fetch despite a broken foreign ref");
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "refs/remotes/origin/main"]),
        upstream_oid
    );

    git(
        &updater,
        &["commit", "--allow-empty", "-m", "upstream again"],
    );
    git(&updater, &["push", "--quiet", "origin", "main"]);
    let pulled_oid = git_stdout(&updater, &["rev-parse", "HEAD"]);
    backend
        .pull(
            directory.path(),
            &PullOptions {
                skip_submodules: false,
                remote: None,
                branch: None,
                mode: PullMode::FastForwardOnly,
                prune: false,
                autostash: false,
            },
            CancellationToken::new(),
        )
        .await
        .expect("pull despite a broken foreign ref");
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "HEAD"]),
        pulled_oid
    );
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
                skip_submodules: false,
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
                skip_submodules: false,
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
                skip_submodules: false,
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
async fn sign_in_recovery_uses_the_commands_host_even_without_a_stored_token() {
    let (directory, backend, _) = committed_repository().await;
    git(
        directory.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ],
    );
    git(
        directory.path(),
        &[
            "remote",
            "add",
            "other",
            "https://gitlab.com/owner/repo.git",
        ],
    );
    git(
        directory.path(),
        &["remote", "add", "ssh", "git@github.com:owner/repo.git"],
    );
    let remotes = backend.remotes(directory.path()).await.expect("remotes");

    let options = backend
        .network_options(&remotes, Some("origin"), true)
        .await;
    assert_eq!(options.remote_host.as_deref(), Some("github.com"));
    assert!(options.credential.is_none());
    // Exercise the real mutation error path without contacting GitHub or
    // changing credentials. Git's alias emits the observed permission denial.
    let failure = backend
        .mutate_network(
            directory.path(),
            os_args(&[
                "-c",
                "alias.denied=!echo 'remote: Permission to owner/repo.git denied to user.' >&2; exit 1",
                "denied",
            ]),
            CancellationToken::new(),
            options.clone(),
        )
        .await
        .expect_err("permission denial");
    assert_eq!(failure.code, ErrorCode::ProtectedOperation);
    assert_eq!(failure.recovery_actions[0].kind, "sign_in_github");
    assert!(
        backend
            .network_options(&remotes, None, true)
            .await
            .remote_host
            .is_none()
    );
    assert!(
        backend
            .network_options(&remotes, Some("ssh"), true)
            .await
            .remote_host
            .is_none()
    );

    for code in [
        ErrorCode::AuthenticationRequired,
        ErrorCode::ProtectedOperation,
    ] {
        let error = ApiError::new(code, "access denied")
            .with_recovery_action("open_settings", "Check the hosting service connection");
        let recovered = network_recovery(error.clone(), options.remote_host.as_deref());
        assert_eq!(recovered.recovery_actions[0].kind, "sign_in_github");
        for host in [None, Some("gitlab.com"), Some("github.example.test")] {
            assert_eq!(network_recovery(error.clone(), host), error);
        }
    }
    let protected = ApiError::new(ErrorCode::ProtectedOperation, "protected branch");
    assert_eq!(
        network_recovery(protected.clone(), Some("github.com")),
        protected
    );
    let rejected = ApiError::new(ErrorCode::NonFastForward, "fetch first")
        .with_recovery_action("pull", "Pull, then push again");
    assert_eq!(
        network_recovery(rejected.clone(), Some("github.com")),
        rejected
    );

    git(
        directory.path(),
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            "https://gitlab.com/owner/repo.git",
        ],
    );
    let remotes = backend
        .remotes(directory.path())
        .await
        .expect("remotes with push URL");
    assert_eq!(
        backend
            .network_options(&remotes, Some("origin"), true)
            .await
            .remote_host
            .as_deref(),
        Some("gitlab.com")
    );
    assert_eq!(
        backend
            .network_options(&remotes, Some("origin"), false)
            .await
            .remote_host
            .as_deref(),
        Some("github.com")
    );

    let mixed: Vec<_> = remotes
        .into_iter()
        .filter(|remote| remote.name != "other")
        .collect();
    assert!(
        backend
            .network_options(&mixed, None, false)
            .await
            .remote_host
            .is_none()
    );
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
    assert!(renewal_candidate(&refused(ErrorCode::ProtectedOperation), &options).is_none());

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

#[tokio::test]
async fn delete_remote_branch_removes_it_on_the_remote_and_locally() {
    let (directory, backend, _) = committed_repository().await;
    let bare = tempdir().expect("bare remote");
    git(bare.path(), &["init", "--bare", "--quiet"]);
    let bare_path = bare.path().to_string_lossy().into_owned();
    git(directory.path(), &["remote", "add", "origin", &bare_path]);
    git(
        directory.path(),
        &["push", "--quiet", "-u", "origin", "main"],
    );
    git(directory.path(), &["branch", "feature/gone"]);
    git(
        directory.path(),
        &["push", "--quiet", "origin", "feature/gone"],
    );
    // A tag of the same name must survive: only the branch is deleted.
    git(directory.path(), &["tag", "feature/gone"]);
    git(
        directory.path(),
        &["push", "--quiet", "origin", "refs/tags/feature/gone"],
    );

    let refused = backend
        .delete_remote_branch(directory.path(), "origin", "feature/gone", false)
        .await
        .expect_err("unconfirmed delete refused");
    assert_eq!(refused.code, ErrorCode::ProtectedOperation);

    backend
        .delete_remote_branch(directory.path(), "origin", "feature/gone", true)
        .await
        .expect("delete remote branch");

    let remote_heads = git_stdout(bare.path(), &["for-each-ref", "--format=%(refname)"]);
    assert!(
        !remote_heads.contains("refs/heads/feature/gone"),
        "{remote_heads}"
    );
    assert!(
        remote_heads.contains("refs/tags/feature/gone"),
        "{remote_heads}"
    );
    let tracking = git_stdout(
        directory.path(),
        &["for-each-ref", "--format=%(refname)", "refs/remotes/origin"],
    );
    assert!(!tracking.contains("feature/gone"), "{tracking}");
    // The local branch is not touched.
    git(
        directory.path(),
        &["rev-parse", "--verify", "refs/heads/feature/gone"],
    );

    let unknown = backend
        .delete_remote_branch(directory.path(), "nowhere", "main", true)
        .await
        .expect_err("unknown remote refused");
    assert_ne!(unknown.code, ErrorCode::ProtectedOperation);
}
