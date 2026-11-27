//! History pages, commit search, commit details and diffs, and the stash rows inside them.

use super::*;

impl GitCliBackend {
    pub(super) async fn refs_output(&self, path: &Path) -> ApiResult<GitCommandOutput> {
        let mut args = os_args(&["for-each-ref"]);
        args.push(format!("--format={REF_FORMAT}").into());
        args.extend(os_args(&["refs/heads", "refs/remotes", "refs/tags"]));
        self.read(Some(path), args).await
    }

    pub(super) async fn generation_and_refs(
        &self,
        path: &Path,
    ) -> ApiResult<(String, Vec<u8>, ParsedStatus)> {
        let (status, refs) = tokio::try_join!(self.status_output(path), self.refs_output(path))?;
        let parsed = parse_status(&status.stdout)?;
        let mut hasher = DefaultHasher::new();
        status.stdout.hash(&mut hasher);
        refs.stdout.hash(&mut hasher);
        Ok((format!("{:016x}", hasher.finish()), refs.stdout, parsed))
    }

    pub(super) async fn history_inputs(&self, path: &Path) -> ApiResult<HistoryInputs> {
        let (status, refs, head, stash_output) = tokio::try_join!(
            self.status_output(path),
            self.refs_output(path),
            self.read_allow_failure(
                Some(path),
                os_args(&["rev-parse", "--verify", "--end-of-options", "HEAD"]),
            ),
            self.stash_graph_output(path),
        )?;
        let parsed_status = parse_status(&status.stdout)?;
        let has_wip = !parsed_status.status.clean;
        let wip_head_oid = if has_wip {
            match parsed_status.head {
                HeadState::Branch { oid, .. } | HeadState::Detached { oid } => Some(oid),
                HeadState::Unborn { .. } => None,
            }
        } else {
            None
        };
        let refs = refs.stdout;
        let mut hasher = DefaultHasher::new();
        refs.hash(&mut hasher);
        head.status.code().hash(&mut hasher);
        head.stdout.hash(&mut hasher);
        has_wip.hash(&mut hasher);
        stash_output.hash(&mut hasher);
        Ok(HistoryInputs {
            generation: format!("{:016x}", hasher.finish()),
            refs_output: refs,
            layout: GraphLayoutContext { wip_head_oid },
            stashes: parse_stash_graph(&stash_output)?,
        })
    }

    pub(super) async fn changed_files_for_commit(
        &self,
        path: &Path,
        oid: &str,
        parents: &[String],
        parent_index: usize,
    ) -> ApiResult<Vec<ChangedFile>> {
        let (name_args, stat_args) = if parents.is_empty() {
            if parent_index != 0 {
                return Err(ApiError::new(
                    ErrorCode::InvalidRevision,
                    "Root commit has no selected parent",
                ));
            }
            (
                vec![
                    "diff-tree".into(),
                    "--root".into(),
                    "--no-commit-id".into(),
                    "-r".into(),
                    "-M".into(),
                    "--name-status".into(),
                    "-z".into(),
                    oid.into(),
                ],
                vec![
                    "diff-tree".into(),
                    "--root".into(),
                    "--no-commit-id".into(),
                    "-r".into(),
                    "-M".into(),
                    "--numstat".into(),
                    "-z".into(),
                    oid.into(),
                ],
            )
        } else {
            let parent = parents.get(parent_index).ok_or_else(|| {
                ApiError::new(
                    ErrorCode::InvalidRevision,
                    "Commit parent index is out of range",
                )
            })?;
            (
                vec![
                    "diff".into(),
                    "--no-ext-diff".into(),
                    "--no-textconv".into(),
                    "-M".into(),
                    "--name-status".into(),
                    "-z".into(),
                    parent.into(),
                    oid.into(),
                    "--".into(),
                ],
                vec![
                    "diff".into(),
                    "--no-ext-diff".into(),
                    "--no-textconv".into(),
                    "-M".into(),
                    "--numstat".into(),
                    "-z".into(),
                    parent.into(),
                    oid.into(),
                    "--".into(),
                ],
            )
        };
        let (names, stats) = tokio::try_join!(
            self.read(Some(path), name_args),
            self.read(Some(path), stat_args)
        )?;
        parse_changed_files(&names.stdout, &stats.stdout)
    }

    pub(super) async fn stash_graph_output(&self, path: &Path) -> ApiResult<Vec<u8>> {
        let mut args = os_args(&["stash", "list"]);
        args.push(format!("--format={STASH_GRAPH_FORMAT}").into());
        Ok(self.read(Some(path), args).await?.stdout)
    }

