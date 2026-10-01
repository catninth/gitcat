/**
 * JSON contracts emitted by `gitcat-contracts` through serde.
 *
 * Field names intentionally stay snake_case. These are payload fields, not
 * Tauri command argument names; changing them to camelCase would break the
 * transport contract.
 */

export type RepositoryId = string;
export type JobId = string;

export interface GitVersion {
  major: number;
  minor: number;
  patch: number;
  raw: string;
}

export type ObjectFormat = "sha1" | "sha256" | "unknown";

export interface RepositoryInfo {
  root: string;
  git_dir: string;
  common_dir: string;
  name: string;
  is_bare: boolean;
  object_format: ObjectFormat;
}

export interface OpenedRepository {
  repository_id: RepositoryId;
  info: RepositoryInfo;
}

export type HeadState =
  | { kind: "branch"; name: string; oid: string }
  | { kind: "detached"; oid: string }
  | { kind: "unborn"; intended_branch: string };

export type RepositoryOperationState =
  | "normal"
  | "merge"
  | "rebase"
  | "cherry_pick"
  | "revert"
  | "bisect";

export interface RepositoryCapabilities {
  shallow: boolean;
  partial_clone: boolean;
  sparse_checkout: boolean;
  worktree: boolean;
}

export type ChangeKind =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "type_changed"
  | "unmerged"
  | "untracked"
  | "ignored";

export interface LineStats {
  additions: number;
  deletions: number;
}

export interface StatusEntry {
  path: string;
  old_path?: string;
  index?: ChangeKind;
  worktree?: ChangeKind;
  conflicted: boolean;
  submodule: boolean;
  index_stats?: LineStats;
  worktree_stats?: LineStats;
}

export interface WorktreeStatus {
  clean: boolean;
  ahead: number;
  behind: number;
  stash_count: number;
  entries: StatusEntry[];
}

export type RefKind = "local_branch" | "remote_branch" | "tag";

export interface RefLabel {
  name: string;
  full_name: string;
  kind: RefKind;
  is_head: boolean;
}

export interface BranchInfo {
  name: string;
  full_name: string;
  oid: string;
  kind: RefKind;
  is_head: boolean;
  upstream?: string;
  ahead?: number;
  behind?: number;
}

export type RemoteUrlScheme = "https" | "http" | "ssh" | "scp_like" | "git" | "unknown";

export type ForgeKind = "unknown" | "github" | "gitlab" | "bitbucket" | "gitea" | "azure_devops";

export interface AvatarSettings {
  /** Resolve commit authors against the repository's hosting service. */
  enabled: boolean;
  /** Ask Gravatar for authors the hosting service does not know. Off by
   *  default: it sends a hash of the address to a third party. */
  gravatar_fallback: boolean;
}

export interface AvatarLookup {
  host: string;
  owner: string;
  repo: string;
  forge: ForgeKind;
  tip_oid?: string;
  emails: string[];
}

export interface AvatarEntry {
  email: string;
  /** A `data:` URI; the image was fetched and cached by the backend. */
  image: string;
}

export interface ForgeSettings {
  /** Show the open pull request a branch belongs to. */
  pull_requests: boolean;
  /** Show the rolled-up check state of a branch tip. */
  checks: boolean;
}

/** The repository a hosting-service request is scoped to. */
export interface ForgeRepo {
  host: string;
  owner: string;
  repo: string;
  forge: ForgeKind;
}

export type PullRequestState = "open" | "draft" | "merged" | "closed";

export interface PullRequestInfo {
  number: number;
  title: string;
  state: PullRequestState;
  author?: string;
  /** Branch being merged, named as it is in the repository that holds it. */
  head_ref: string;
  head_oid: string;
  /** Owner of the head repository when the branch lives in a fork. */
  head_owner?: string;
  base_ref: string;
  url: string;
  updated_at?: string;
}

/** `none` means nothing reported at all, which is not the same as passing. */
export type CheckState = "none" | "success" | "failure" | "pending" | "neutral";

export interface CheckSummary {
  oid: string;
  state: CheckState;
  total: number;
  failed: number;
  pending: number;
}

export type CredentialKind = "token" | "oauth";

export interface ForgeCredential {
  host: string;
  /** Last few characters of the token, never the token itself. */
  hint: string;
  kind: CredentialKind;
  /** Account the credential belongs to, when the sign-in reported one. */
  account?: string;
  /**
   * Scopes GitCat now asks for that this sign-in was never granted. A service
   * does not widen a token in place, so the only way out is to sign in again.
   */
  missing_scopes?: string[];
}

export interface ForgeAccount {
  host: string;
  login: string;
  name?: string;
  /** Served by the hosting service; the image policy already allows it. */
  avatar_url?: string;
}

