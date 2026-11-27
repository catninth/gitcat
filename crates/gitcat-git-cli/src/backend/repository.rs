//! Probing Git, opening, initializing and cloning repositories, and the snapshot of one.

use super::*;

impl GitCliBackend {
    pub(super) async fn inspect_repository(&self, path: &Path) -> ApiResult<RepositoryInfo> {
        let selected = dunce::canonicalize(path).map_err(|error| {
            ApiError::new(
                ErrorCode::InvalidPath,
                "Repository path could not be resolved",
            )
            .with_details(format!("{}: {error}", path.display()))
        })?;
        if !selected.is_dir() {
            return Err(ApiError::new(
                ErrorCode::InvalidPath,
                "Repository path must be a directory",
            ));
        }

        let bare = self
            .read(
                Some(&selected),
                os_args(&["rev-parse", "--is-bare-repository"]),
            )
            .await?
            .stdout_lossy()
            .trim()
            == "true";
        if bare {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Bare repositories are not supported by the desktop worktree API",
            ));
        }
        let root = if bare {
            selected.clone()
        } else {
            let output = self
                .read(Some(&selected), os_args(&["rev-parse", "--show-toplevel"]))
                .await?;
            dunce::canonicalize(output.stdout_lossy().trim()).map_err(|error| {
                ApiError::new(
                    ErrorCode::InvalidRepository,
                    "Repository root could not be resolved",
                )
                .with_details(error.to_string())
            })?
        };

        let git_dir_output = self
            .read(
                Some(&selected),
                os_args(&["rev-parse", "--absolute-git-dir"]),
            )
            .await?;
        let git_dir = canonical_or_absolute(&selected, git_dir_output.stdout_lossy().trim())?;
        let common_output = self
            .read(
                Some(&selected),
                os_args(&["rev-parse", "--path-format=absolute", "--git-common-dir"]),
            )
            .await?;
        let common_dir = canonical_or_absolute(&selected, common_output.stdout_lossy().trim())?;

        let format_output = self
            .read_allow_failure(
                Some(&selected),
                os_args(&["rev-parse", "--show-object-format"]),
            )
            .await?;
        let object_format = if format_output.success() {
            match format_output.stdout_lossy().trim() {
                "sha1" => ObjectFormat::Sha1,
                "sha256" => ObjectFormat::Sha256,
                _ => ObjectFormat::Unknown,
            }
        } else {
            ObjectFormat::Sha1
        };
        let name = root
            .file_name()
            .unwrap_or(root.as_os_str())
            .to_string_lossy()
            .into_owned();
        Ok(RepositoryInfo {
            root: root.to_string_lossy().into_owned(),
            git_dir: git_dir.to_string_lossy().into_owned(),
            common_dir: common_dir.to_string_lossy().into_owned(),
            name,
            is_bare: bare,
            object_format,
        })
    }

    pub(super) async fn probe_inner(&self) -> ApiResult<GitVersion> {
        let output = self.read(None, os_args(&["version"])).await?;
        let (major, minor, patch, raw) = parse_git_version(&output.stdout)?;
        if major < 2 || (major == 2 && minor < 31) {
            return Err(ApiError::new(
                ErrorCode::UnsupportedGitVersion,
                "Git 2.31 or newer is required",
            )
            .with_details(raw));
        }
        Ok(GitVersion {
            major,
            minor,
            patch,
            raw,
        })
    }

    pub(super) async fn init_repository_inner(
        &self,
        path: &Path,
        default_branch: &str,
    ) -> ApiResult<RepositoryInfo> {
        self.validate_branch_name(default_branch).await?;
        let args = vec![
            "init".into(),
            format!("--initial-branch={default_branch}").into(),
            "--".into(),
            path.as_os_str().to_owned(),
        ];
        self.runner
            .run(
                None,
                &args,
                None,
                CancellationToken::new(),
                GitRunOptions::mutation(READ_OUTPUT_CAP),
            )
            .await?;
        self.inspect_repository(path).await
    }

    pub(super) async fn clone_repository_inner(
        &self,
        options: &CloneOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<RepositoryInfo> {
        validate_remote_url(&options.url)?;
        let destination = PathBuf::from(&options.destination);
        if destination.as_os_str().is_empty() {
            return Err(ApiError::new(
                ErrorCode::InvalidPath,
                "Clone destination is empty",
            ));
        }
        let mut args = os_args(&["clone", "--progress"]);
        if let Some(branch) = &options.branch {
            self.validate_branch_name(branch).await?;
            args.push("--branch".into());
            args.push(branch.into());
        }
        if let Some(depth) = options.depth {
            if depth == 0 {
                return Err(ApiError::new(
                    ErrorCode::InvalidSettings,
                    "Clone depth must be greater than zero",
                ));
            }
            args.push(format!("--depth={depth}").into());
        }
        if options.filter_blob_none {
            args.push("--filter=blob:none".into());
        }
        let sparse = options
            .sparse_paths
            .as_deref()
            .map(sparse_patterns)
            .transpose()?;
        if sparse.is_some() {
            args.push("--sparse".into());
        }
        args.push("--".into());
        args.push(options.url.as_str().into());
        args.push(destination.as_os_str().to_owned());
        let run = self.clone_options(&options.url).await;
        self.runner
            .run(None, &args, None, cancellation.clone(), run)
            .await?;
        // The clone itself only sets the sparse root up; which directories
        // join it is a second command, and the patterns go in on stdin so a
        // path that looks like an option cannot become one.
        if let Some(patterns) = sparse.filter(|patterns| !patterns.is_empty()) {
            self.runner
                .run(
                    Some(&destination),
                    &os_args(&["sparse-checkout", "set", "--stdin"]),
                    Some(patterns.join("\n").as_bytes()),
                    cancellation,
                    GitRunOptions::mutation(READ_OUTPUT_CAP),
                )
                .await?;
        }
        self.inspect_repository(&destination).await
    }

    pub(super) async fn snapshot_inner(&self, path: &Path) -> ApiResult<RepositorySnapshot> {
        let (generation, refs_output, mut parsed_status) = self.generation_and_refs(path).await?;
        parsed_status.status.stash_count = self
            .stash_graph(path)
            .await?
            .commits
            .len()
            .try_into()
            .unwrap_or(u32::MAX);
        self.apply_worktree_renames(path, &mut parsed_status.status)
            .await;
        self.apply_line_stats(path, &mut parsed_status.status)
            .await?;
        let refs = parse_refs(&refs_output)?;
        let mut local_branches = Vec::new();
        let mut remote_branches = Vec::new();
        let mut symbolic_remote_targets = Vec::new();
        let mut tags = Vec::new();
        for parsed_ref in refs {
            match parsed_ref.label.kind {
                RefKind::LocalBranch => {
                    if let Some(branch) = parsed_ref.branch {
                        local_branches.push(branch);
                    }
                }
                RefKind::RemoteBranch if parsed_ref.symbolic_target.is_none() => {
                    if let Some(branch) = parsed_ref.branch {
                        remote_branches.push(branch);
                    }
                }
                RefKind::Tag => tags.push(parsed_ref.label),
                RefKind::RemoteBranch => {
                    if let Some(target) = parsed_ref.symbolic_target {
                        symbolic_remote_targets.push((parsed_ref.label.full_name, target));
                    }
                }
            }
        }
        local_branches.sort_by(|left, right| left.name.cmp(&right.name));
        remote_branches.sort_by(|left, right| left.name.cmp(&right.name));
        tags.sort_by(|left, right| left.name.cmp(&right.name));
        symbolic_remote_targets.sort_by(|left, right| left.0.cmp(&right.0));
        let remote_exists = |name: &str| remote_branches.iter().any(|branch| branch.name == name);
        let default_conflict_target = local_branches
            .iter()
            .find(|branch| branch.is_head)
            .and_then(|branch| branch.upstream.clone())
            .filter(|upstream| remote_exists(upstream))
            .or_else(|| {
                let resolve_target = |target: &str| {
                    target
                        .strip_prefix("refs/remotes/")
                        .filter(|target| remote_exists(target))
                        .map(str::to_owned)
                };
                let origin_default = symbolic_remote_targets
                    .iter()
                    .find(|(symbolic, _)| symbolic == "refs/remotes/origin/HEAD")
                    .and_then(|(_, target)| resolve_target(target));
                origin_default.or_else(|| {
                    symbolic_remote_targets
                        .iter()
                        .find_map(|(_, target)| resolve_target(target))
                })
            });

        let info = self.inspect_repository(path).await?;
        let shallow = self
            .read_allow_failure(
                Some(path),
                os_args(&["rev-parse", "--is-shallow-repository"]),
            )
            .await?
            .stdout_lossy()
            .trim()
            == "true";
        let partial_clone = self
            .read_allow_failure(
                Some(path),
                os_args(&["config", "--get-regexp", r"^remote\..*\.promisor$"]),
            )
            .await?
            .success();
        let sparse_checkout = self
            .read_allow_failure(
                Some(path),
                os_args(&["config", "--bool", "core.sparseCheckout"]),
            )
            .await?
            .stdout_lossy()
            .trim()
            == "true";
        let operation = self.operation_status(path).await?;
        let operation_state = operation.state;
        let name_side = |value: String| {
            let branch = local_branches
                .iter()
                .chain(remote_branches.iter())
                .find(|branch| branch.oid == value);
            match branch {
                Some(branch) => branch.name.clone(),
                None => short_operation_oid(&value),
            }
        };
        let operation_source = operation.source.map(|source| OperationSource {
            incoming: name_side(source.incoming),
            onto: source.onto.map(name_side),
            incoming_oid: source.incoming_oid,
        });
        let remotes = self.remotes(path).await?;
        Ok(RepositorySnapshot {
            generation,
            head: parsed_status.head,
            operation_state,
            operation_progress: operation.progress,
            operation_source,
            operation_message: operation.message,
            operation_autostash: operation.autostash,
            status: parsed_status.status,
            local_branches,
            remote_branches,
            default_conflict_target,
            tags,
            remotes,
            capabilities: RepositoryCapabilities {
                shallow,
                partial_clone,
                sparse_checkout,
                worktree: info.git_dir != info.common_dir,
            },
        })
    }
}
