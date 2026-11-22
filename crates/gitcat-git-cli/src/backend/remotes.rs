//! Remotes, the credentials network commands use, and fetch, pull and push.

use super::*;

impl GitCliBackend {
    pub(super) async fn remotes(&self, path: &Path) -> ApiResult<Vec<RemoteInfo>> {
        let output = self.read(Some(path), os_args(&["remote"])).await?;
        let mut remotes = Vec::new();
        for name in output
            .stdout_lossy()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            validate_remote_name(name)?;
            let fetch = self
                .read(
                    Some(path),
                    vec!["remote".into(), "get-url".into(), name.into()],
                )
                .await?;
            let fetch_url = fetch.stdout_lossy().trim().to_owned();
            validate_remote_url(&fetch_url)?;
            let mut push_options = GitRunOptions::read_only(READ_OUTPUT_CAP);
            push_options.allow_failure = true;
            let push = self
                .runner
                .run(
                    Some(path),
                    &[
                        "remote".into(),
                        "get-url".into(),
                        "--push".into(),
                        name.into(),
                    ],
                    None,
                    CancellationToken::new(),
                    push_options,
                )
                .await?;
            let push_url = if push.success() {
                push.stdout_lossy().trim().to_owned()
            } else {
                fetch_url.clone()
            };
            validate_remote_url(&push_url)?;
            let fetch_url = redact_sensitive(&fetch_url);
            let push_url = redact_sensitive(&push_url);
            // Fetch names where the repository lives; push may point at a fork
            // remote that shares no host, so it is only the fallback.
            let url = parse_remote_url(&fetch_url).or_else(|| parse_remote_url(&push_url));
            let forge = url.as_ref().map(detect_forge).unwrap_or_default();
            let web_url = url.as_ref().and_then(|parts| forge_web_url(parts, forge));
            remotes.push(RemoteInfo {
                name: name.to_owned(),
                fetch_url,
                push_url,
                url,
                forge,
                web_url,
            });
        }
        Ok(remotes)
    }

    /// Checks the selection and hands back the remotes it was checked against,
    /// so a caller that also needs their URLs does not ask Git twice.
    pub(crate) async fn validate_remote_selection(
        &self,
        path: &Path,
        remote: Option<&str>,
    ) -> ApiResult<Vec<RemoteInfo>> {
        let remotes = self.remotes(path).await?;
        if let Some(remote) = remote {
            validate_remote_name(remote)?;
            if !remotes.iter().any(|entry| entry.name == remote) {
                return Err(ApiError::new(
                    ErrorCode::InvalidSettings,
                    "Selected remote does not exist",
                ));
            }
        } else if remotes.is_empty() {
            return Err(ApiError::new(
                ErrorCode::UpstreamMissing,
                "Repository has no configured remote",
            ));
        }
        Ok(remotes)
    }

    /// Options for a network command, carrying a token when it is unambiguous
    /// which host the command will contact.
    ///
    /// A command that reaches several hosts -- a push or pull with no explicit
    /// remote in a repository whose remotes sit on different services -- is
    /// left to the credential helpers Git already has: installing one clears
    /// them for the whole command, and a token for one host is no use to
    /// another. A fetch never lands here with more than one host, because it
    /// runs one command per remote.
    pub(crate) async fn network_options(
        &self,
        remotes: &[RemoteInfo],
        remote: Option<&str>,
        for_push: bool,
    ) -> GitRunOptions {
        let mut options = GitRunOptions::network(NETWORK_OUTPUT_CAP);
        let selected_hosts: Vec<Option<String>> = remotes
            .iter()
            .filter(|entry| remote.is_none_or(|name| entry.name == name))
            .map(|entry| {
                let parsed = parse_remote_url(if for_push {
                    &entry.push_url
                } else {
                    &entry.fetch_url
                });
                password_host(parsed.as_ref()).map(str::to_owned)
            })
            .collect();
        let hosts: BTreeSet<&str> = selected_hosts
            .iter()
            .filter_map(|host| host.as_deref())
            .collect();
        let mut hosts = hosts.into_iter();
        let (Some(host), None) = (hosts.next(), hosts.next()) else {
            return options;
        };

        // If Git might choose an SSH or local remote, do not offer an OAuth
        // sign-in for an unrelated HTTPS remote in the same repository.
        if selected_hosts.iter().all(Option::is_some) {
            options.remote_host = Some(host.to_owned());
        }
        let Some(source) = self.credentials.as_ref() else {
            return options;
        };
        if let Some(token) = source.token_for(host).await {
            options.credential = Some(HostCredential {
                host: host.to_owned(),
                token,
            });
        }
        options
    }

    /// Options for a clone, whose host comes from the URL rather than from a
    /// configured remote.
    pub(crate) async fn clone_options(&self, url: &str) -> GitRunOptions {
        let mut options = GitRunOptions::network(NETWORK_OUTPUT_CAP);
        let Some(source) = self.credentials.as_ref() else {
            return options;
        };
        let parsed = parse_remote_url(url);
        let Some(host) = password_host(parsed.as_ref()) else {
            return options;
        };

        if let Some(token) = source.token_for(host).await {
            options.credential = Some(HostCredential {
                host: host.to_owned(),
                token,
            });
        }
        options
    }

    /// The commit a push would send, and the commit its destination already
    /// has -- when both are knowable without contacting the remote.
    ///
    /// Gittyup answers this locally before spending a network round trip, and
    /// so does this: the destination's remote-tracking ref already names a
    /// commit, and a push that would send the one the branch is on has nothing
    /// to do. The tracking ref can be stale, which is the price of not asking
    /// the remote; that is the same data the ahead/behind counters are drawn
    /// from, so a user acting on what the window shows gets a consistent
    /// answer.
    ///
    /// `None` means the shape is one this cannot answer for -- a detached
    /// HEAD, a `push.default` that sends more than one branch, no destination
    /// to derive. Those go to Git unchanged.
    pub(super) async fn push_destination_state(
        &self,
        path: &Path,
        options: &PushOptions,
    ) -> ApiResult<Option<(String, String)>> {
        // A named branch is both the source and the destination name; only a
        // push that names none is about whatever HEAD is on.
        let branch = match &options.branch {
            Some(branch) => {
                self.validate_branch_name(branch).await?;
                branch.clone()
            }
            None => {
                let head = self
                    .read_allow_failure(
                        Some(path),
                        os_args(&["symbolic-ref", "--quiet", "--short", "HEAD"]),
                    )
                    .await?;
                if !head.success() {
                    return Ok(None);
                }
                let head = head.stdout_lossy().trim().to_owned();
                if head.is_empty() {
                    return Ok(None);
                }
                head
            }
        };

        let config = |key: String| async move {
            let output = self
                .read_allow_failure(Some(path), os_args(&["config", "--get", key.as_str()]))
                .await?;
            let value = output.stdout_lossy().trim().to_owned();
            ApiResult::Ok(if output.success() && !value.is_empty() {
                Some(value)
            } else {
                None
            })
        };

        // `matching` and `nothing` do not describe one branch going to one
        // place, so nothing here can stand in for the command.
        let mode = config("push.default".to_owned())
            .await?
            .unwrap_or_else(|| "simple".to_owned());
        if options.branch.is_none()
            && !matches!(
                mode.as_str(),
                "simple" | "current" | "upstream" | "tracking"
            )
        {
            return Ok(None);
        }

        // The remote the push actually goes to. An explicitly chosen one wins,
        // which is what keeps an upstream on a different remote out of the
        // comparison.
        let remote = match &options.remote {
            Some(remote) => remote.clone(),
            None => {
                let configured = match config(format!("branch.{branch}.pushRemote")).await? {
                    Some(value) => Some(value),
                    None => match config("remote.pushDefault".to_owned()).await? {
                        Some(value) => Some(value),
                        None => config(format!("branch.{branch}.remote")).await?,
                    },
                };
                match configured {
                    Some(remote) => remote,
                    None => return Ok(None),
                }
            }
        };

        let destination = match &options.branch {
            Some(branch) => branch.clone(),
            None if matches!(mode.as_str(), "upstream" | "tracking") => {
                // Only the branch's own upstream names the destination here,
                // and only on the remote that upstream lives on.
                if config(format!("branch.{branch}.remote")).await?.as_deref() != Some(&remote) {
                    return Ok(None);
                }
                match config(format!("branch.{branch}.merge")).await? {
                    Some(merge) => merge
                        .strip_prefix("refs/heads/")
                        .unwrap_or(&merge)
                        .to_owned(),
                    None => return Ok(None),
                }
            }
            None => branch.clone(),
        };

        let local = self
            .rev_parse_commit(path, &format!("refs/heads/{branch}"))
            .await?;
        let tracking = self
            .rev_parse_commit(path, &format!("refs/remotes/{remote}/{destination}"))
            .await?;
        Ok(match (local, tracking) {
            (Some(local), Some(tracking)) => Some((local, tracking)),
            _ => None,
        })
    }

    /// Runs a network mutation, and gives a refused credential exactly one
    /// second chance.
    ///
    /// The token GitCat holds can die between commands -- revoked on the
    /// service's page, or superseded by a sign-in elsewhere -- and the user has
    /// no way to tell that apart from "the push failed". A rejection is worth
    /// one renewal: if the source can produce a different token, the same
    /// command runs again with it. One retry, and only when the token actually
    /// changed, so a service that keeps refusing is reported rather than asked
    /// twice.
    pub(crate) async fn mutate_network(
        &self,
        path: &Path,
        args: Vec<OsString>,
        cancellation: CancellationToken,
        options: GitRunOptions,
    ) -> ApiResult<MutationResult> {
        let result = self
            .mutate_with(
                path,
                args.clone(),
                None,
                cancellation.clone(),
                options.clone(),
            )
            .await;
        let (Some(credential), Some(source)) = (
            renewal_candidate(&result, &options),
            self.credentials.as_ref(),
        ) else {
            return result;
        };
        let renewed = source.renew_rejected(&credential.host).await;
        let Some(token) = renewed_token(credential, renewed) else {
            return result;
        };
        let mut retry = options.clone();
        retry.credential = Some(HostCredential {
            host: credential.host.clone(),
            token,
        });
        self.mutate_with(path, args, None, cancellation, retry)
            .await
    }

    pub(super) async fn add_remote_inner(
        &self,
        path: &Path,
        name: &str,
        url: &str,
    ) -> ApiResult<MutationResult> {
        validate_remote_name(name)?;
        validate_remote_url(url)?;
        self.mutate(
            path,
            vec!["remote".into(), "add".into(), name.into(), url.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn fetch_inner(
        &self,
        path: &Path,
        options: &FetchOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        let remotes = self
            .validate_remote_selection(path, options.remote.as_deref())
            .await?;
        // Every remote is fetched by its own command rather than through
        // `fetch --all`, so each one reaches a single host and can carry the
        // token issued for that host.
        let targets: Vec<String> = match &options.remote {
            Some(remote) => vec![remote.clone()],
            None => remotes.iter().map(|entry| entry.name.clone()).collect(),
        };
        let mut result = None;
        for target in targets {
            let mut args = download_args("fetch");
            if options.prune {
                args.push("--prune".into());
            }
            if options.tags {
                args.push("--tags".into());
            }
            args.push("--".into());
            args.push(target.as_str().into());
            let run = self
                .network_options(&remotes, Some(target.as_str()), false)
                .await;
            result = Some(
                self.mutate_network(path, args, cancellation.clone(), run)
                    .await?,
            );
        }
        result.ok_or_else(|| {
            ApiError::new(
                ErrorCode::UpstreamMissing,
                "Repository has no configured remote",
            )
        })
    }

    pub(super) async fn pull_inner(
        &self,
        path: &Path,
        options: &PullOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.pull_with_progress(path, options, cancellation, Arc::new(|_| {}))
            .await
    }

    pub(super) async fn pull_with_progress_inner(
        &self,
        path: &Path,
        options: &PullOptions,
        cancellation: CancellationToken,
        progress: PullProgress,
    ) -> ApiResult<MutationResult> {
        let remotes = self
            .validate_remote_selection(path, options.remote.as_deref())
            .await?;
        if options.branch.is_some() && options.remote.is_none() {
            return Err(ApiError::new(
                ErrorCode::InvalidSettings,
                "Pull branch requires an explicit remote",
            ));
        }
        let mut args = download_args("pull");
        if options.skip_submodules {
            args.push("--no-recurse-submodules".into());
        }
        match options.mode {
            PullMode::Merge => {
                args.push("--no-rebase".into());
                args.push("--ff".into());
            }
            PullMode::FastForwardOnly => {
                args.push("--ff-only".into());
                args.push("--no-rebase".into());
            }
            PullMode::Rebase => args.push("--rebase".into()),
        }
        if options.prune {
            args.push("--prune".into());
        }
        // GitCat stashes around the pull itself, untracked files included;
        // Git's `--autostash` leaves those behind to block the checkout.
        args.push("--no-autostash".into());
        if let Some(remote) = &options.remote {
            args.push("--".into());
            args.push(remote.into());
            if let Some(branch) = &options.branch {
                self.validate_branch_name(branch).await?;
                args.push(branch.into());
            }
        }
        let run = self
            .network_options(&remotes, options.remote.as_deref(), false)
            .await;
        self.pull_with_stash(path, args, options.autostash, cancellation, run, progress)
            .await
    }

    pub(super) async fn push_inner(
        &self,
        path: &Path,
        options: &PushOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        if options.branch.is_some() && options.remote.is_none() {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "Pushing an explicit branch requires an explicit remote",
            ));
        }
        let remotes = self
            .validate_remote_selection(path, options.remote.as_deref())
            .await?;
        if options.set_upstream && (options.remote.is_none() || options.branch.is_none()) {
            return Err(ApiError::new(
                ErrorCode::InvalidSettings,
                "Setting upstream requires an explicit remote and branch",
            ));
        }
        // Setting the upstream is a change to the repository's own
        // configuration, so it runs even when the commits are already there.
        // A forced push is an order to make the remote match this branch, and
        // the local view of the remote is exactly what may be out of date, so
        // it is never answered from the tracking ref either.
        if !options.set_upstream
            && options.force == PushForce::None
            && let Some((local, tracking)) = self.push_destination_state(path, options).await?
            && local == tracking
        {
            let before_oid = self.head_oid(path).await?;
            let mut result = self.mutation_result(path, before_oid).await?;
            result.notice = Some("Everything up-to-date".to_owned());
            return Ok(result);
        }
        let mut args = os_args(&["push", "--porcelain", "--progress"]);
        if options.skip_submodules {
            args.push("--no-recurse-submodules".into());
        }
        match options.force {
            PushForce::None => {}
            // `--force-if-includes` is what makes the lease mean what the user
            // read: without it a background fetch that moved the tracking ref
            // satisfies the lease on work this branch never saw.
            PushForce::WithLease => {
                args.push("--force-with-lease".into());
                args.push("--force-if-includes".into());
            }
            PushForce::Force => args.push("--force".into()),
        }
        if options.set_upstream {
            args.push("--set-upstream".into());
        }
        if let Some(remote) = &options.remote {
            args.push("--".into());
            args.push(remote.into());
            if let Some(branch) = &options.branch {
                self.validate_branch_name(branch).await?;
                args.push(branch.into());
            }
        }
        let run = self
            .network_options(&remotes, options.remote.as_deref(), true)
            .await;
        self.mutate_network(path, args, cancellation, run).await
    }
}
