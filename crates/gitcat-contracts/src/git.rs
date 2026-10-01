use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RepositoryId(pub Uuid);

impl RepositoryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RepositoryId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub Uuid);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryInfo {
    pub root: String,
    pub git_dir: String,
    pub common_dir: String,
    pub name: String,
    pub is_bare: bool,
    pub object_format: ObjectFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ObjectFormat {
    #[default]
    Sha1,
    Sha256,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HeadState {
    Branch { name: String, oid: String },
    Detached { oid: String },
    Unborn { intended_branch: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryOperationState {
    #[default]
    Normal,
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Bisect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RepositoryCapabilities {
    pub shallow: bool,
    pub partial_clone: bool,
    pub sparse_checkout: bool,
    pub worktree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationProgress {
    pub current: u32,
    pub total: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

/// Where an interrupted operation is bringing changes from, so the working tree
/// panel can name both sides instead of only the branch that is checked out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationSource {
    pub incoming: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onto: Option<String>,
    /// The object id the incoming side points at, where Git recorded one. A
    /// merge in progress is the second parent the working copy will commit, so
    /// the graph can draw the connection that is not in the history yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incoming_oid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    pub generation: String,
    pub head: HeadState,
    pub operation_state: RepositoryOperationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_progress: Option<OperationProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_source: Option<OperationSource>,
    pub status: WorktreeStatus,
    pub local_branches: Vec<BranchInfo>,
    pub remote_branches: Vec<BranchInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_conflict_target: Option<String>,
    pub tags: Vec<RefLabel>,
    pub remotes: Vec<RemoteInfo>,
    pub capabilities: RepositoryCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
    Untracked,
    Ignored,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineStats {
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusEntry {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<ChangeKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<ChangeKind>,
    pub conflicted: bool,
    pub submodule: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_stats: Option<LineStats>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_stats: Option<LineStats>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeStatus {
    pub clean: bool,
    pub ahead: u32,
    pub behind: u32,
    pub stash_count: u32,
    pub entries: Vec<StatusEntry>,
}

impl Default for WorktreeStatus {
    fn default() -> Self {
        Self {
            clean: true,
            ahead: 0,
            behind: 0,
            stash_count: 0,
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    LocalBranch,
    RemoteBranch,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefLabel {
    pub name: String,
    pub full_name: String,
    pub kind: RefKind,
    pub is_head: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub full_name: String,
    pub oid: String,
    pub kind: RefKind,
    pub is_head: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahead: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
    /// Structured view of the fetch URL, falling back to the push URL. `None`
    /// for local paths and URL shapes without a host/path pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<RemoteUrlParts>,
    /// Hosting service recognised from the URL host alone. The UI may still
    /// override this for self-hosted installations.
    #[serde(default)]
    pub forge: ForgeKind,
    /// Repository home page, built from the parsed URL and the forge layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
}

/// Transport a remote URL names. Git accepts several spellings for the same
/// endpoint, and the differences matter: only the explicit `ssh://` form
/// carries a port, while `git@host:path` treats everything after the colon as
/// the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RemoteUrlScheme {
    Https,
    Http,
    /// Explicit `ssh://user@host:port/path`.
    Ssh,
    /// Abbreviated `user@host:path`, which has no port.
    ScpLike,
    Git,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteUrlParts {
    pub scheme: RemoteUrlScheme,
    /// Lower-cased host without credentials or port.
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Repository path without a leading slash or a trailing `.git`.
    pub path: String,
    /// Everything in `path` above the repository itself: the user for GitHub,
    /// the full group chain for GitLab subgroups, `org/project` for Azure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ForgeKind {
    #[default]
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "gitlab")]
    GitLab,
    #[serde(rename = "bitbucket")]
    Bitbucket,
    #[serde(rename = "gitea")]
    Gitea,
    #[serde(rename = "azure_devops")]
    AzureDevOps,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitTime {
    pub seconds: i64,
    pub offset_minutes: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub parent_oid: String,
    pub from_lane: usize,
    pub to_lane: usize,
    pub merge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GraphCell {
    pub lane: usize,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StashRef {
    pub index: usize,
    pub selector: String,
    pub oid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitSummary {
    pub oid: String,
    pub short_oid: String,
    pub parent_oids: Vec<String>,
    pub subject: String,
    pub body_preview: String,
    pub author: Identity,
    pub authored_at: CommitTime,
    pub committed_at: CommitTime,
    pub decorations: Vec<RefLabel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stash: Option<StashRef>,
    pub graph: GraphCell,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneState {
    pub heads: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub merge_reserved: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryCursor {
    pub generation: String,
    pub offset: usize,
    pub lanes: LaneState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum HistoryScope {
    CurrentBranch,
    AllRefs,
    Ref(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub scope: HistoryScope,
    pub cursor: Option<HistoryCursor>,
    pub limit: usize,
}

impl Default for HistoryQuery {
    fn default() -> Self {
        Self {
            scope: HistoryScope::AllRefs,
            cursor: None,
            limit: 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryPage {
    pub generation: String,
    pub commits: Vec<CommitSummary>,
    pub next_cursor: Option<HistoryCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitSearchQuery {
    pub query: String,
    pub scope: HistoryScope,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitSearchHit {
    pub oid: String,
    pub subject: String,
    pub body_excerpt: Option<String>,
    pub matched_subject: bool,
    pub matched_body: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitSearchResult {
    pub total: usize,
    pub truncated: bool,
    pub hits: Vec<CommitSearchHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffStats {
    pub files: u32,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub old_path: Option<String>,
    pub new_path: String,
    pub status: ChangeKind,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub similarity: Option<u8>,
    pub binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitDetails {
    pub oid: String,
    pub short_oid: String,
    pub tree_oid: String,
    pub parent_oids: Vec<String>,
    pub author: Identity,
    pub committer: Identity,
    pub authored_at: CommitTime,
    pub committed_at: CommitTime,
    pub subject: String,
    pub body: String,
    pub stats: DiffStats,
    pub files: Vec<ChangedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffTarget {
    Worktree,
    Staged,
    HeadToWorktree,
    Commit { oid: String, parent_index: usize },
    Between { base_oid: String, head_oid: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffRequest {
    pub target: DiffTarget,
    pub path: String,
    pub context_lines: u16,
    pub ignore_whitespace: bool,
    pub max_bytes: usize,
    #[serde(default)]
    pub whole_file: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    NoNewline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

/// What happens to the chosen lines of a worktree diff. `Stage` and `Discard`
/// read the unstaged diff, `Unstage` the staged one -- each acts on the side
/// the lines were picked from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinePatchAction {
    Stage,
    Unstage,
    Discard,
}

/// Changed lines as the diff pane showed them. The backend re-reads the diff
/// and refuses when a line no longer matches, so a stale pane cannot move the
/// wrong text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinePatchRequest {
    pub path: String,
    pub action: LinePatchAction,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: String,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    pub status: ChangeKind,
    pub binary: bool,
    pub stats: DiffStats,
    pub hunks: Vec<DiffHunk>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedState {
    pub head_oid: Option<String>,
    pub generation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullMode {
    Merge,
    FastForwardOnly,
    Rebase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinueOperation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    Ours,
    Theirs,
    MarkResolved,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictContentKind {
    Text,
    Binary,
    TooLarge,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictLineEnding {
    None,
    Lf,
    CrLf,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictLineEndingPolicy {
    Preserve,
    Lf,
    CrLf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFileContent {
    pub kind: ConflictContentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<ConflictLineEnding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictIndexVersion {
    pub oid: String,
    pub mode: String,
    pub content: ConflictFileContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictStageIdentity {
    pub oid: String,
    pub mode: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictWorktreeKind {
    Missing,
    Regular,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictWorktreeIdentity {
    pub kind: ConflictWorktreeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<ConflictLineEnding>,
    /// Unix permission bits observed when the editor snapshot was created.
    /// `None` on platforms where Git does not track executable mode this way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictExpectedState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<ConflictStageIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ours: Option<ConflictStageIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theirs: Option<ConflictStageIdentity>,
    pub result: ConflictWorktreeIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFileDetails {
    pub path: String,
    pub expected_state: ConflictExpectedState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<ConflictIndexVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ours: Option<ConflictIndexVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theirs: Option<ConflictIndexVersion>,
    pub result: ConflictFileContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPreflightState {
    Clean,
    Conflicting,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictPreflightResult {
    /// Revision exactly as requested by the caller.
    pub target: String,
    /// Full commit ID resolved before running the preflight.
    pub target_oid: String,
    pub state: ConflictPreflightState,
    #[serde(default)]
    pub conflicting_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationResult {
    pub before_oid: Option<String>,
    pub after_oid: Option<String>,
    pub generation: String,
    pub conflicts: Vec<StatusEntry>,
    pub needs_user_action: bool,
    /// What the command did not have to do, for the cases where "done" would
    /// overstate it -- a push whose destination already had the commits. The
    /// ordinary result carries nothing here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneOptions {
    pub url: String,
    pub destination: String,
    pub branch: Option<String>,
    pub depth: Option<u32>,
    pub filter_blob_none: bool,
    /// Directories to check out, or `None` for a full checkout. An empty list
    /// is sparse with nothing chosen yet: the clone lands with its root files
    /// only, which is what `git clone --sparse` gives on its own.
    #[serde(default)]
    pub sparse_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitOptions {
    pub message: String,
    pub amend: bool,
    pub signoff: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullOptions {
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub mode: PullMode,
    pub prune: bool,
    pub autostash: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushOptions {
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub set_upstream: bool,
    #[serde(default)]
    pub force: PushForce,
}

/// How far a push may go in overwriting what the remote already has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushForce {
    /// Fast-forward only, which is what a push means unless the user says
    /// otherwise.
    #[default]
    None,
    /// Overwrite, but only while the remote is where the last fetch left it.
    /// Work someone else pushed in the meantime stops the push instead of
    /// disappearing.
    WithLease,
    /// Overwrite whatever is there. The user has been told what this discards
    /// and asked for it anyway.
    Force,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchOptions {
    pub remote: Option<String>,
    pub prune: bool,
    pub tags: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StashEntry {
    pub index: usize,
    pub oid: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitActionKind {
    Checkout,
    CreateBranch,
    CherryPick,
    Revert,
    Reset,
    CreateTag,
    CopySha,
    Reword,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitActionAvailability {
    pub kind: CommitActionKind,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoreEvent {
    RepositoryChanged {
        repository_id: RepositoryId,
        generation: String,
    },
    OperationProgress {
        job_id: JobId,
        phase: String,
        message: String,
    },
    OperationCompleted {
        job_id: JobId,
        repository_id: RepositoryId,
    },
    OperationFailed {
        job_id: JobId,
        message: String,
    },
}
