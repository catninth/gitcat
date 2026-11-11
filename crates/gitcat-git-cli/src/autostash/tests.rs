use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use gitcat_contracts::*;
use gitcat_core::GitBackend;
use tempfile::{TempDir, tempdir};
use tokio_util::sync::CancellationToken;

use crate::backend::GitCliBackend;

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("run fixture git command");
    assert!(
        output.status.success(),
        "fixture git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn configure(path: &Path) {
    git(path, &["config", "user.name", "GitCat Test"]);
    git(path, &["config", "user.email", "gitcat@example.test"]);
    git(path, &["config", "commit.gpgSign", "false"]);
    git(path, &["config", "core.autocrlf", "false"]);
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .expect("read fixture file")
        .replace("\r\n", "\n")
}

/// A local branch one commit ahead and one behind its remote, where the remote
/// commit starts tracking `plugins/icons.jar`, plus a dirty tracked edit -- the
/// state that made `git pull --rebase --autostash` stop at the checkout.
struct Diverged {
    _temp: TempDir,
    local: PathBuf,
    updater: PathBuf,
}

fn diverged(remote_icons: &str) -> Diverged {
    let temp = tempdir().expect("temp directory");
    let bare = temp.path().join("remote.git");
    git(
        temp.path(),
        &[
            "init",
            "--quiet",
            "--bare",
            "-b",
            "main",
            bare.to_str().unwrap(),
        ],
    );
    let updater = temp.path().join("updater");
    git(
        temp.path(),
        &["clone", "--quiet", bare.to_str().unwrap(), "updater"],
    );
    configure(&updater);
    fs::write(updater.join("tracked.txt"), "base\n").unwrap();
    git(&updater, &["add", "."]);
    git(&updater, &["commit", "--quiet", "-m", "base"]);
    git(&updater, &["push", "--quiet", "origin", "main"]);

    let local = temp.path().join("local");
    git(
        temp.path(),
        &["clone", "--quiet", bare.to_str().unwrap(), "local"],
    );
    configure(&local);

    fs::create_dir_all(updater.join("plugins")).unwrap();
    fs::write(updater.join("plugins/icons.jar"), remote_icons).unwrap();
    git(&updater, &["add", "."]);
    git(&updater, &["commit", "--quiet", "-m", "add icons.jar"]);
    git(&updater, &["push", "--quiet", "origin", "main"]);

    fs::write(local.join("tracked.txt"), "base\nlocal\n").unwrap();
    git(&local, &["commit", "--quiet", "-am", "local commit"]);
    fs::create_dir_all(local.join("plugins")).unwrap();
    fs::write(local.join("plugins/icons.jar"), "icons\n").unwrap();
    fs::write(local.join("tracked.txt"), "base\nlocal\ndirty\n").unwrap();
    Diverged {
        _temp: temp,
        local,
        updater,
    }
}

fn options(mode: PullMode) -> PullOptions {
    PullOptions {
        skip_submodules: false,
        remote: None,
        branch: None,
        mode,
        prune: false,
        autostash: true,
    }
}

async fn pull(path: &Path, mode: PullMode) -> (ApiResult<MutationResult>, Vec<PullPhase>) {
    let phases = Arc::new(Mutex::new(Vec::new()));
    let sink = phases.clone();
    let result = GitCliBackend::default()
        .pull_with_progress(
            path,
            &options(mode),
            CancellationToken::new(),
            Arc::new(move |phase| sink.lock().unwrap().push(phase)),
        )
        .await;
    let phases = phases.lock().unwrap().clone();
    (result, phases)
}

fn stash_count(path: &Path) -> usize {
    git(path, &["stash", "list"]).lines().count()
}

