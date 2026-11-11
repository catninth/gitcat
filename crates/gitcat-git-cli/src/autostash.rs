//! GitCat's own autostash around a pull.
//!
//! `git pull --autostash` stashes tracked changes only. An untracked file that
//! the incoming commits start tracking then stops the checkout ("untracked
//! working tree files would be overwritten"), even when the file is
//! byte-identical to the incoming one -- typically a build artefact that was
//! produced locally and committed elsewhere. GitKraken gets through the same
//! state by stashing untracked files too and, on the way back, accepting a
//! file that already holds exactly what the stash would restore. This module
//! does the same with plain Git commands:
//!
//! 1. `stash push --include-untracked`, so the pull meets a clean worktree;
//! 2. the pull itself, unchanged;
//! 3. restore: an untracked file whose path now exists with identical content
//!    is removed first, so `stash apply` can put it back as the same bytes,
//!    and the stash is dropped once applied.
//!
//! An untracked file whose path now exists with *different* content is never
//! silently overwritten. Like GitKraken, the stash is applied anyway and the
//! path becomes a "both added" conflict -- stage 2 the pulled version, stage 3
//! the stashed one -- so it is resolved in the ordinary conflict view. The
//! stash stays in the list until the user drops it.
//! When the pull stops in a merge or rebase, the stash is handed to Git the way
//! `--autostash` itself would have stored it, so finishing or aborting that
//! operation brings the changes back without GitCat having to be running.

use std::{ffi::OsString, path::Path, time::Duration};

use gitcat_contracts::*;
use tokio_util::sync::CancellationToken;

use crate::{
    backend::GitCliBackend,
    limits::READ_OUTPUT_CAP,
    operation::{operation_progress_from_git_dir, operation_state_from_git_dir},
    runner::{GitCommandOutput, GitRunOptions, os_args},
};

const AUTOSTASH_MESSAGE: &str = "GitCat: autostash before pull";
const REBASE_POLL: Duration = Duration::from_millis(150);

impl GitCliBackend {
    /// Runs a pull, holding local changes aside around it when `autostash` is
    /// set, and reports each phase as it starts.
    pub(crate) async fn pull_with_stash(
        &self,
        path: &Path,
        args: Vec<OsString>,
        autostash: bool,
        cancellation: CancellationToken,
        run: GitRunOptions,
        progress: PullProgress,
    ) -> ApiResult<MutationResult> {
        let stash = if autostash {
            self.stash_for_pull(path, &progress).await?
        } else {
            None
        };

        progress(PullPhase::Pulling);
        let result = self
            .pull_reporting_rebase(path, args, cancellation, run, &progress)
            .await;
        let Some(stash) = stash else {
            return result;
        };

        // A pull that stopped in a merge or rebase keeps the stash for Git to
        // apply when that operation finishes or is aborted.
        let git_dir = self.git_dir(path).await?;
        let state = operation_state_from_git_dir(&git_dir);
        if matches!(
            state,
            RepositoryOperationState::Merge | RepositoryOperationState::Rebase
        ) && self.hand_stash_to_git(path, &git_dir, state, &stash).await
        {
            return result;
        }

        progress(PullPhase::Restoring);
        let restored = self
            .restore_pull_stash(path, &stash)
            .await
            .unwrap_or_else(|error| {
                Restore::Kept(format!(
                    "Your local changes are kept in the stash (\"{AUTOSTASH_MESSAGE}\"): {}",
                    error.message
                ))
            });
        let notice = match restored {
            Restore::Clean => None,
            Restore::Kept(notice) => Some(notice),
            Restore::Conflicted(notice) => {
                // The conflicts were made after the pull's own result was
                // read, so it is read again for the conflict list.
                let result = match result {
                    Ok(result) => self.mutation_result(path, result.before_oid).await,
                    Err(error) => Err(error),
                };
                return match result {
                    Ok(mut result) => {
                        result.needs_user_action = true;
                        result.notice = Some(notice);
                        Ok(result)
                    }
                    Err(error) => Err(append_notice(error, notice)),
                };
            }
        };
        match (result, notice) {
            (result, None) => result,
            (Ok(mut result), Some(notice)) => {
                result.needs_user_action = true;
                result.notice = Some(notice);
                Ok(result)
            }
            // The pull's own failure is the one the user asked about; a stash
            // that could not come back is added to it rather than hiding it.
            (Err(error), Some(notice)) => Err(append_notice(error, notice)),
        }
    }