/** What the user has to do to finish a device-flow sign-in. */
export interface DeviceAuthorization {
  host: string;
  user_code: string;
  verification_uri: string;
  interval_seconds: number;
  expires_in_seconds: number;
}

export type LoginState = "pending" | "complete" | "expired" | "denied";

export interface LoginPoll {
  state: LoginState;
  account?: ForgeAccount;
}

/**
 * What a repository created on a hosting service is created with. It is always
 * created under the connected account.
 */
export interface NewRepository {
  host: string;
  name: string;
  description?: string;
  private: boolean;
}

/** One repository the signed-in account can reach. */
export interface ForgeRepository {
  full_name: string;
  owner: string;
  name: string;
  private: boolean;
  fork: boolean;
  description?: string;
  default_branch?: string;
  clone_url: string;
  ssh_url?: string;
  updated_at?: string;
}

export interface RemoteUrlParts {
  scheme: RemoteUrlScheme;
  host: string;
  port?: number;
  path: string;
  owner?: string;
  repo?: string;
}

export interface RemoteInfo {
  name: string;
  fetch_url: string;
  push_url: string;
  /** Absent for local paths and URL shapes without a host/path pair. */
  url?: RemoteUrlParts;
  /** Recognised from the URL host; settings may override it per host. */
  forge: ForgeKind;
  web_url?: string;
}

export interface OperationProgress {
  current: number;
  total: number;
  subject?: string;
}

/** Where an interrupted operation is bringing changes from. */
export interface OperationSource {
  incoming: string;
  onto?: string;
  incoming_oid?: string;
}

export interface RepositorySnapshot {
  generation: string;
  head: HeadState;
  operation_state: RepositoryOperationState;
  operation_progress?: OperationProgress;
  operation_source?: OperationSource;
  status: WorktreeStatus;
  local_branches: BranchInfo[];
  remote_branches: BranchInfo[];
  default_conflict_target?: string;
  tags: RefLabel[];
  remotes: RemoteInfo[];
  capabilities: RepositoryCapabilities;
}

export interface Identity {
  name: string;
  email: string;
}

export interface CommitTime {
  seconds: number;
  offset_minutes: number;
}

export interface GraphEdge {
  parent_oid: string;
  from_lane: number;
  to_lane: number;
  merge: boolean;
}

export interface GraphCell {
  lane: number;
  edges: GraphEdge[];
}

export interface StashRef {
  index: number;
  selector: string;
  oid: string;
}

export interface CommitSummary {
  oid: string;
  short_oid: string;
  parent_oids: string[];
  subject: string;
  body_preview: string;
  author: Identity;
  authored_at: CommitTime;
  committed_at: CommitTime;
  decorations: RefLabel[];
  stash?: StashRef | null;
  graph: GraphCell;
}

export interface LaneState {
  heads: Array<string | null>;
  merge_reserved?: Record<string, number>;
}

export interface HistoryCursor {
  generation: string;
  offset: number;
  lanes: LaneState;
}

export type HistoryScope =
  | { kind: "current_branch" }
  | { kind: "all_refs" }
  | { kind: "ref"; value: string };

export interface HistoryQuery {
  scope: HistoryScope;
  cursor: HistoryCursor | null;
  limit: number;
}

export interface HistoryPage {
  generation: string;
  commits: CommitSummary[];
  next_cursor: HistoryCursor | null;
  has_more: boolean;
}

export interface CommitSearchQuery {
  query: string;
  scope: HistoryScope;
  limit: number;
}

export interface CommitSearchHit {
  oid: string;
  subject: string;
  body_excerpt: string | null;
  matched_subject: boolean;
  matched_body: boolean;
}

export interface CommitSearchResult {
  total: number;
  truncated: boolean;
  hits: CommitSearchHit[];
}

export interface DiffStats {
  files: number;
  additions: number;
  deletions: number;
}

export interface ChangedFile {
  old_path: string | null;
  new_path: string;
  status: ChangeKind;
  additions: number | null;
  deletions: number | null;
  similarity: number | null;
  binary: boolean;
}

export interface CommitDetails {
  oid: string;
  short_oid: string;
  tree_oid: string;
  parent_oids: string[];
  author: Identity;
  committer: Identity;
  authored_at: CommitTime;
  committed_at: CommitTime;
  subject: string;
  body: string;
  stats: DiffStats;
  files: ChangedFile[];
}

export type DiffTarget =
  | { kind: "worktree" }
  | { kind: "staged" }
  | { kind: "head_to_worktree" }
  | { kind: "commit"; oid: string; parent_index: number }
  | { kind: "between"; base_oid: string; head_oid: string };

export interface DiffRequest {
  target: DiffTarget;
  path: string;
  context_lines: number;
  ignore_whitespace: boolean;
  max_bytes: number;
  whole_file: boolean;
}

