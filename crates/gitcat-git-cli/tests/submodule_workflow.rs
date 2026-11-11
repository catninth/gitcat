use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use gitcat_contracts::{ErrorCode, PullMode, PullOptions, RepositoryId, SubmoduleSyncAction};
use gitcat_core::CoreApi;
use gitcat_git_cli::GitCliBackend;
use tempfile::{TempDir, tempdir};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_ALLOW_PROTOCOL", "file:git:http:https:ssh")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("run git fixture command");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn init(root: &Path) {
    fs::create_dir_all(root).unwrap();
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.name", "Submodule Tests"]);
    git(root, &["config", "user.email", "tests@example.invalid"]);
    git(root, &["config", "commit.gpgSign", "false"]);
    git(root, &["config", "core.autocrlf", "false"]);
    commit(root, "initial");
}

fn commit(root: &Path, message: &str) -> String {
    fs::write(root.join("content.txt"), message).unwrap();
    git(root, &["add", "--", "content.txt"]);
    git(root, &["commit", "-m", message]);
    git(root, &["rev-parse", "HEAD"])
}

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    source: PathBuf,
    remote: PathBuf,
    api: CoreApi,
    id: RepositoryId,
}

impl Fixture {
    async fn new() -> Self {
        let temp = tempdir().unwrap();
        let root = temp.path().join("parent with spaces");
        let source = temp.path().join("source");
        let remote = temp.path().join("remote.git");
        init(&root);
        init(&source);
        git(
            temp.path(),
            &[
                "clone",
                "--bare",
                source.to_str().unwrap(),
                remote.to_str().unwrap(),
            ],
        );
        git(
            &source,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&source, &["push", "--set-upstream", "origin", "main"]);
        let api = CoreApi::new(Arc::new(GitCliBackend::default()));
        let (id, _) = api.open(&root).await.unwrap();
        Self {
            _temp: temp,
            root,
            source,
            remote,
            api,
            id,
        }
    }

    async fn add(&self) -> PathBuf {
        self.api
            .add_submodule(
                &self.id,
                self.remote.to_str().unwrap(),
                "packages/child with spaces",
            )
            .await
            .unwrap();
        git(&self.root, &["commit", "-m", "add child"]);
        self.root.join("packages/child with spaces")
    }
}

const CHILD: &str = "packages/child with spaces";