#[tokio::test]
async fn rebase_pull_accepts_an_untracked_file_identical_to_the_incoming_one() {
    let fixture = diverged("icons\n");
    let (result, phases) = pull(&fixture.local, PullMode::Rebase).await;
    let result = result.expect("pull succeeds like GitKraken's");
    assert!(!result.needs_user_action);
    assert_eq!(result.notice, None);

    assert_eq!(
        git(&fixture.local, &["rev-parse", "HEAD~1"]),
        git(&fixture.updater, &["rev-parse", "HEAD"]),
        "the local commit is replayed on top of the remote one"
    );
    assert_eq!(
        read(&fixture.local.join("tracked.txt")),
        "base\nlocal\ndirty\n"
    );
    assert_eq!(read(&fixture.local.join("plugins/icons.jar")), "icons\n");
    assert_eq!(
        git(&fixture.local, &["status", "--porcelain"]),
        "M tracked.txt",
        "only the dirty edit remains; the jar is tracked and unchanged"
    );
    assert_eq!(stash_count(&fixture.local), 0, "the autostash is dropped");

    assert_eq!(phases.first(), Some(&PullPhase::Stashing));
    assert!(phases.contains(&PullPhase::Pulling));
    assert_eq!(phases.last(), Some(&PullPhase::Restoring));
}

#[tokio::test]
async fn merge_pull_accepts_an_untracked_file_identical_to_the_incoming_one() {
    let fixture = diverged("icons\n");
    let (result, _) = pull(&fixture.local, PullMode::Merge).await;
    result.expect("merge pull succeeds");
    assert_eq!(
        git(&fixture.local, &["rev-list", "--count", "--merges", "HEAD"]),
        "1"
    );
    assert_eq!(
        read(&fixture.local.join("tracked.txt")),
        "base\nlocal\ndirty\n"
    );
    assert_eq!(stash_count(&fixture.local), 0);
}

#[tokio::test]
async fn a_differing_untracked_file_is_never_overwritten_and_stays_stashed() {
    let fixture = diverged("remote icons\n");
    let (result, _) = pull(&fixture.local, PullMode::Rebase).await;
    let result = result.expect("the pull itself succeeds");
    assert!(result.needs_user_action);
    let notice = result.notice.expect("a notice names the path");
    assert!(notice.contains("plugins/icons.jar"), "{notice}");

    assert_eq!(
        read(&fixture.local.join("plugins/icons.jar")),
        "remote icons\n"
    );
    assert_eq!(stash_count(&fixture.local), 1, "the user's version is kept");
    assert_eq!(
        git(&fixture.local, &["show", "stash@{0}^3:plugins/icons.jar"]),
        "icons"
    );
}

#[tokio::test]
async fn a_clean_worktree_pulls_without_stashing() {
    let fixture = diverged("icons\n");
    fs::remove_dir_all(fixture.local.join("plugins")).unwrap();
    git(&fixture.local, &["checkout", "--", "tracked.txt"]);
    let (result, phases) = pull(&fixture.local, PullMode::Rebase).await;
    result.expect("clean pull");
    assert_eq!(phases.first(), Some(&PullPhase::Pulling));
    assert!(!phases.contains(&PullPhase::Stashing));
    assert!(!phases.contains(&PullPhase::Restoring));
}

#[tokio::test]
async fn a_rebase_stopped_on_conflicts_hands_the_stash_to_git() {
    let fixture = diverged("icons\n");
    // The remote edits the same line the local commit does.
    fs::write(fixture.updater.join("tracked.txt"), "base\nremote\n").unwrap();
    git(
        &fixture.updater,
        &["commit", "--quiet", "-am", "remote edit"],
    );
    git(&fixture.updater, &["push", "--quiet", "origin", "main"]);

    let (result, phases) = pull(&fixture.local, PullMode::Rebase).await;
    let result = result.expect("conflicts are an outcome, not a failure");
    assert!(result.needs_user_action);
    assert!(!phases.contains(&PullPhase::Restoring));
    let git_dir = PathBuf::from(git(&fixture.local, &["rev-parse", "--absolute-git-dir"]));
    assert!(git_dir.join("rebase-merge/autostash").is_file());
    assert_eq!(stash_count(&fixture.local), 0, "Git owns the stash now");

    // Aborting brings everything back exactly as Git's own autostash would.
    git(&fixture.local, &["rebase", "--abort"]);
    assert_eq!(
        read(&fixture.local.join("tracked.txt")),
        "base\nlocal\ndirty\n"
    );
    assert_eq!(read(&fixture.local.join("plugins/icons.jar")), "icons\n");
    assert_eq!(stash_count(&fixture.local), 0);
}