export type DiffLineKind = "context" | "addition" | "deletion" | "no_newline";

export interface DiffLine {
  kind: DiffLineKind;
  old_line: number | null;
  new_line: number | null;
  content: string;
}

export interface DiffHunk {
  header: string;
  old_start: number;
  old_count: number;
  new_start: number;
  new_count: number;
  lines: DiffLine[];
}

export type LinePatchAction = "stage" | "unstage" | "discard";

export interface LinePatchRequest {
  path: string;
  action: LinePatchAction;
  lines: DiffLine[];
}

export interface FileDiff {
  old_path: string | null;
  new_path: string;
  old_mode: string | null;
  new_mode: string | null;
  status: ChangeKind;
  binary: boolean;
  stats: DiffStats;
  hunks: DiffHunk[];
  truncated: boolean;
}

export interface ExpectedState {
  head_oid: string | null;
  generation: string;
}

export type PullMode = "merge" | "fast_forward_only" | "rebase";
export type ResetMode = "soft" | "mixed" | "hard";
export type ContinueOperation = "merge" | "rebase" | "cherry_pick" | "revert";

export interface MutationResult {
  before_oid: string | null;
  after_oid: string | null;
  generation: string;
  conflicts: StatusEntry[];
  needs_user_action: boolean;
  /** What the command did not have to do, when "done" would overstate it. */
  notice?: string | null;
}

export interface CloneOptions {
  url: string;
  destination: string;
  branch: string | null;
  depth: number | null;
  filter_blob_none: boolean;
  /** Directories to check out, or null for the whole tree. */
  sparse_paths: string[] | null;
}

export interface CommitOptions {
  message: string;
  amend: boolean;
  signoff: boolean;
}

export interface PullOptions {
  remote: string | null;
  branch: string | null;
  mode: PullMode;
  prune: boolean;
  autostash: boolean;
}

/** How far a push may go in overwriting what the remote already has. */
export type PushForce = "none" | "with_lease" | "force";

export interface PushOptions {
  remote: string | null;
  branch: string | null;
  set_upstream: boolean;
  /** Omitted is `"none"`: a push is fast-forward only unless it says otherwise. */
  force?: PushForce;
}

export interface FetchOptions {
  remote: string | null;
  prune: boolean;
  tags: boolean;
}

export interface StashEntry {
  index: number;
  oid: string;
  message: string;
}

export type CommitActionKind =
  | "checkout"
  | "create_branch"
  | "cherry_pick"
  | "revert"
  | "reset"
  | "create_tag"
  | "copy_sha"
  | "reword";

export interface CommitActionAvailability {
  kind: CommitActionKind;
  enabled: boolean;
  disabled_reason: string | null;
  requires_confirmation: boolean;
}

export type CoreEvent =
  | { kind: "repository_changed"; repository_id: RepositoryId; generation: string }
  | { kind: "operation_progress"; job_id: JobId; phase: string; message: string }
  | { kind: "operation_completed"; job_id: JobId; repository_id: RepositoryId }
  | { kind: "operation_failed"; job_id: JobId; message: string };

export type ErrorCode =
  | "git_not_found"
  | "unsupported_git_version"
  | "invalid_repository"
  | "repository_closed"
  | "invalid_path"
  | "invalid_request"
  | "invalid_ref_name"
  | "invalid_revision"
  | "stale_snapshot"
  | "repository_busy"
  | "dirty_worktree"
  | "conflicts_present"
  | "operation_in_progress"
  | "upstream_missing"
  | "authentication_required"
  | "network_failed"
  | "non_fast_forward"
  | "protected_operation"
  | "unsupported_operation"
  | "cancelled"
  | "timeout"
  | "output_too_large"
  | "git_command_failed"
  | "io"
  | "invalid_settings"
  | "internal";

export interface RecoveryAction {
  kind: string;
  label: string;
}

export interface ApiError {
  code: ErrorCode;
  message: string;
  details?: string;
  recovery_actions?: RecoveryAction[];
}

export interface ThemeColors {
  background: string;
  surface: string;
  panel: string;
  border: string;
  text: string;
  muted_text: string;
  accent: string;
  success: string;
  warning: string;
  danger: string;
  diff_addition: string;
  diff_deletion: string;
  graph_palette: string[];
}

export interface AppTheme {
  id: string;
  name: string;
  built_in: boolean;
  colors: ThemeColors;
}

export type FileViewMode = "path" | "tree";

export type DiffViewMode = "hunk" | "inline" | "split";

export interface GraphColumnSettings {
  refs: boolean;
  graph: boolean;
  message: boolean;
  author: boolean;
  date: boolean;
  sha: boolean;
}