#[tokio::test]
async fn lists_commits_edits_url_and_removes_without_deleting_history() {
    let fixture = Fixture::new().await;
    assert!(
        fixture
            .api
            .list_submodules(&fixture.id)
            .await
            .unwrap()
            .is_empty()
    );
    let child = fixture.add().await;
    let modules = fixture.api.list_submodules(&fixture.id).await.unwrap();
    assert_eq!(modules.len(), 1);
    let module = &modules[0];
    assert_eq!(module.path, CHILD);
    assert_eq!(module.name, CHILD);
    assert!(module.initialized && module.head_matches_recorded && !module.dirty);
    assert_eq!(module.head_summary.as_deref(), Some("initial"));
    assert!(module.head_committed_at.unwrap() > 0);
    assert_eq!(module.branch.as_deref(), Some("main"));
    let alternate = fixture._temp.path().join("alternate.git");
    git(
        fixture._temp.path(),
        &[
            "clone",
            "--bare",
            fixture.remote.to_str().unwrap(),
            alternate.to_str().unwrap(),
        ],
    );
    fixture
        .api
        .set_submodule_url(&fixture.id, CHILD, alternate.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(
        fixture.api.list_submodules(&fixture.id).await.unwrap()[0].url,
        alternate.to_string_lossy()
    );
    assert_eq!(
        git(&child, &["remote", "get-url", "origin"]).replace('\\', "/"),
        alternate.to_string_lossy().replace('\\', "/")
    );
    git(&fixture.root, &["add", ".gitmodules"]);
    git(&fixture.root, &["commit", "-m", "edit child URL"]);
    let git_dir = git(&child, &["rev-parse", "--absolute-git-dir"]);
    assert_eq!(
        fixture
            .api
            .remove_submodule(&fixture.id, CHILD, false)
            .await
            .unwrap_err()
            .code,
        ErrorCode::ProtectedOperation
    );
    fixture
        .api
        .remove_submodule(&fixture.id, CHILD, true)
        .await
        .unwrap();
    assert!(
        fixture
            .api
            .list_submodules(&fixture.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(!child.exists());
    assert!(
        Path::new(&git_dir).exists(),
        "submodule object database remains recoverable"
    );
}

#[tokio::test]
async fn pulls_detached_checkout_without_staging_parent_pointer() {
    let fixture = Fixture::new().await;
    let child = fixture.add().await;
    git(&child, &["switch", "--detach"]);
    let recorded = git(&child, &["rev-parse", "HEAD"]);
    let latest = commit(&fixture.source, "remote advance");
    git(&fixture.source, &["push"]);
    fixture
        .api
        .update_submodule(&fixture.id, CHILD)
        .await
        .unwrap();
    let module = fixture
        .api
        .list_submodules(&fixture.id)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(module.head_oid.as_deref(), Some(latest.as_str()));
    assert_eq!(module.recorded_oid.as_deref(), Some(recorded.as_str()));
    assert!(module.branch.is_none() && !module.head_matches_recorded && !module.dirty);
    assert!(git(&fixture.root, &["diff", "--cached", "--name-only"]).is_empty());
    assert_eq!(
        fixture
            .api
            .sync_submodule(&fixture.id, CHILD, SubmoduleSyncAction::Push)
            .await
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedOperation
    );
}

#[tokio::test]
async fn initializes_recorded_commit_before_a_separate_remote_update() {
    let fixture = Fixture::new().await;
    fixture.add().await;
    git(&fixture.root, &["submodule", "deinit", "--", CHILD]);
    let module = fixture
        .api
        .list_submodules(&fixture.id)
        .await
        .unwrap()
        .remove(0);
    assert!(!module.initialized && module.head_oid.is_none());
    let recorded = module.recorded_oid;
    let latest = commit(&fixture.source, "remote after deinit");
    git(&fixture.source, &["push"]);
    fixture
        .api
        .update_submodule(&fixture.id, CHILD)
        .await
        .unwrap();
    assert_eq!(
        fixture.api.list_submodules(&fixture.id).await.unwrap()[0].head_oid,
        recorded
    );
    fixture
        .api
        .update_submodule(&fixture.id, CHILD)
        .await
        .unwrap();
    assert_eq!(
        fixture.api.list_submodules(&fixture.id).await.unwrap()[0]
            .head_oid
            .as_deref(),
        Some(latest.as_str())
    );
}

#[tokio::test]
async fn rejects_dirty_ignored_and_divergent_work_without_losing_data() {
    let fixture = Fixture::new().await;
    let child = fixture.add().await;
    fs::write(child.join("untracked.txt"), "keep me").unwrap();
    assert_eq!(
        fixture
            .api
            .update_submodule(&fixture.id, CHILD)
            .await
            .unwrap_err()
            .code,
        ErrorCode::DirtyWorktree
    );
    assert_eq!(
        fixture
            .api
            .remove_submodule(&fixture.id, CHILD, true)
            .await
            .unwrap_err()
            .code,
        ErrorCode::DirtyWorktree
    );
    fs::remove_file(child.join("untracked.txt")).unwrap();
    let git_dir = git(&child, &["rev-parse", "--absolute-git-dir"]);
    fs::write(Path::new(&git_dir).join("info/exclude"), "precious.cache\n").unwrap();
    fs::write(child.join("precious.cache"), "ignored local data").unwrap();
    assert_eq!(
        fixture
            .api
            .remove_submodule(&fixture.id, CHILD, true)
            .await
            .unwrap_err()
            .code,
        ErrorCode::DirtyWorktree
    );
    assert_eq!(
        fs::read_to_string(child.join("precious.cache")).unwrap(),
        "ignored local data"
    );
    git(&child, &["config", "user.name", "Submodule Tests"]);
    git(&child, &["config", "user.email", "tests@example.invalid"]);
    git(&child, &["config", "commit.gpgSign", "false"]);
    git(&child, &["switch", "--detach"]);
    let local = commit(&child, "local detached work");
    commit(&fixture.source, "different remote work");
    git(&fixture.source, &["push"]);
    assert_eq!(
        fixture
            .api
            .update_submodule(&fixture.id, CHILD)
            .await
            .unwrap_err()
            .code,
        ErrorCode::NonFastForward
    );
    assert_eq!(git(&child, &["rev-parse", "HEAD"]), local);
}

#[tokio::test]
async fn attached_branch_pull_and_push_use_its_tracking_remote() {
    let fixture = Fixture::new().await;
    let child = fixture.add().await;
    let latest = commit(&fixture.source, "pull on branch");
    git(&fixture.source, &["push"]);
    fixture
        .api
        .sync_submodule(&fixture.id, CHILD, SubmoduleSyncAction::Pull)
        .await
        .unwrap();
    assert_eq!(git(&child, &["rev-parse", "HEAD"]), latest);
    git(&child, &["config", "user.name", "Submodule Tests"]);
    git(&child, &["config", "user.email", "tests@example.invalid"]);
    git(&child, &["config", "commit.gpgSign", "false"]);
    let local = commit(&child, "push from child");
    fixture
        .api
        .sync_submodule(&fixture.id, CHILD, SubmoduleSyncAction::Push)
        .await
        .unwrap();
    assert_eq!(
        git(&fixture.remote, &["rev-parse", "refs/heads/main"]),
        local
    );
}

#[tokio::test]
async fn rejects_unsafe_paths_urls_and_unknown_modules() {
    let fixture = Fixture::new().await;
    for path in [
        "../outside",
        ".git/objects",
        "nested/../../outside",
        "C:/outside",
        "-option",
        "nested\\outside",
        ".",
    ] {
        assert_eq!(
            fixture
                .api
                .add_submodule(&fixture.id, fixture.remote.to_str().unwrap(), path)
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidPath,
            "{path}"
        );
    }
    assert_eq!(
        fixture
            .api
            .add_submodule(&fixture.id, "ext::unsafe", "valid")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ProtectedOperation
    );
    assert_eq!(
        fixture
            .api
            .update_submodule(&fixture.id, "unknown")
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        fixture
            .api
            .set_submodule_url(&fixture.id, "unknown", "https://example.test/repo.git")
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
}

#[tokio::test]
async fn allows_multiple_staged_additions_but_preserves_unstaged_gitmodules_edits() {
    let fixture = Fixture::new().await;
    let url = fixture.remote.to_str().unwrap();
    fixture
        .api
        .add_submodule(&fixture.id, url, "first")
        .await
        .unwrap();
    fixture
        .api
        .add_submodule(&fixture.id, url, "second")
        .await
        .unwrap();
    assert_eq!(
        fixture
            .api
            .list_submodules(&fixture.id)
            .await
            .unwrap()
            .len(),
        2
    );
    let modules_path = fixture.root.join(".gitmodules");
    let text = format!(
        "{}\n# private working edit\n",
        fs::read_to_string(&modules_path).unwrap()
    );
    fs::write(&modules_path, &text).unwrap();
    assert_eq!(
        fixture
            .api
            .add_submodule(&fixture.id, url, "third")
            .await
            .unwrap_err()
            .code,
        ErrorCode::DirtyWorktree
    );
    assert_eq!(fs::read_to_string(&modules_path).unwrap(), text);
    assert!(!fixture.root.join("third").exists());
    assert!(!git(&fixture.root, &["show", ":.gitmodules"]).contains("private working edit"));
}

#[tokio::test]
async fn detached_update_obeys_configured_remote_branch() {
    let fixture = Fixture::new().await;
    let child = fixture.add().await;
    git(&child, &["switch", "--detach"]);
    git(&fixture.source, &["switch", "-c", "release"]);
    let expected = commit(&fixture.source, "release commit");
    git(
        &fixture.source,
        &["push", "--set-upstream", "origin", "release"],
    );
    git(
        &fixture.root,
        &[
            "config",
            "--file",
            ".gitmodules",
            &format!("submodule.{CHILD}.branch"),
            "release",
        ],
    );
    fixture
        .api
        .update_submodule(&fixture.id, CHILD)
        .await
        .unwrap();
    assert_eq!(git(&child, &["rev-parse", "HEAD"]), expected);
}

#[tokio::test]
async fn pull_preserves_ignored_files_when_remote_starts_tracking_them() {
    for detached in [false, true] {
        let fixture = Fixture::new().await;
        let child = fixture.add().await;
        if detached {
            git(&child, &["switch", "--detach"]);
        }
        let git_dir = git(&child, &["rev-parse", "--absolute-git-dir"]);
        fs::write(Path::new(&git_dir).join("info/exclude"), "precious.cache\n").unwrap();
        fs::write(child.join("precious.cache"), "local data to preserve").unwrap();
        fs::write(fixture.source.join("precious.cache"), "remote replacement").unwrap();
        git(&fixture.source, &["add", "precious.cache"]);
        git(
            &fixture.source,
            &["commit", "-m", "track formerly ignored file"],
        );
        git(&fixture.source, &["push"]);
        assert_eq!(
            fixture
                .api
                .update_submodule(&fixture.id, CHILD)
                .await
                .unwrap_err()
                .code,
            ErrorCode::DirtyWorktree
        );
        assert_eq!(
            fs::read_to_string(child.join("precious.cache")).unwrap(),
            "local data to preserve"
        );
    }
}

#[tokio::test]
async fn pushing_a_submodule_does_not_publish_unlisted_nested_repositories() {
    let fixture = Fixture::new().await;
    let child = fixture.add().await;
    let source = fixture._temp.path().join("nested-source");
    let remote = fixture._temp.path().join("nested.git");
    init(&source);
    git(
        fixture._temp.path(),
        &[
            "clone",
            "--bare",
            source.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    let remote_before = git(&remote, &["rev-parse", "main"]);
    git(&child, &["config", "user.name", "Submodule Tests"]);
    git(&child, &["config", "user.email", "tests@example.invalid"]);
    git(&child, &["config", "commit.gpgSign", "false"]);
    git(
        &child,
        &["submodule", "add", "--", remote.to_str().unwrap(), "nested"],
    );
    let nested = child.join("nested");
    git(&nested, &["config", "user.name", "Submodule Tests"]);
    git(&nested, &["config", "user.email", "tests@example.invalid"]);
    git(&nested, &["config", "commit.gpgSign", "false"]);
    let unpublished = commit(&nested, "unlisted nested work");
    git(&child, &["add", "nested"]);
    git(&child, &["commit", "-m", "record nested commit"]);
    git(&child, &["config", "push.recurseSubmodules", "on-demand"]);
    fixture
        .api
        .sync_submodule(&fixture.id, CHILD, SubmoduleSyncAction::Push)
        .await
        .unwrap();
    assert_eq!(git(&remote, &["rev-parse", "main"]), remote_before);
    assert_ne!(unpublished, remote_before);
    assert_eq!(
        git(&fixture.remote, &["rev-parse", "main"]),
        git(&child, &["rev-parse", "HEAD"])
    );
}

#[tokio::test]
async fn scoped_parent_pull_does_not_recursively_update_children() {
    let fixture = Fixture::new().await;
    let child = fixture.add().await;
    let checkout = fixture._temp.path().join("parent-checkout");
    git(
        fixture._temp.path(),
        &[
            "clone",
            fixture.root.to_str().unwrap(),
            checkout.to_str().unwrap(),
        ],
    );
    git(&checkout, &["submodule", "update", "--init", "--", CHILD]);
    let before = git(&checkout.join(CHILD), &["rev-parse", "HEAD"]);
    let latest = commit(&fixture.source, "new child revision");
    git(&fixture.source, &["push"]);
    git(&child, &["pull", "--ff-only"]);
    git(&fixture.root, &["add", "--", CHILD]);
    git(&fixture.root, &["commit", "-m", "advance child pointer"]);
    git(&checkout, &["config", "submodule.recurse", "true"]);
    let (id, _) = fixture.api.open(&checkout).await.unwrap();
    fixture
        .api
        .pull(
            &id,
            &PullOptions {
                remote: None,
                branch: None,
                mode: PullMode::FastForwardOnly,
                prune: true,
                autostash: false,
                skip_submodules: true,
            },
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        git(&checkout, &["rev-parse", "HEAD"]),
        git(&fixture.root, &["rev-parse", "HEAD"])
    );
    assert_eq!(git(&checkout.join(CHILD), &["rev-parse", "HEAD"]), before);
    assert_ne!(before, latest);
}

#[tokio::test]
async fn initialization_and_removal_preserve_local_files_in_a_deinitialized_submodule() {
    let fixture = Fixture::new().await;
    let child = fixture.add().await;
    git(&fixture.root, &["submodule", "deinit", "--", CHILD]);
    fs::write(child.join("content.txt"), "uncommitted local work").unwrap();
    let error = fixture
        .api
        .update_submodule(&fixture.id, CHILD)
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::DirtyWorktree);
    assert_eq!(
        fixture
            .api
            .remove_submodule(&fixture.id, CHILD, true)
            .await
            .unwrap_err()
            .code,
        ErrorCode::DirtyWorktree
    );
    assert_eq!(
        fs::read_to_string(child.join("content.txt")).unwrap(),
        "uncommitted local work"
    );
    assert!(!child.join(".git").exists());
}

#[tokio::test]
async fn removal_preserves_files_inside_a_deinitialized_nested_submodule() {
    let fixture = Fixture::new().await;
    let child = fixture.add().await;
    git(&child, &["config", "user.name", "Submodule Tests"]);
    git(&child, &["config", "user.email", "tests@example.invalid"]);
    git(&child, &["config", "commit.gpgSign", "false"]);
    git(
        &child,
        &[
            "submodule",
            "add",
            "--",
            fixture.remote.to_str().unwrap(),
            "nested",
        ],
    );
    git(&child, &["commit", "-m", "add nested module"]);
    git(&fixture.root, &["add", "--", CHILD]);
    git(&fixture.root, &["commit", "-m", "record nested module"]);
    git(&child, &["submodule", "deinit", "--", "nested"]);
    let precious = child.join("nested/precious.txt");
    fs::write(&precious, "nested local work").unwrap();
    assert!(
        git(
            &child,
            &[
                "status",
                "--porcelain",
                "--ignored",
                "--ignore-submodules=none"
            ]
        )
        .is_empty(),
        "Git status does not report files in deinitialized nested submodules"
    );
    assert_eq!(
        fixture
            .api
            .remove_submodule(&fixture.id, CHILD, true)
            .await
            .unwrap_err()
            .code,
        ErrorCode::DirtyWorktree
    );
    assert_eq!(fs::read_to_string(precious).unwrap(), "nested local work");
}

/// A pointer conflict has no file to merge: the view describes the three
/// commits from the submodule itself, and taking a side stages that commit.
#[tokio::test]
async fn submodule_pointer_conflict_is_described_and_resolved_by_side() {
    let fixture = Fixture::new().await;
    let child = fixture.add().await;
    let base = git(&fixture.root, &["rev-parse", "HEAD"]);

    let ours = commit(&child, "ours child commit");
    git(&fixture.root, &["add", "--", CHILD]);
    git(&fixture.root, &["commit", "-m", "point at ours"]);
    git(&fixture.root, &["checkout", "-b", "theirs", &base]);
    git(&child, &["checkout", "--detach", "HEAD~1"]);
    let theirs = commit(&child, "theirs child commit");
    git(&fixture.root, &["add", "--", CHILD]);
    git(&fixture.root, &["commit", "-m", "point at theirs"]);
    git(&fixture.root, &["checkout", "main"]);
    git(&child, &["checkout", "--detach", &ours]);
    let merge = Command::new("git")
        .arg("-C")
        .arg(&fixture.root)
        .args(["merge", "theirs"])
        .output()
        .expect("run merge");
    assert!(!merge.status.success(), "the pointers conflict");

    let snapshot = fixture.api.snapshot(&fixture.id).await.unwrap();
    let entry = snapshot
        .status
        .entries
        .iter()
        .find(|entry| entry.path == CHILD)
        .expect("conflicted submodule");
    assert!(entry.conflicted && entry.submodule);

    let details = fixture
        .api
        .submodule_conflict_details(&fixture.id, CHILD)
        .await
        .unwrap();
    assert_eq!(details.current.as_ref().unwrap().oid, ours);
    assert_eq!(details.ours.as_ref().unwrap().oid, ours);
    let incoming = details.theirs.as_ref().unwrap();
    assert_eq!(incoming.oid, theirs);
    assert_eq!(incoming.summary.as_deref(), Some("theirs child commit"));

    let expected = fixture
        .api
        .conflict_details(&fixture.id, CHILD)
        .await
        .unwrap()
        .expected_state;
    let result = fixture
        .api
        .resolve_conflict(
            &fixture.id,
            CHILD,
            gitcat_contracts::ConflictResolution::Theirs,
            &expected,
        )
        .await
        .unwrap();
    assert!(result.conflicts.is_empty());
    let staged = git(&fixture.root, &["ls-files", "--stage", "--", CHILD]);
    assert!(staged.contains(&theirs), "{staged}");
    assert!(staged.contains(" 0\t"), "{staged}");
}
