import { createDemoGitCatApi } from "./demo";
import { getGitCatRuntime, invokeTauri } from "./platform";
import type {
  AppMetadata,
  ApiError,
  CloneOptions,
  CommitActionAvailability,
  CommitDetails,
  CommitOptions,
  CommitPanelData,
  CommitSearchQuery,
  CommitSearchResult,
  ConflictExpectedState,
  ConflictFileDetails,
  ConflictLineEndingPolicy,
  ConflictPreflightResult,
  ConflictResolution,
  ContinueOperation,
  DiffRequest,
  ExpectedState,
  FetchOptions,
  FileDiff,
  LinePatchRequest,
  GitVersion,
  HistoryPage,
  HistoryQuery,
  MutationResult,
  OpenedRepository,
  PersistedState,
  PullOptions,
  PushOptions,
  RepositoryId,
  RepositoryOverview,
  RepositorySnapshot,
  ResetMode,
  StashEntry,
} from "./types";

export interface GitCatCommands {
  appMetadata(): Promise<AppMetadata>;
  probe(): Promise<GitVersion>;
  openRepository(path: string): Promise<OpenedRepository>;
  /**
   * The folder GitCat was started with, or `null`.
   *
   * The Windows Explorer context menu the installer offers launches GitCat
   * with a folder; asking for it clears it, so a reload does not reopen it.
   */
  launchRepositoryPath(): Promise<string | null>;
  initRepository(path: string, defaultBranch: string): Promise<OpenedRepository>;
  cloneRepository(options: CloneOptions): Promise<OpenedRepository>;
  closeRepository(repositoryId: RepositoryId): Promise<void>;
  /** Start watching a repository's worktree so the UI auto-refreshes on change. */
  watchRepository(repositoryId: RepositoryId): Promise<void>;
  /** Stop the active repository watch. */
  unwatchRepository(): Promise<void>;
  /** Open the repository's worktree root, or a folder inside it, in the OS file explorer. */
  openRepositoryFolder(repositoryId: RepositoryId, path?: string): Promise<void>;
  snapshot(repositoryId: RepositoryId): Promise<RepositorySnapshot>;
  history(repositoryId: RepositoryId, query: HistoryQuery): Promise<HistoryPage>;
  searchCommits(
    repositoryId: RepositoryId,
    query: CommitSearchQuery,
  ): Promise<CommitSearchResult>;
  commitDetails(
    repositoryId: RepositoryId,
    oid: string,
    parentIndex?: number,
  ): Promise<CommitDetails>;
  diff(repositoryId: RepositoryId, request: DiffRequest): Promise<FileDiff>;
  conflictPreflight(repositoryId: RepositoryId, target: string): Promise<ConflictPreflightResult>;
  conflictDetails(repositoryId: RepositoryId, path: string): Promise<ConflictFileDetails>;
  stagePaths(repositoryId: RepositoryId, paths: string[]): Promise<MutationResult>;
  unstagePaths(repositoryId: RepositoryId, paths: string[]): Promise<MutationResult>;
  discardPaths(repositoryId: RepositoryId, paths: string[]): Promise<MutationResult>;
  applyDiffLines(repositoryId: RepositoryId, request: LinePatchRequest): Promise<MutationResult>;
  stashFile(
    repositoryId: RepositoryId,
    paths: string[],
    message: string | null,
  ): Promise<MutationResult>;
  appendGitignore(repositoryId: RepositoryId, patterns: string[]): Promise<MutationResult>;
  savePatch(
    repositoryId: RepositoryId,
    paths: string[],
    staged: boolean,
    destination: string,
  ): Promise<void>;
  resolveConflict(
    repositoryId: RepositoryId,
    path: string,
    resolution: ConflictResolution,
    expectedState: ConflictExpectedState,
  ): Promise<MutationResult>;
  saveConflictResult(
    repositoryId: RepositoryId,
    path: string,
    text: string,
    lineEnding: ConflictLineEndingPolicy,
    expectedState: ConflictExpectedState,
  ): Promise<MutationResult>;
  resolveConflicts(
    repositoryId: RepositoryId,
    paths: string[],
    resolution: ConflictResolution,
  ): Promise<MutationResult>;
  autoResolveConflicts(repositoryId: RepositoryId): Promise<MutationResult>;
  createCommit(repositoryId: RepositoryId, options: CommitOptions): Promise<MutationResult>;
  /**
   * Composes the first commit of a repository that has none: what is staged if
   * anything is, otherwise a seeded `README.md`.
   */
  createInitialCommit(repositoryId: RepositoryId, message: string): Promise<MutationResult>;
  rewordCommit(
    repositoryId: RepositoryId,
    oid: string,
    message: string,
    expected: ExpectedState,
  ): Promise<MutationResult>;
  createBranch(
    repositoryId: RepositoryId,
    name: string,
    startOid: string,
    checkout: boolean,
  ): Promise<MutationResult>;
  checkoutBranch(repositoryId: RepositoryId, name: string): Promise<MutationResult>;
  renameBranch(
    repositoryId: RepositoryId,
    oldName: string,
    newName: string,
  ): Promise<MutationResult>;
  deleteBranch(
    repositoryId: RepositoryId,
    name: string,
    force: boolean,
    confirmed: boolean,
    expected: ExpectedState,
  ): Promise<MutationResult>;
  setUpstream(
    repositoryId: RepositoryId,
    branch: string,
    upstream: string,
  ): Promise<MutationResult>;
  mergeBranch(repositoryId: RepositoryId, branch: string): Promise<MutationResult>;
  addRemote(repositoryId: RepositoryId, name: string, url: string): Promise<MutationResult>;
  fetch(repositoryId: RepositoryId, options: FetchOptions): Promise<MutationResult>;
  pull(repositoryId: RepositoryId, options: PullOptions): Promise<MutationResult>;
  push(repositoryId: RepositoryId, options: PushOptions): Promise<MutationResult>;
  checkoutCommit(repositoryId: RepositoryId, oid: string): Promise<MutationResult>;
  createTag(
    repositoryId: RepositoryId,
    name: string,
    oid: string,
    message: string | null,
  ): Promise<MutationResult>;
  cherryPick(
    repositoryId: RepositoryId,
    oid: string,
    mainlineParent?: number | null,
  ): Promise<MutationResult>;
  revertCommit(
    repositoryId: RepositoryId,
    oid: string,
    mainlineParent?: number | null,
  ): Promise<MutationResult>;
  resetCommit(
    repositoryId: RepositoryId,
    oid: string,
    mode: ResetMode,
    confirmed: boolean,
    expected: ExpectedState,
  ): Promise<MutationResult>;
  commitActionAvailability(
    repositoryId: RepositoryId,
    oid: string,
  ): Promise<CommitActionAvailability[]>;
  continueOperation(
    repositoryId: RepositoryId,
    operation: ContinueOperation,
  ): Promise<MutationResult>;
  abortOperation(
    repositoryId: RepositoryId,
    operation: ContinueOperation,
  ): Promise<MutationResult>;
  skipOperation(
    repositoryId: RepositoryId,
    operation: ContinueOperation,
  ): Promise<MutationResult>;
  stashList(repositoryId: RepositoryId): Promise<StashEntry[]>;
  stashPush(
    repositoryId: RepositoryId,
    message: string | null,
    includeUntracked: boolean,
  ): Promise<MutationResult>;
  stashApply(repositoryId: RepositoryId, oid: string, pop: boolean): Promise<MutationResult>;
  stashDrop(
    repositoryId: RepositoryId,
    oid: string,
    confirmed: boolean,
    expected: ExpectedState,
  ): Promise<MutationResult>;
  loadPersistedState(): Promise<PersistedState>;
  savePersistedState(state: PersistedState): Promise<void>;
}