export interface GraphColumnWidths {
  refs: number;
  // Null while the graph column follows the lane extent it computes itself.
  graph: number | null;
  message: number;
  author: number;
  date: number;
  sha: number;
}

export interface AppSettings {
  default_pull_mode: PullMode;
  auto_fetch_interval_minutes: number;
  auto_prune: boolean;
  history_page_size: number;
  diff_context_lines: number;
  diff_max_bytes: number;
  file_view_mode: FileViewMode;
  diff_view_mode: DiffViewMode;
  graph_columns: GraphColumnSettings;
  graph_column_widths: GraphColumnWidths;
  keybinds: KeybindSettings;
  /** Whole-interface zoom in percent; see `lib/zoom.ts`. */
  ui_zoom_percent: number;
  /** Hosting service per URL host, for installs the backend cannot name. */
  forge_overrides: Record<string, ForgeKind>;
  avatars: AvatarSettings;
  forge: ForgeSettings;
  active_theme_id: string;
  themes: AppTheme[];
}

export interface KeybindSettings {
  next_repository: string;
  previous_repository: string;
  repository_1: string;
  repository_2: string;
  repository_3: string;
  repository_4: string;
  repository_5: string;
  repository_6: string;
  repository_7: string;
  repository_8: string;
  repository_9: string;
  new_repository_tab: string;
  close_repository: string;
  reopen_closed_repository: string;
  open_repository: string;
  open_repository_folder: string;
  search_commits: string;
  open_settings: string;
  refresh_repository: string;
  toggle_left_panel: string;
  toggle_right_panel: string;
  fetch: string;
  pull: string;
  push: string;
  create_branch: string;
  stash: string;
  show_worktree: string;
  show_graph: string;
  diff_inline: string;
  diff_split: string;
  copy_selected_sha: string;
  continue_operation: string;
  abort_operation: string;
  stage_all: string;
  unstage_all: string;
  focus_commit_message: string;
  auto_resolve_conflicts: string;
  commit: string;
}

export type ConflictResolution = "ours" | "theirs" | "mark_resolved" | "delete";

export type ConflictContentKind = "text" | "binary" | "too_large" | "missing";
export type ConflictLineEnding = "none" | "lf" | "cr_lf" | "mixed";
export type ConflictLineEndingPolicy = "preserve" | "lf" | "cr_lf";

export interface ConflictFileContent {
  kind: ConflictContentKind;
  size?: number;
  text?: string;
  line_ending?: ConflictLineEnding;
}

export interface ConflictIndexVersion {
  oid: string;
  mode: string;
  content: ConflictFileContent;
}

export interface ConflictStageIdentity {
  oid: string;
  mode: string;
}

export interface ConflictExpectedState {
  base?: ConflictStageIdentity;
  ours?: ConflictStageIdentity;
  theirs?: ConflictStageIdentity;
  result: ConflictWorktreeIdentity;
}

export interface ConflictWorktreeIdentity {
  kind: "missing" | "regular" | "symlink";
  size?: number;
  sha256?: string;
  line_ending?: ConflictLineEnding;
  mode?: number;
}

export interface ConflictFileDetails {
  path: string;
  expected_state: ConflictExpectedState;
  base?: ConflictIndexVersion;
  ours?: ConflictIndexVersion;
  theirs?: ConflictIndexVersion;
  result: ConflictFileContent;
}

export type ConflictPreflightState = "clean" | "conflicting" | "unavailable";

export interface ConflictPreflightResult {
  target: string;
  target_oid: string;
  state: ConflictPreflightState;
  conflicting_paths: string[];
  unavailable_reason?: string;
}

export interface AppMetadata {
  version: string;
  commit: string;
}

export type TabKind = "repository" | "start";

export interface RepositoryTab {
  id: string;
  repository_path: string;
  display_name: string;
  order: number;
  kind?: TabKind;
  conflict_target?: string | null;
  conflict_target_disabled?: boolean;
}

export interface RepositoryGroup {
  id: string;
  name: string;
  collapsed: boolean;
  order: number;
  tabs: RepositoryTab[];
}

export interface WorkspaceState {
  version: number;
  ungrouped_tabs: RepositoryTab[];
  groups: RepositoryGroup[];
  active_tab_id: string | null;
}

export interface RecentRepository {
  path: string;
  name: string;
  opened_at: number;
}

export interface PersistedState {
  settings: AppSettings;
  workspace: WorkspaceState;
  recents: RecentRepository[];
  /** Folder the last repository was cloned into, initialized in, or opened from. */
  last_directory: string | null;
}

export interface RepositoryOverview {
  snapshot: RepositorySnapshot;
  history: HistoryPage;
  stashes: StashEntry[];
}

export interface CommitPanelData {
  details: CommitDetails;
  actions: CommitActionAvailability[];
}