    pub(super) async fn stash_graph(&self, path: &Path) -> ApiResult<StashGraph> {
        parse_stash_graph(&self.stash_graph_output(path).await?)
    }

    pub(super) async fn history_revision(
        &self,
        path: &Path,
        scope: &HistoryScope,
    ) -> ApiResult<OsString> {
        match scope {
            HistoryScope::CurrentBranch => Ok("HEAD".into()),
            HistoryScope::AllRefs => Ok("--all".into()),
            HistoryScope::Ref(reference) => Ok(self.resolve_commit(path, reference).await?.into()),
        }
    }

    /// The revisions the all-refs graph walks. Only the namespaces the graph
    /// decorates are named: `--all` would also walk refs other tools park
    /// under their own prefixes, and one of those pointing at a pruned object
    /// fails the whole walk with "bad object". Stashes are added separately.
    pub(super) async fn all_refs_revisions(&self, path: &Path) -> ApiResult<Vec<OsString>> {
        let mut revisions = os_args(&["--branches", "--remotes", "--tags"]);
        if self.head_oid(path).await?.is_some() {
            revisions.push("HEAD".into());
        }
        Ok(revisions)
    }

    pub(super) async fn visible_history_revision(
        &self,
        path: &Path,
        scope: &HistoryScope,
        stashes: &StashGraph,
    ) -> ApiResult<OsString> {
        let revision = self.history_revision(path, scope).await?;
        let HistoryScope::Ref(_) = scope else {
            return Ok(revision);
        };
        let oid = revision.to_string_lossy();
        Ok(stash_for_oid(stashes, &oid)
            .map_or_else(|| revision.clone(), |(visible_oid, _)| visible_oid.into()))
    }

    pub(super) async fn search_inputs(
        &self,
        path: &Path,
        scope: &HistoryScope,
        stashes: &StashGraph,
    ) -> ApiResult<SearchInputs> {
        match scope {
            HistoryScope::AllRefs => Ok(SearchInputs {
                normal_revisions: self.all_refs_revisions(path).await?,
                virtual_stashes: stashes
                    .commits
                    .iter()
                    .map(|(oid, stash)| (oid.clone(), stash.clone()))
                    .collect(),
            }),
            HistoryScope::CurrentBranch => Ok(SearchInputs {
                normal_revisions: vec!["HEAD".into()],
                virtual_stashes: Vec::new(),
            }),
            HistoryScope::Ref(reference) => {
                let oid = self.resolve_commit(path, reference).await?;
                if let Some((visible_oid, stash)) = stash_for_oid(stashes, &oid) {
                    Ok(SearchInputs {
                        normal_revisions: stash
                            .base_oid
                            .iter()
                            .cloned()
                            .map(OsString::from)
                            .collect(),
                        virtual_stashes: vec![(visible_oid.clone(), stash.clone())],
                    })
                } else {
                    Ok(SearchInputs {
                        normal_revisions: vec![oid.into()],
                        virtual_stashes: Vec::new(),
                    })
                }
            }
        }
    }

    pub(super) async fn virtual_stash_search_hits(
        &self,
        path: &Path,
        needle: &str,
        candidates: Vec<(String, StashCommit)>,
    ) -> ApiResult<Vec<CommitSearchHit>> {
        let needle_lower = needle.to_lowercase();
        let mut matching = candidates
            .into_iter()
            .filter(|(_, stash)| stash.label.to_lowercase().contains(&needle_lower))
            .collect::<HashMap<_, _>>();
        if matching.is_empty() {
            return Ok(Vec::new());
        }

        let mut tips = matching.keys().cloned().collect::<Vec<_>>();
        tips.sort_unstable();
        let mut args = os_args(&[
            "log",
            "--no-walk=sorted",
            "--no-show-signature",
            "--encoding=UTF-8",
        ]);
        args.push(format!("--format={LOG_FORMAT}").into());
        args.extend(tips.into_iter().map(OsString::from));
        args.push("--".into());
        let output = self.read(Some(path), args).await?;

        Ok(parse_log(&output.stdout)?
            .into_iter()
            .filter_map(|commit| {
                matching.remove(&commit.oid).map(|stash| CommitSearchHit {
                    oid: commit.oid,
                    subject: stash.label,
                    body_excerpt: None,
                    matched_subject: true,
                    matched_body: false,
                })
            })
            .collect())
    }

