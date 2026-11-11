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
//! overwritten: the stash stays, and the result says which paths need a look.
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
        let notice = self
            .restore_pull_stash(path, &stash)
            .await
            .unwrap_or_else(|error| {
                Some(format!(
                    "Your local changes are kept in the stash (\"{AUTOSTASH_MESSAGE}\"): {}",
                    error.message
                ))
            });
        match (result, notice) {
            (result, None) => result,
            (Ok(mut result), Some(notice)) => {
                result.needs_user_action = true;
                result.notice = Some(notice);
                Ok(result)
            }
            // The pull's own failure is the one the user asked about; a stash
            // that could not come back is added to it rather than hiding it.
            (Err(error), Some(notice)) => {
                let details = match error.details.clone() {
                    Some(details) => format!("{details}\n\n{notice}"),
                    None => notice,
                };
                Err(error.with_details(details))
            }
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

    /// Puts the stash back. `Ok(Some(notice))` when it had to stay in the
    /// stash list, saying why.
    async fn restore_pull_stash(&self, path: &Path, stash: &str) -> ApiResult<Option<String>> {
        let untracked = self.stashed_untracked(path, stash).await?;
        let mut identical = Vec::new();
        let mut differing = Vec::new();
        for (blob, relative) in untracked {
            let file = path.join(&relative);
            if std::fs::symlink_metadata(&file).is_err() {
                continue;
            }
            if self.worktree_blob(path, &relative).await.as_deref() == Some(blob.as_str()) {
                identical.push(relative);
            } else {
                differing.push(relative);
            }
        }
        if !differing.is_empty() {
            return Ok(Some(kept_notice(
                "These untracked files now exist with different content",
                &differing,
            )));
        }

        // The pulled file holds exactly what the stash would restore, so it is
        // removed and put back by the apply as the same bytes.
        for relative in &identical {
            let _ = std::fs::remove_file(path.join(relative));
        }
        let applied = self
            .write(path, os_args(&["stash", "apply", stash]))
            .await?;
        if !applied.success() {
            // Nothing that was removed may stay missing: the commit has it.
            for relative in &identical {
                if std::fs::symlink_metadata(path.join(relative)).is_err() {
                    let mut args = os_args(&["checkout", "HEAD", "--"]);
                    args.push(relative.into());
                    let _ = self.write(path, args).await;
                }
            }
            return Ok(Some(format!(
                "Your local changes could not be reapplied cleanly and are kept in the stash \
                 (\"{AUTOSTASH_MESSAGE}\").\n{}",
                applied.stderr_lossy_redacted().trim()
            )));
        }
        self.drop_pull_stash(path, stash).await?;
        Ok(None)
    }

    /// The untracked part of a stash, as `(blob, path)` pairs. A stash made
    /// without untracked files has no third parent and yields nothing.
    async fn stashed_untracked(
        &self,
        path: &Path,
        stash: &str,
    ) -> ApiResult<Vec<(String, String)>> {
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
                let _mode = fields.next()?;
                (fields.next()? == "blob").then_some(())?;
                Some((fields.next()?.to_owned(), relative.to_owned()))
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