export interface GitCatApi extends GitCatCommands {
  readonly runtime: "tauri" | "browser";
  loadRepositoryOverview(
    repositoryId: RepositoryId,
    query?: HistoryQuery,
  ): Promise<RepositoryOverview>;
  loadCommitPanel(
    repositoryId: RepositoryId,
    oid: string,
    parentIndex?: number,
  ): Promise<CommitPanelData>;
}

const defaultHistoryQuery = (): HistoryQuery => ({
  scope: { kind: "all_refs" },
  cursor: null,
  limit: 200,
});

export async function loadRepositoryOverview(
  commands: GitCatCommands,
  repositoryId: RepositoryId,
  query: HistoryQuery = defaultHistoryQuery(),
): Promise<RepositoryOverview> {
  const [snapshot, history, stashes] = await Promise.all([
    commands.snapshot(repositoryId),
    commands.history(repositoryId, query),
    commands.stashList(repositoryId),
  ]);
  return { snapshot, history, stashes };
}

export async function loadCommitPanel(
  commands: GitCatCommands,
  repositoryId: RepositoryId,
  oid: string,
  parentIndex = 0,
): Promise<CommitPanelData> {
  const [details, actions] = await Promise.all([
    commands.commitDetails(repositoryId, oid, parentIndex),
    commands.commitActionAvailability(repositoryId, oid),
  ]);
  return { details, actions };
}