    pub(super) async fn history_inner(
        &self,
        path: &Path,
        query: &HistoryQuery,
    ) -> ApiResult<HistoryPage> {
        let HistoryInputs {
            generation,
            refs_output,
            layout,
            stashes,
        } = self.history_inputs(path).await?;
        if let Some(cursor) = &query.cursor {
            if cursor.generation != generation {
                return Err(ApiError::new(
                    ErrorCode::StaleSnapshot,
                    "Repository changed while commit history was paged",
                ));
            }
        }
        if matches!(query.scope, HistoryScope::CurrentBranch)
            && self.head_oid(path).await?.is_none()
        {
            return Ok(HistoryPage {
                generation,
                commits: Vec::new(),
                next_cursor: None,
                has_more: false,
            });
        }
        let limit = query.limit.clamp(1, MAX_HISTORY_PAGE);
        let offset = query.cursor.as_ref().map_or(0, |cursor| cursor.offset);
        if offset > 10_000_000 {
            return Err(ApiError::new(
                ErrorCode::InvalidSettings,
                "History cursor offset is outside the supported range",
            ));
        }
        let mut args = os_args(&[
            "log",
            "--topo-order",
            "--date-order",
            "--no-show-signature",
            "--encoding=UTF-8",
        ]);
        args.push(format!("--format={LOG_FORMAT}").into());
        args.push(format!("--skip={offset}").into());
        args.push(format!("--max-count={}", limit + 1).into());
        if matches!(query.scope, HistoryScope::AllRefs) {
            args.extend(self.all_refs_revisions(path).await?);
            let mut stash_tips: Vec<&String> = stashes.commits.keys().collect();
            stash_tips.sort_unstable();
            args.extend(stash_tips.into_iter().map(OsString::from));
        } else {
            args.push(
                self.visible_history_revision(path, &query.scope, &stashes)
                    .await?,
            );
        }
        args.push("--".into());
        let output = self.read(Some(path), args).await?;
        let inputs_after = self.history_inputs(path).await?;
        if inputs_after.generation != generation {
            return Err(ApiError::new(
                ErrorCode::StaleSnapshot,
                "Repository changed while commit history was read",
            ));
        }
        let mut commits = parse_log(&output.stdout)?;
        let has_more = commits.len() > limit;
        commits.truncate(limit);
        let walked = commits.len();
        apply_stash_view(&mut commits, &stashes);

        let mut labels: HashMap<String, Vec<RefLabel>> = HashMap::new();
        for parsed_ref in parse_refs(&refs_output)? {
            if matches!(parsed_ref.label.kind, RefKind::RemoteBranch)
                && parsed_ref.symbolic_target.is_some()
            {
                continue;
            }
            labels
                .entry(parsed_ref.oid)
                .or_default()
                .push(parsed_ref.label);
        }
        for commit in &mut commits {
            commit.decorations = labels.remove(&commit.oid).unwrap_or_default();
        }
        let mut lanes = query
            .cursor
            .as_ref()
            .map(|cursor| cursor.lanes.clone())
            .unwrap_or_default();
        layout_commits_with_context(&mut commits, &mut lanes, &layout);
        let next_cursor = has_more.then(|| HistoryCursor {
            generation: generation.clone(),
            offset: offset + walked,
            lanes,
        });
        Ok(HistoryPage {
            generation,
            commits,
            next_cursor,
            has_more,
        })
    }