    /// A command that writes to the repository, run with mutation settings
    /// rather than the read-only ones `read` uses.
    async fn write(&self, path: &Path, args: Vec<OsString>) -> ApiResult<GitCommandOutput> {
        let mut options = GitRunOptions::mutation(READ_OUTPUT_CAP);
        options.allow_failure = true;
        // `stash` runs its own pathspecs internally (`git clean -- :/` for the
        // untracked part). Under the runner's literal pathspecs `:/` names a
        // file rather than the whole tree, so the untracked files would be
        // stashed but never removed -- and the pull would stop on them again.
        if args.first().is_some_and(|command| command == "stash") {
            options
                .extra_env
                .push(("GIT_LITERAL_PATHSPECS".into(), "0".into()));
        }
        self.runner
            .run(Some(path), &args, None, CancellationToken::new(), options)
            .await
    }

    async fn write_checked_stdin(
        &self,
        path: &Path,
        args: Vec<OsString>,
        stdin: &[u8],
    ) -> ApiResult<()> {
        let mut options = GitRunOptions::mutation(READ_OUTPUT_CAP);
        options.allow_failure = true;
        let output = self
            .runner
            .run(
                Some(path),
                &args,
                Some(stdin),
                CancellationToken::new(),
                options,
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(self.runner.failure_error(&output))
        }
    }

    async fn write_checked(&self, path: &Path, args: Vec<OsString>) -> ApiResult<()> {
        let output = self.write(path, args).await?;
        if output.success() {
            Ok(())
        } else {
            Err(self.runner.failure_error(&output))
        }
    }

    /// Stashes tracked and untracked changes. `None` when there was nothing to
    /// stash, or when there is no commit yet to stash against -- Git's own
    /// `--autostash` is no better off on an unborn branch.
    async fn stash_for_pull(
        &self,
        path: &Path,
        progress: &PullProgress,
    ) -> ApiResult<Option<String>> {
        if self.head_oid(path).await?.is_none() {
            return Ok(None);
        }
        let status = self
            .read(
                Some(path),
                os_args(&[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                    "--ignore-submodules=dirty",
                ]),
            )
            .await?;
        if status.stdout.is_empty() {
            return Ok(None);
        }
        progress(PullPhase::Stashing);
        let before = self.stash_tip(path).await?;
        self.write_checked(
            path,
            os_args(&[
                "stash",
                "push",
                "--include-untracked",
                "--message",
                AUTOSTASH_MESSAGE,
            ]),
        )
        .await?;
        let after = self.stash_tip(path).await?;
        Ok(after.filter(|after| Some(after) != before.as_ref()))
    }

    async fn stash_tip(&self, path: &Path) -> ApiResult<Option<String>> {
        let output = self
            .read_allow_failure(
                Some(path),
                os_args(&["rev-parse", "--verify", "--quiet", "refs/stash"]),
            )
            .await?;
        Ok(output
            .success()
            .then(|| output.stdout_lossy().trim().to_owned()))
    }