export function createTauriGitCatApi(): GitCatApi {
  const commands: GitCatCommands = {
    appMetadata: () => invokeTauri("app_metadata"),
    probe: () => invokeTauri("git_probe"),
    openRepository: (path) => invokeTauri("repository_open", { path }),
    launchRepositoryPath: () => invokeTauri("launch_repository_path"),
    initRepository: (path, defaultBranch) =>
      invokeTauri("repository_init", { path, defaultBranch }),
    cloneRepository: (options) => invokeTauri("repository_clone", { options }),
    closeRepository: (repositoryId) =>
      invokeTauri("repository_close", { repositoryId }),
    watchRepository: (repositoryId) =>
      invokeTauri("repository_watch", { repositoryId }),
    unwatchRepository: () => invokeTauri("repository_unwatch"),
    openRepositoryFolder: (repositoryId, path) =>
      invokeTauri("repository_reveal", { repositoryId, path: path ?? null }),
    snapshot: (repositoryId) =>
      invokeTauri("repository_snapshot", { repositoryId }),
    history: (repositoryId, query) =>
      invokeTauri("history_page", { repositoryId, query }),
    searchCommits: (repositoryId, query) =>
      invokeTauri("history_search", { repositoryId, query }),
    commitDetails: (repositoryId, oid, parentIndex = 0) =>
      invokeTauri("commit_details", { repositoryId, oid, parentIndex }),
    diff: (repositoryId, request) =>
      invokeTauri("file_diff", { repositoryId, request }),
    conflictPreflight: (repositoryId, target) =>
      invokeTauri("conflicts_preflight", { repositoryId, target }),
    conflictDetails: (repositoryId, path) =>
      invokeTauri("conflict_details", { repositoryId, path }),
    stagePaths: (repositoryId, paths) =>
      invokeTauri("paths_stage", { repositoryId, paths }),
    unstagePaths: (repositoryId, paths) =>
      invokeTauri("paths_unstage", { repositoryId, paths }),
    discardPaths: (repositoryId, paths) =>
      invokeTauri("paths_discard", { repositoryId, paths }),
    applyDiffLines: (repositoryId, request) =>
      invokeTauri("diff_lines_apply", { repositoryId, request }),
    stashFile: (repositoryId, paths, message) =>
      invokeTauri("path_stash", { repositoryId, paths, message }),
    appendGitignore: (repositoryId, patterns) =>
      invokeTauri("gitignore_append", { repositoryId, patterns }),
    savePatch: (repositoryId, paths, staged, destination) =>
      invokeTauri("file_patch_save", { repositoryId, paths, staged, destination }),
    resolveConflict: (repositoryId, path, resolution, expectedState) =>
      invokeTauri("conflict_resolve", { repositoryId, path, resolution, expectedState }),
    saveConflictResult: (repositoryId, path, text, lineEnding, expectedState) =>
      invokeTauri("conflict_save_edited", { repositoryId, path, text, lineEnding, expectedState }),
    resolveConflicts: (repositoryId, paths, resolution) =>
      invokeTauri("conflicts_resolve", { repositoryId, paths, resolution }),
    autoResolveConflicts: (repositoryId) =>
      invokeTauri("conflicts_auto_resolve", { repositoryId }),
    createCommit: (repositoryId, options) =>
      invokeTauri("create_commit", { repositoryId, options }),
    createInitialCommit: (repositoryId, message) =>
      invokeTauri("commit_initial", { repositoryId, message }),
    rewordCommit: (repositoryId, oid, message, expected) =>
      invokeTauri("commit_reword", { repositoryId, oid, message, expected }),
    createBranch: (repositoryId, name, startOid, checkout) =>
      invokeTauri("branch_create", { repositoryId, name, startOid, checkout }),
    checkoutBranch: (repositoryId, name) =>
      invokeTauri("branch_checkout", { repositoryId, name }),
    renameBranch: (repositoryId, oldName, newName) =>
      invokeTauri("branch_rename", { repositoryId, oldName, newName }),
    deleteBranch: (repositoryId, name, force, confirmed, expected) =>
      invokeTauri("branch_delete", {
        repositoryId,
        name,
        force,
        confirmed,
        expected,
      }),
    setUpstream: (repositoryId, branch, upstream) =>
      invokeTauri("branch_set_upstream", { repositoryId, branch, upstream }),
    mergeBranch: (repositoryId, branch) =>
      invokeTauri("branch_merge", { repositoryId, branch }),
    addRemote: (repositoryId, name, url) =>
      invokeTauri("remote_add", { repositoryId, name, url }),
    fetch: (repositoryId, options) =>
      invokeTauri("remote_fetch", { repositoryId, options }),
    pull: (repositoryId, options) =>
      invokeTauri("remote_pull", { repositoryId, options }),
    push: (repositoryId, options) =>
      invokeTauri("remote_push", { repositoryId, options }),
    checkoutCommit: (repositoryId, oid) =>
      invokeTauri("commit_checkout", { repositoryId, oid }),
    createTag: (repositoryId, name, oid, message) =>
      invokeTauri("tag_create", { repositoryId, name, oid, message }),
    cherryPick: (repositoryId, oid, mainlineParent = null) =>
      invokeTauri("commit_cherry_pick", { repositoryId, oid, mainlineParent }),
    revertCommit: (repositoryId, oid, mainlineParent = null) =>
      invokeTauri("commit_revert", { repositoryId, oid, mainlineParent }),
    resetCommit: (repositoryId, oid, mode, confirmed, expected) =>
      invokeTauri("commit_reset", {
        repositoryId,
        oid,
        mode,
        confirmed,
        expected,
      }),
    commitActionAvailability: (repositoryId, oid) =>
      invokeTauri("commit_action_availability", { repositoryId, oid }),
    continueOperation: (repositoryId, operation) =>
      invokeTauri("operation_continue", { repositoryId, operation }),
    abortOperation: (repositoryId, operation) =>
      invokeTauri("operation_abort", { repositoryId, operation }),
    skipOperation: (repositoryId, operation) =>
      invokeTauri("operation_skip", { repositoryId, operation }),
    stashList: (repositoryId) =>
      invokeTauri("stash_list", { repositoryId }),
    stashPush: (repositoryId, message, includeUntracked) =>
      invokeTauri("stash_push", { repositoryId, message, includeUntracked }),
    stashApply: (repositoryId, oid, pop) =>
      invokeTauri("stash_apply", { repositoryId, oid, pop }),
    stashDrop: (repositoryId, oid, confirmed, expected) =>
      invokeTauri("stash_drop", {
        repositoryId,
        oid,
        confirmed,
        expected,
      }),
    loadPersistedState: () => invokeTauri("persisted_state_load"),
    savePersistedState: (state) => invokeTauri("persisted_state_save", { state }),
  };

  return {
    ...commands,
    runtime: "tauri",
    loadRepositoryOverview: (repositoryId, query = defaultHistoryQuery()) =>
      loadRepositoryOverview(commands, repositoryId, query),
    loadCommitPanel: (repositoryId, oid, parentIndex = 0) =>
      loadCommitPanel(commands, repositoryId, oid, parentIndex),
  };
}

export function isApiError(value: unknown): value is ApiError {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<ApiError>;
  return typeof candidate.code === "string" && typeof candidate.message === "string";
}

export function getApiError(value: unknown): ApiError {
  if (isApiError(value)) return value;
  if (value instanceof Error) {
    return { code: "internal", message: value.message };
  }
  return { code: "internal", message: String(value) };
}

export const gitcatApi: GitCatApi =
  getGitCatRuntime() === "tauri" ? createTauriGitCatApi() : createDemoGitCatApi();

export const api = gitcatApi;