    pub(super) async fn search_commits_inner(
        &self,
        path: &Path,
        query: &CommitSearchQuery,
    ) -> ApiResult<CommitSearchResult> {
        let needle = query.query.trim();
        if needle.is_empty() {
            return Ok(CommitSearchResult {
                total: 0,
                truncated: false,
                hits: Vec::new(),
            });
        }
        if needle.chars().count() > 256 || needle.contains('\0') {
            return Err(ApiError::new(
                ErrorCode::InvalidSettings,
                "Commit search query must contain at most 256 characters",
            ));
        }
        if matches!(query.scope, HistoryScope::CurrentBranch)
            && self.head_oid(path).await?.is_none()
        {
            return Ok(CommitSearchResult {
                total: 0,
                truncated: false,
                hits: Vec::new(),
            });
        }
        let limit = query.limit.clamp(1, MAX_SEARCH_RESULTS);
        let stashes = self.stash_graph(path).await?;
        let SearchInputs {
            normal_revisions,
            virtual_stashes,
        } = self.search_inputs(path, &query.scope, &stashes).await?;
        let mut virtual_hits = self
            .virtual_stash_search_hits(path, needle, virtual_stashes)
            .await?;
        let virtual_total = virtual_hits.len();
        let normal_limit = limit.saturating_sub(virtual_total.min(limit));
        let grep = format!("--grep={needle}");
        let (normal_total, mut normal_hits) = if normal_revisions.is_empty() {
            (0, Vec::new())
        } else {
            let mut count_args = os_args(&[
                "rev-list",
                "--count",
                "--fixed-strings",
                "--regexp-ignore-case",
            ]);
            count_args.push(grep.clone().into());
            count_args.extend(normal_revisions.iter().cloned());
            count_args.push("--".into());
            let count_output = self.read(Some(path), count_args).await?;
            let total = count_output
                .stdout_lossy()
                .trim()
                .parse::<usize>()
                .map_err(|_| {
                    ApiError::new(
                        ErrorCode::GitCommandFailed,
                        "Git returned an invalid search count",
                    )
                })?;

            if normal_limit == 0 {
                (total, Vec::new())
            } else {
                // Fetch enough additional raw matches to replace any stash
                // plumbing reachable through an unusual extra ref.
                let raw_limit = normal_limit
                    .saturating_add(stashes.hidden.len())
                    .saturating_add(stashes.commits.len());
                let mut log_args = os_args(&[
                    "log",
                    "--topo-order",
                    "--date-order",
                    "--fixed-strings",
                    "--regexp-ignore-case",
                    "--encoding=UTF-8",
                ]);
                log_args.push(grep.into());
                log_args.push(format!("--format={LOG_FORMAT}").into());
                log_args.push(format!("--max-count={raw_limit}").into());
                log_args.extend(normal_revisions.iter().cloned());
                log_args.push("--".into());
                let output = self.read(Some(path), log_args).await?;
                let mut hits = parse_search_hits(&output.stdout, needle)?;
                let raw_hit_count = hits.len();
                hits.retain(|hit| {
                    !stashes.hidden.contains(&hit.oid) && !stashes.commits.contains_key(&hit.oid)
                });
                let visible_total = total.saturating_sub(raw_hit_count - hits.len());
                hits.truncate(normal_limit);
                (visible_total, hits)
            }
        };

        // Virtual stash labels are not stored in their index-parent commit
        // objects, so Git cannot place them inside a single `--grep` traversal.
        // Keep virtual rows first in commit-date order, then preserve Git's
        // original topo/date order for every ordinary commit hit.
        let total = virtual_total.saturating_add(normal_total);
        virtual_hits.truncate(limit);
        virtual_hits.append(&mut normal_hits);
        Ok(CommitSearchResult {
            total,
            truncated: total > virtual_hits.len(),
            hits: virtual_hits,
        })
    }

    pub(super) async fn commit_details_inner(
        &self,
        path: &Path,
        oid: &str,
        parent_index: usize,
    ) -> ApiResult<CommitDetails> {
        let oid = self.resolve_commit(path, oid).await?;
        let stashes = self.stash_graph(path).await?;
        let stash = stash_for_oid(&stashes, &oid)
            .map(|(visible_oid, stash)| (visible_oid.clone(), stash.clone()));
        let show_oid = stash
            .as_ref()
            .map_or_else(|| oid.as_str(), |(_, stash)| stash.stash_oid.as_str());
        let mut args = os_args(&["show", "-s", "--no-show-signature", "--encoding=UTF-8"]);
        args.push(format!("--format={DETAIL_FORMAT}").into());
        args.push(show_oid.into());
        args.push("--".into());
        let output = self.read(Some(path), args).await?;
        let mut details = parse_commit_details(&output.stdout)?.details;
        let (diff_oid, diff_parents) = if let Some((visible_oid, stash)) = stash {
            details.oid = visible_oid.clone();
            details.short_oid = self.short_oid(path, &visible_oid).await?;
            details.subject = stash.label;
            details.parent_oids = stash.base_oid.iter().cloned().collect();
            (stash.stash_oid, details.parent_oids.clone())
        } else {
            (oid.clone(), details.parent_oids.clone())
        };
        let files = self
            .changed_files_for_commit(path, &diff_oid, &diff_parents, parent_index)
            .await?;
        details.stats = DiffStats {
            files: files.len().try_into().unwrap_or(u32::MAX),
            additions: files.iter().filter_map(|file| file.additions).sum(),
            deletions: files.iter().filter_map(|file| file.deletions).sum(),
        };
        details.files = files;
        Ok(details)
    }