    /// Runs the pull and, while a rebase is replaying commits, reports how far
    /// it has got. Git writes the counters to the rebase state directory, so a
    /// short poll is all it takes; a merge has no such steps to report.
    async fn pull_reporting_rebase(
        &self,
        path: &Path,
        args: Vec<OsString>,
        cancellation: CancellationToken,
        run: GitRunOptions,
        progress: &PullProgress,
    ) -> ApiResult<MutationResult> {
        let git_dir = self.git_dir(path).await.ok();
        let pull = self.mutate_network(path, args, cancellation, run);
        tokio::pin!(pull);
        let mut last = None;
        let mut ticker = tokio::time::interval(REBASE_POLL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                result = &mut pull => return result,
                _ = ticker.tick() => {
                    let Some(git_dir) = git_dir.as_deref() else { continue };
                    let Some(step) = operation_progress_from_git_dir(git_dir) else { continue };
                    let step = (step.current, step.total);
                    if last != Some(step) {
                        last = Some(step);
                        progress(PullPhase::Rebasing { current: step.0, total: step.1 });
                    }
                }
            }
        }
    }

    /// Stores the stash where Git's own `--autostash` would have, and takes it
    /// off the stash list so finishing the operation does not leave a copy.
    /// Answers `false` when Git's state directory is not where it is expected,
    /// in which case the stash is restored right away instead.
    async fn hand_stash_to_git(
        &self,
        path: &Path,
        git_dir: &Path,
        state: RepositoryOperationState,
        stash: &str,
    ) -> bool {
        let stored = match state {
            RepositoryOperationState::Rebase => {
                let directory = ["rebase-merge", "rebase-apply"]
                    .into_iter()
                    .map(|name| git_dir.join(name))
                    .find(|directory| directory.is_dir());
                match directory {
                    Some(directory) => {
                        std::fs::write(directory.join("autostash"), format!("{stash}\n")).is_ok()
                    }
                    None => false,
                }
            }
            RepositoryOperationState::Merge => self
                .write_checked(path, os_args(&["update-ref", "MERGE_AUTOSTASH", stash]))
                .await
                .is_ok(),
            _ => false,
        };
        if stored {
            let _ = self.drop_pull_stash(path, stash).await;
        }
        stored
    }

    /// Puts the stash back and says how that went.
    async fn restore_pull_stash(&self, path: &Path, stash: &str) -> ApiResult<Restore> {
        let untracked = self.stashed_untracked(path, stash).await?;
        let mut identical = Vec::new();
        let mut differing = Vec::new();
        for entry in untracked {
            if std::fs::symlink_metadata(path.join(&entry.path)).is_err() {
                continue;
            }
            if self.worktree_blob(path, &entry.path).await.as_deref() == Some(entry.blob.as_str()) {
                identical.push(entry);
                continue;
            }
            // A differing file the pull did not bring in has no pulled side
            // to set against the stashed one; the stash keeps everything.
            match self.head_blob(path, &entry.path).await {
                Some(pulled) => differing.push((entry, pulled)),
                None => {
                    return Ok(Restore::Kept(kept_notice(
                        "These untracked files now exist with different content",
                        &[entry.path],
                    )));
                }
            }
        }

        // Every colliding path is tracked in HEAD now, so removing it loses
        // nothing: the apply writes the stashed bytes back, and the pulled
        // ones stay in the commit.
        let removed = identical
            .iter()
            .map(|entry| entry.path.as_str())
            .chain(differing.iter().map(|(entry, _)| entry.path.as_str()))
            .collect::<Vec<_>>();
        for relative in &removed {
            let _ = std::fs::remove_file(path.join(relative));
        }
        let applied = self
            .write(path, os_args(&["stash", "apply", stash]))
            .await?;
        if !applied.success() {
            for relative in &removed {
                if std::fs::symlink_metadata(path.join(relative)).is_err() {
                    let mut args = os_args(&["checkout", "HEAD", "--"]);
                    args.push((*relative).into());
                    let _ = self.write(path, args).await;
                }
            }
            return Ok(Restore::Kept(format!(
                "Your local changes could not be reapplied cleanly and are kept in the stash \
                 (\"{AUTOSTASH_MESSAGE}\").\n{}",
                applied.stderr_lossy_redacted().trim()
            )));
        }
        if differing.is_empty() {
            self.drop_pull_stash(path, stash).await?;
            return Ok(Restore::Clean);
        }

        // Both versions go into the index as an add/add conflict, and the
        // worktree gets the usual markers drawn from those two stages.
        let mut index_info = String::new();
        for (entry, pulled) in &differing {
            index_info.push_str(&format!("0 {ZERO_OID}\t{}\n", entry.path));
            index_info.push_str(&format!("{} {pulled} 2\t{}\n", entry.mode, entry.path));
            index_info.push_str(&format!(
                "{} {} 3\t{}\n",
                entry.mode, entry.blob, entry.path
            ));
        }
        self.write_checked_stdin(
            path,
            os_args(&["update-index", "--index-info"]),
            index_info.as_bytes(),
        )
        .await?;
        let mut checkout = os_args(&["checkout", "--conflict=merge", "--"]);
        checkout.extend(
            differing
                .iter()
                .map(|(entry, _)| OsString::from(&entry.path)),
        );
        self.write_checked(path, checkout).await?;

        let paths = differing
            .into_iter()
            .map(|(entry, _)| entry.path)
            .collect::<Vec<_>>();
        Ok(Restore::Conflicted(kept_notice(
            "The pulled version and your untracked version conflict in",
            &paths,
        )))
    }

    /// The blob HEAD records at `relative`, if it records one there.
    async fn head_blob(&self, path: &Path, relative: &str) -> Option<String> {
        let output = self
            .read_allow_failure(
                Some(path),
                vec![
                    "rev-parse".into(),
                    "--verify".into(),
                    "--quiet".into(),
                    format!("HEAD:{relative}").into(),
                ],
            )
            .await
            .ok()?;
        output
            .success()
            .then(|| output.stdout_lossy().trim().to_owned())
    }

    /// The untracked part of a stash, as `(blob, path)` pairs. A stash made
    /// without untracked files has no third parent and yields nothing.
    async fn stashed_untracked(&self, path: &Path, stash: &str) -> ApiResult<Vec<StashedFile>> {
        let output = self
            .read_allow_failure(
                Some(path),
                vec![
                    "ls-tree".into(),
                    "-r".into(),
                    "-z".into(),
                    "--full-tree".into(),
                    format!("{stash}^3").into(),
                ],
            )
            .await?;
        if !output.success() {
            return Ok(Vec::new());
        }
        Ok(output
            .stdout
            .split(|byte| *byte == 0)
            .filter_map(|entry| {
                let entry = std::str::from_utf8(entry).ok()?;
                let (meta, relative) = entry.split_once('\t')?;
                let mut fields = meta.split_whitespace();
                let mode = fields.next()?.to_owned();
                (fields.next()? == "blob").then_some(())?;
                Some(StashedFile {
                    mode,
                    blob: fields.next()?.to_owned(),
                    path: relative.to_owned(),
                })
            })
            .collect())
    }

    /// The blob a worktree file would be stored as, with the repository's own
    /// filters applied, so a line-ending conversion does not read as a change.
    async fn worktree_blob(&self, path: &Path, relative: &str) -> Option<String> {
        let mut args = os_args(&["hash-object", "--"]);
        args.push(relative.into());
        let output = self.read_allow_failure(Some(path), args).await.ok()?;
        output
            .success()
            .then(|| output.stdout_lossy().trim().to_owned())
    }

    async fn drop_pull_stash(&self, path: &Path, stash: &str) -> ApiResult<()> {
        let list = self
            .read(Some(path), os_args(&["stash", "list", "--format=%H"]))
            .await?;
        let Some(index) = list
            .stdout_lossy()
            .lines()
            .position(|line| line.trim() == stash)
        else {
            return Ok(());
        };
        self.write_checked(
            path,
            vec![
                "stash".into(),
                "drop".into(),
                format!("stash@{{{index}}}").into(),
            ],
        )
        .await
    }
}

const ZERO_OID: &str = "0000000000000000000000000000000000000000";

/// How putting the stash back went.
enum Restore {
    /// Everything is back and the stash is dropped.
    Clean,
    /// Back, with conflicts to resolve; the stash stays as a copy.
    Conflicted(String),
    /// Nothing was applied; the stash holds the changes.
    Kept(String),
}

struct StashedFile {
    mode: String,
    blob: String,
    path: String,
}

fn append_notice(error: ApiError, notice: String) -> ApiError {
    let details = match error.details.clone() {
        Some(details) => format!("{details}\n\n{notice}"),
        None => notice,
    };
    error.with_details(details)
}

fn kept_notice(reason: &str, paths: &[String]) -> String {
    const SHOWN: usize = 5;
    let mut listed = paths
        .iter()
        .take(SHOWN)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if paths.len() > SHOWN {
        listed.push_str(&format!(" and {} more", paths.len() - SHOWN));
    }
    format!(
        "{reason}: {listed}. Your local changes are kept in the stash (\"{AUTOSTASH_MESSAGE}\")."
    )
}

#[cfg(test)]
#[path = "autostash/tests.rs"]
mod tests;