    pub(super) async fn diff_inner(
        &self,
        path: &Path,
        request: &DiffRequest,
    ) -> ApiResult<FileDiff> {
        validate_relative_path(&request.path)?;
        let old_path = request
            .old_path
            .as_deref()
            .filter(|old_path| *old_path != request.path);
        if let Some(old_path) = old_path {
            validate_relative_path(old_path)?;
        }
        if matches!(
            request.target,
            DiffTarget::Worktree | DiffTarget::HeadToWorktree
        ) && self.is_untracked_file(path, &request.path).await?
        {
            if let (Some(old_path), DiffTarget::Worktree) = (old_path, &request.target) {
                if let Some(diff) = self.worktree_rename_diff(path, request, old_path).await? {
                    return Ok(diff);
                }
            }
            return self.untracked_diff(path, request).await;
        }
        let mut args = match &request.target {
            DiffTarget::Worktree => os_args(&["diff"]),
            DiffTarget::Staged => os_args(&["diff", "--cached"]),
            DiffTarget::HeadToWorktree => {
                if let Some(head) = self.head_oid(path).await? {
                    vec!["diff".into(), head.into()]
                } else {
                    os_args(&["diff", "--cached"])
                }
            }
            DiffTarget::Commit { oid, parent_index } => {
                let oid = self.resolve_commit(path, oid).await?;
                let stashes = self.stash_graph(path).await?;
                let stash = stash_for_oid(&stashes, &oid).map(|(_, stash)| stash.clone());
                if let Some(stash) = stash {
                    if *parent_index != 0 {
                        return Err(ApiError::new(
                            ErrorCode::InvalidRevision,
                            "Commit parent index is out of range",
                        ));
                    }
                    if let Some(base_oid) = stash.base_oid {
                        vec!["diff".into(), base_oid.into(), stash.stash_oid.into()]
                    } else {
                        vec![
                            "show".into(),
                            "--format=".into(),
                            "--root".into(),
                            stash.stash_oid.into(),
                        ]
                    }
                } else {
                    let details = self.commit_details(path, &oid, *parent_index).await?;
                    if details.parent_oids.is_empty() {
                        vec![
                            "show".into(),
                            "--format=".into(),
                            "--root".into(),
                            oid.into(),
                        ]
                    } else {
                        vec![
                            "diff".into(),
                            details.parent_oids[*parent_index].clone().into(),
                            oid.into(),
                        ]
                    }
                }
            }
            DiffTarget::Between { base_oid, head_oid } => vec![
                "diff".into(),
                self.resolve_commit(path, base_oid).await?.into(),
                self.resolve_commit(path, head_oid).await?.into(),
            ],
        };
        // A rename is diffed against the path it came from. When Git no longer
        // pairs the two (the file changed past the similarity threshold since
        // the pane was opened), the new path is shown on its own instead.
        let mut rename_source = old_path;
        let selected = loop {
            let mut name_args = args.clone();
            if rename_source.is_some() {
                name_args.push("-M".into());
            }
            name_args.extend(os_args(&[
                "--name-status",
                "-z",
                "--no-ext-diff",
                "--no-textconv",
                "--",
            ]));
            if let Some(old_path) = rename_source {
                name_args.push(old_path.into());
            }
            name_args.push(request.path.as_str().into());
            let names = self.read(Some(path), name_args).await?;
            let selected = parse_name_status(&names.stdout)?;
            // An unmerged path is reported once per stage, so the same path
            // landing here twice is expected. Only a foreign path means the
            // request widened past the single file the pane asked for.
            if selected.iter().all(|file| file.new_path == request.path) {
                break selected;
            }
            if rename_source.take().is_none() {
                return Err(ApiError::new(
                    ErrorCode::InvalidRequest,
                    "Diff request must select exactly one changed file",
                ));
            }
        };
        // Git answers an unmerged worktree path with a combined diff the patch
        // parser cannot read. Ask for the stage-2 side instead, so the pane
        // shows the conflicted working copy against "ours".
        if matches!(request.target, DiffTarget::Worktree)
            && selected
                .iter()
                .any(|file| file.status == ChangeKind::Unmerged)
        {
            args.push("--ours".into());
        }
        args.extend(os_args(&[
            "--patch",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
        ]));
        args.push(format!("--unified={}", unified_context(request)).into());
        if request.ignore_whitespace {
            args.push("--ignore-all-space".into());
        }
        if rename_source.is_some() {
            args.push("-M".into());
        }
        args.push("--".into());
        if let Some(old_path) = rename_source {
            args.push(old_path.into());
        }
        args.push(request.path.as_str().into());
        let mut options = GitRunOptions::read_only(request.max_bytes.clamp(1, MAX_DIFF_BYTES));
        options.allow_stdout_truncation = true;
        options.timeout = Some(Duration::from_secs(60));
        let output = self
            .runner
            .run(Some(path), &args, None, CancellationToken::new(), options)
            .await?;
        parse_file_diff(&output.stdout, &request.path, output.stdout_truncated)
    }
}
