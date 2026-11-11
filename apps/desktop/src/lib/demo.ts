import type { GitCatApi } from "./api";
import packageMetadata from "../../package.json";
import { cloneDefaultThemes, DEFAULT_THEME_ID } from "../app/themePresets";
import { DEFAULT_KEYBINDS } from "./keybinds";
import { createDemoSubmodules, demoRepositoryKey, demoSubmodulePath } from "./demoSubmodules";
import type {
  AppMetadata,
  PullPhase,
  ApiError,
  BranchInfo,
  ChangedFile,
  CloneOptions,
  CommitActionAvailability,
  CommitDetails,
  CommitOptions,
  CommitPanelData,
  CommitSearchQuery,
  CommitSearchResult,
  CommitSummary,
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
  GitVersion,
  HistoryPage,
  HistoryQuery,
  Identity,
  LinePatchRequest,
  MutationResult,
  OpenedRepository,
  PersistedState,
  PullOptions,
  PushOptions,
  RefLabel,
  RepositoryId,
  RepositoryInfo,
  RepositoryOverview,
  RepositorySnapshot,
  ResetMode,
  StashEntry,
  StatusEntry,
  SubmoduleInfo,
  SubmoduleSyncAction,
} from "./types";

const DEMO_REPOSITORY_ID = "7e43a7bc-d48b-4f8f-92ab-43a41676ce7e";
const STORAGE_KEY = "gitcat.demo.persisted-state.v1";
const NETWORK_DELAY_MS = 24;

const oids = [
  "3fd829c4680a741cc0d61ef11b4d9e3bc602188f",
  "917de27eb1a65a3062d0f0e08b8d63f67a4fc130",
  "f20846dafe9f8d1d9748909f7ed82bcf12ec02b5",
  "712c90b37c341ddf63ea7220bd43a2ed516eb4cc",
  "b86a940690b57ff9a33c07d3732901655d010788",
  "ce30fa4458be5a4fbe5e3c1de717f9114684d252",
  "2cb6d7e913b20347f77827b7e33f54ca24449638",
  "d70a94f75d0d7ed44f3af762847f1bf0acbc64b9",
  "578c50136d4770cbcb12e69eb697c437d84af65a",
  "04af90d0dd2371e33e869f3d0e3eaeca49e06356",
] as const;

const ris: Identity = { name: "Ris", email: "ris@example.com" };
const lina: Identity = { name: "Lina Kovács", email: "lina@example.com" };

const localLabel = (name: string, isHead = false): RefLabel => ({
  name,
  full_name: `refs/heads/${name}`,
  kind: "local_branch",
  is_head: isHead,
});

const remoteLabel = (name: string): RefLabel => ({
  name,
  full_name: `refs/remotes/${name}`,
  kind: "remote_branch",
  is_head: false,
});

const tagLabel = (name: string): RefLabel => ({
  name,
  full_name: `refs/tags/${name}`,
  kind: "tag",
  is_head: false,
});

interface CommitSeed {
  subject: string;
  body: string;
  author: Identity;
  hoursAgo: number;
  lane: number;
  parents: number[];
  parentLanes: number[];
  decorations?: RefLabel[];
}

const commitSeeds: CommitSeed[] = [
  {
    subject: "fix(authentication): reset session after unsuccessful login",
    body: "Prevent stale sessions after rejected credentials.\n\nThe session cache now updates atomically and preserves the original audit entry.",
    author: ris,
    hoursAgo: 2,
    lane: 0,
    parents: [1],
    parentLanes: [0],
    decorations: [localLabel("main", true)],
  },
  {
    subject: "merge: integrate diff viewer into main",
    body: "Merge the structured diff renderer after keyboard and large-file testing.",
    author: ris,
    hoursAgo: 5,
    lane: 0,
    parents: [2, 5],
    parentLanes: [0, 1],
    decorations: [remoteLabel("origin/main")],
  },
  {
    subject: "feat(search): find commits in subject and description",
    body: "Add fixed-string, case-insensitive history search for Ctrl+F.\n\nResults distinguish subject and body matches.",
    author: lina,
    hoursAgo: 8,
    lane: 0,
    parents: [3],
    parentLanes: [0],
  },
  {
    subject: "perf(graph): virtualize long commit histories",
    body: "Keep graph lane state in the page cursor and render only visible rows.",
    author: ris,
    hoursAgo: 12,
    lane: 0,
    parents: [4],
    parentLanes: [0],
  },
  {
    subject: "feat(branches): add focused branch workflow",
    body: "Create, checkout, rename and safely delete branches from one compact menu.",
    author: lina,
    hoursAgo: 20,
    lane: 0,
    parents: [7],
    parentLanes: [0],
  },
  {
    subject: "feat(diff): render structured inline hunks",
    body: "Add old and new line numbers, binary markers and truncation state.",
    author: ris,
    hoursAgo: 10,
    lane: 1,
    parents: [6],
    parentLanes: [1],
    decorations: [localLabel("feature/diff-viewer"), remoteLabel("origin/feature/diff-viewer")],
  },
  {
    subject: "fix(pull): always honor explicit rebase mode",
    body: "Ignore ambient pull.rebase config and pass the selected strategy explicitly.",
    author: ris,
    hoursAgo: 17,
    lane: 1,
    parents: [7],
    parentLanes: [0],
  },
  {
    subject: "release: prepare GitCat 0.1 preview",
    body: "Freeze transport contracts and document the Tauri integration boundary.",
    author: lina,
    hoursAgo: 28,
    lane: 0,
    parents: [8],
    parentLanes: [0],
    decorations: [localLabel("release/0.1"), tagLabel("v0.1.0-preview")],
  },
  {
    subject: "feat(workspace): persist grouped repository tabs",
    body: "Remember group order, collapsed state, active repository and semantic theme colors.",
    author: ris,
    hoursAgo: 42,
    lane: 0,
    parents: [9],
    parentLanes: [0],
  },
  {
    subject: "chore: initialize lightweight Git core",
    body: "Introduce typed contracts, shell-free Git execution and bounded command output.",
    author: ris,
    hoursAgo: 50,
    lane: 0,
    parents: [],
    parentLanes: [],
  },
];

const nowSeconds = Math.floor(Date.now() / 1000);

const initialCommits: CommitSummary[] = commitSeeds.map((seed, index) => ({
  oid: oids[index],
  short_oid: oids[index].slice(0, 7),
  parent_oids: seed.parents.map((parent) => oids[parent]),
  subject: seed.subject,
  body_preview: seed.body.split("\n")[0],
  author: seed.author,
  authored_at: { seconds: nowSeconds - seed.hoursAgo * 3600, offset_minutes: 120 },
  committed_at: { seconds: nowSeconds - seed.hoursAgo * 3600, offset_minutes: 120 },
  decorations: seed.decorations ?? [],
  graph: {
    lane: seed.lane,
    edges: seed.parents.map((parent, parentIndex) => ({
      parent_oid: oids[parent],
      from_lane: seed.lane,
      to_lane: seed.parentLanes[parentIndex],
      merge: seed.parents.length > 1 && parentIndex > 0,
    })),
  },
}));

const changedFiles: ChangedFile[] = [
  {
    old_path: "apps/desktop/src/lib/api.ts",
    new_path: "apps/desktop/src/lib/api.ts",
    status: "modified",
    additions: 28,
    deletions: 9,
    similarity: null,
    binary: false,
  },
  {
    old_path: "apps/desktop/src/components/CommitDetails.tsx",
    new_path: "apps/desktop/src/components/CommitDetails.tsx",
    status: "modified",
    additions: 17,
    deletions: 4,
    similarity: null,
    binary: false,
  },
  {
    old_path: null,
    new_path: "apps/desktop/src/styles/diff.css",
    status: "added",
    additions: 42,
    deletions: 0,
    similarity: null,
    binary: false,
  },
];

const detailFor = (commit: CommitSummary, index: number): CommitDetails => {
  const files = index === 0 ? changedFiles : changedFiles.slice(0, (index % 3) + 1);
  const additions = files.reduce((total, file) => total + (file.additions ?? 0), 0);
  const deletions = files.reduce((total, file) => total + (file.deletions ?? 0), 0);
  return {
    oid: commit.oid,
    short_oid: commit.short_oid,
    tree_oid: `tree${commit.oid.slice(4)}`,
    parent_oids: commit.parent_oids,
    author: commit.author,
    committer: commit.author,
    authored_at: commit.authored_at,
    committed_at: commit.committed_at,
    subject: commit.subject,
    body: commitSeeds[index]?.body ?? commit.body_preview,
    stats: { files: files.length, additions, deletions },
    files,
  };
};

const initialDetails = new Map(
  initialCommits.map((commit, index) => [commit.oid, detailFor(commit, index)]),
);

const initialDiff: FileDiff = {
  old_path: "apps/desktop/src/lib/api.ts",
  new_path: "apps/desktop/src/lib/api.ts",
  old_mode: "100644",
  new_mode: "100644",
  status: "modified",
  binary: false,
  stats: { files: 1, additions: 7, deletions: 4 },
  hunks: [
    {
      header: "@@ -18,11 +18,14 @@ export async function loadRepository",
      old_start: 18,
      old_count: 11,
      new_start: 18,
      new_count: 14,
      lines: [
        { kind: "context", old_line: 18, new_line: 18, content: "  const id = activeRepository.id;" },
        { kind: "deletion", old_line: 19, new_line: null, content: "  const snapshot = await api.snapshot(id);" },
        { kind: "deletion", old_line: 20, new_line: null, content: "  const history = await api.history(id, query);" },
        { kind: "addition", old_line: null, new_line: 19, content: "  const [snapshot, history, stashes] = await Promise.all([" },
        { kind: "addition", old_line: null, new_line: 20, content: "    api.snapshot(id)," },
        { kind: "addition", old_line: null, new_line: 21, content: "    api.history(id, query)," },
        { kind: "addition", old_line: null, new_line: 22, content: "    api.stashList(id)," },
        { kind: "addition", old_line: null, new_line: 23, content: "  ]);" },
        { kind: "context", old_line: 21, new_line: 24, content: "" },
        { kind: "deletion", old_line: 22, new_line: null, content: "  return { snapshot, history };" },
        { kind: "addition", old_line: null, new_line: 25, content: "  return { snapshot, history, stashes };" },
        { kind: "context", old_line: 23, new_line: 26, content: "}" },
      ],
    },
    {
      header: "@@ -54,5 +57,6 @@ export const api = createApi();",
      old_start: 54,
      old_count: 5,
      new_start: 57,
      new_count: 6,
      lines: [
        { kind: "context", old_line: 54, new_line: 57, content: "export const api = createApi();" },
        { kind: "addition", old_line: null, new_line: 58, content: "export const runtime = api.runtime;" },
        { kind: "context", old_line: 55, new_line: 59, content: "" },
      ],
    },
  ],
  truncated: false,
};

const defaultState = (): PersistedState => ({
  settings: {
    default_pull_mode: "merge",
    auto_fetch_interval_minutes: 5,
    auto_prune: true,
    history_page_size: 200,
    diff_context_lines: 3,
    diff_max_bytes: 8 * 1024 * 1024,
    file_view_mode: "path",
    diff_view_mode: "hunk",
    graph_columns: { refs: true, graph: true, message: true, author: true, date: true, sha: true },
    graph_column_widths: { refs: 118, graph: null, message: 300, author: 86, date: 116, sha: 64 },
    forge_overrides: {},
    avatars: { enabled: true, gravatar_fallback: false },
    forge: { pull_requests: true, checks: true },
    keybinds: { ...DEFAULT_KEYBINDS },
    ui_zoom_percent: 110,
    active_theme_id: DEFAULT_THEME_ID,
    themes: cloneDefaultThemes(),
  },
  workspace: {
    version: 2,
    ungrouped_tabs: [
      {
        id: "tab-gitcat",
        repository_path: "C:\\Projects\\GitCat",
        display_name: "GitCat",
        order: 0,
        conflict_target: "origin/main",
        conflict_target_disabled: false,
      },
    ],
    groups: [],
    active_tab_id: "tab-gitcat",
  },
  recents: [
    { path: "C:\\Projects\\GitCat", name: "GitCat", opened_at: Date.now() },
  ],
  last_directory: "C:\\Projects",
});

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function delay(ms = NETWORK_DELAY_MS): Promise<void> {
  return new Promise((resolve) => globalThis.setTimeout(resolve, ms));
}

function fail(code: ApiError["code"], message: string): never {
  throw { code, message } satisfies ApiError;
}

function repositoryName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  return trimmed.split(/[\\/]/).pop() || "Repository";
}

class DemoGitCatApi implements GitCatApi {
  readonly runtime = "browser" as const;
  private readonly repositoryId: RepositoryId;
  private readonly findRepository?: (path: string) => DemoGitCatApi | undefined;
  private submodules: SubmoduleInfo[];
  private readonly committedSubmoduleOids = new Map<string, string | null>();
  private isOpen = true;
  private generationNumber = 1;
  private repositoryInfo: RepositoryInfo = {
    root: "C:\\Projects\\GitCat",
    git_dir: "C:\\Projects\\GitCat\\.git",
    common_dir: "C:\\Projects\\GitCat\\.git",
    name: "GitCat",
    is_bare: false,
    object_format: "sha1",
  };
  private commits = clone(initialCommits);
  private details = new Map(Array.from(initialDetails, ([oid, details]) => [oid, clone(details)]));
  private stashes: StashEntry[] = [
    {
      index: 0,
      oid: "a58029c097b34100885df5126d60efb32ef89289",
      message: "WIP on main: command palette polish",
    },
  ];
  private snapshotValue: RepositorySnapshot = {
    generation: "demo-1",
    head: { kind: "branch", name: "main", oid: oids[0] },
    operation_state: "normal",
    status: {
      clean: false,
      ahead: 1,
      behind: 0,
      stash_count: 1,
      entries: [
        {
          path: "apps/desktop/src/App.tsx",
          index: "modified",
          conflicted: false,
          submodule: false,
          index_stats: { additions: 34, deletions: 12 },
        },
        {
          path: "apps/desktop/src/styles.css",
          worktree: "modified",
          conflicted: false,
          submodule: false,
          worktree_stats: { additions: 18, deletions: 5 },
        },
        {
          path: "apps/desktop/src/components/CommandMenu.tsx",
          worktree: "untracked",
          conflicted: false,
          submodule: false,
          worktree_stats: { additions: 96, deletions: 0 },
        },
      ],
    },
    local_branches: [
      {
        name: "main",
        full_name: "refs/heads/main",
        oid: oids[0],
        kind: "local_branch",
        is_head: true,
        upstream: "origin/main",
        ahead: 1,
        behind: 0,
      },
      {
        name: "feature/diff-viewer",
        full_name: "refs/heads/feature/diff-viewer",
        oid: oids[5],
        kind: "local_branch",
        is_head: false,
        upstream: "origin/feature/diff-viewer",
        ahead: 0,
        behind: 0,
      },
      {
        name: "release/0.1",
        full_name: "refs/heads/release/0.1",
        oid: oids[7],
        kind: "local_branch",
        is_head: false,
      },
    ],
    remote_branches: [
      {
        name: "origin/main",
        full_name: "refs/remotes/origin/main",
        oid: oids[1],
        kind: "remote_branch",
        is_head: false,
      },
      {
        name: "origin/feature/diff-viewer",
        full_name: "refs/remotes/origin/feature/diff-viewer",
        oid: oids[5],
        kind: "remote_branch",
        is_head: false,
      },
    ],
    default_conflict_target: "origin/main",
    tags: [tagLabel("v0.1.0-preview")],
    remotes: [
      {
        name: "origin",
        fetch_url: "git@github.com:example/gitcat.git",
        push_url: "git@github.com:example/gitcat.git",
        url: {
          scheme: "scp_like",
          host: "github.com",
          path: "example/gitcat",
          owner: "example",
          repo: "gitcat",
        },
        forge: "github",
        web_url: "https://github.com/example/gitcat",
      },
    ],
    capabilities: {
      shallow: false,
      partial_clone: false,
      sparse_checkout: false,
      worktree: false,
    },
  };

  constructor(
    repositoryId = DEMO_REPOSITORY_ID,
    submodule?: SubmoduleInfo,
    findRepository?: (path: string) => DemoGitCatApi | undefined,
  ) {
    this.repositoryId = repositoryId;
    this.findRepository = findRepository;
    this.submodules = submodule ? [] : createDemoSubmodules(this.commits);
    for (const item of this.submodules) this.committedSubmoduleOids.set(item.path, item.recorded_oid);
    if (submodule) {
      this.repositoryInfo = this.infoForPath(submodule.absolute_path);
      this.snapshotValue.status.entries = [];
      this.snapshotValue.status.clean = true;
      this.snapshotValue.status.ahead = 0;
      this.snapshotValue.status.behind = 0;
      this.snapshotValue.status.stash_count = 0;
      this.stashes = [];
      const oid = submodule.head_oid ?? submodule.recorded_oid ?? oids[0];
      const branch = submodule.branch;
      this.snapshotValue.head = branch ? { kind: "branch", name: branch, oid } : { kind: "detached", oid };
      this.snapshotValue.local_branches = [{
        name: branch ?? "main",
        full_name: `refs/heads/${branch ?? "main"}`,
        oid,
        kind: "local_branch",
        is_head: Boolean(branch),
        upstream: `origin/${branch ?? "main"}`,
        ahead: 0,
        behind: 0,
      }];
      this.snapshotValue.remote_branches = [{
        name: `origin/${branch ?? "main"}`,
        full_name: `refs/remotes/origin/${branch ?? "main"}`,
        oid,
        kind: "remote_branch",
        is_head: false,
      }];
      this.snapshotValue.tags = [];
      this.snapshotValue.remotes = [{ name: "origin", fetch_url: submodule.url, push_url: submodule.url, forge: "github" }];
      // Show the selected submodule revision at the tip of its own history.
      const index = this.commits.findIndex((commit) => commit.oid === oid);
      if (index >= 0) this.commits = this.commits.slice(index);
      for (const commit of this.commits) {
        commit.decorations = commit.oid === oid && branch ? [localLabel(branch, true)] : [];
      }
    } else {
      this.snapshotValue.status.entries.push({
        path: "packages/shared", worktree: "modified", conflicted: false, submodule: true,
      });
    }
  }

  async appMetadata(): Promise<AppMetadata> {
    await delay();
    return { version: packageMetadata.version, commit: "browser-demo" };
  }

  async probe(): Promise<GitVersion> {
    await delay();
    return { major: 2, minor: 54, patch: 0, raw: "git version 2.54.0.windows.1 (demo)" };
  }

  async openRepository(path: string): Promise<OpenedRepository> {
    await delay();
    this.isOpen = true;
    this.repositoryInfo = this.infoForPath(path);
    return { repository_id: this.repositoryId, info: clone(this.repositoryInfo) };
  }

  async launchRepositoryPath(): Promise<string | null> {
    // Nothing launches the browser demo with a folder.
    return null;
  }

  async initRepository(path: string, _defaultBranch: string): Promise<OpenedRepository> {
    return this.openRepository(path);
  }

  async cloneRepository(options: CloneOptions): Promise<OpenedRepository> {
    await delay(80);
    this.isOpen = true;
    this.repositoryInfo = this.infoForPath(options.destination);
    return { repository_id: this.repositoryId, info: clone(this.repositoryInfo) };
  }

  async closeRepository(repositoryId: RepositoryId): Promise<void> {
    await delay();
    this.ensureRepository(repositoryId);
    this.isOpen = false;
  }

  async watchRepository(_repositoryId: RepositoryId): Promise<void> {
    // The browser demo has no filesystem to watch.
  }

  async unwatchRepository(): Promise<void> {
    // The browser demo has no filesystem to watch.
  }

  async openRepositoryFolder(_repositoryId: RepositoryId, _path?: string): Promise<void> {
    // The browser demo has no filesystem to reveal.
  }

  /** Used by the demo router when a submodule is opened in its own tab. */
  submoduleForRoot(path: string): SubmoduleInfo | undefined {
    return this.currentSubmodules().find((submodule) => demoRepositoryKey(submodule.absolute_path) === demoRepositoryKey(path));
  }

  async listSubmodules(repositoryId: RepositoryId): Promise<SubmoduleInfo[]> {
    await delay();
    this.ensureRepository(repositoryId);
    return clone(this.currentSubmodules());
  }

  async addSubmodule(repositoryId: RepositoryId, url: string, path: string): Promise<MutationResult> {
    await delay(120);
    this.ensureRepository(repositoryId);
    const normalized = path.trim().replace(/\\/g, "/");
    if (!normalized || normalized.startsWith("/") || normalized.includes(":")
      || normalized.split("/").some((part) => !part || part === "." || part === ".." || part.toLowerCase() === ".git")) {
      fail("invalid_path", "Choose a relative folder inside this repository");
    }
    if (this.submodules.some((submodule) => {
      const existing = submodule.path.toLowerCase();
      const candidate = normalized.toLowerCase();
      return existing === candidate || existing.startsWith(`${candidate}/`) || candidate.startsWith(`${existing}/`);
    })) fail("invalid_path", "A submodule already occupies this folder");
    this.validateSubmoduleUrl(url);
    const commit = this.commits[0];
    this.submodules.push({
      name: normalized, path: normalized, url: url.trim(), absolute_path: demoSubmodulePath(this.repositoryInfo.root, normalized),
      initialized: true, head_oid: commit.oid, recorded_oid: commit.oid,
      head_summary: commit.subject, head_committed_at: commit.committed_at.seconds,
      branch: "main", dirty: false, conflicted: false, head_matches_recorded: true,
    });
    this.stageSubmoduleMetadata();
    this.snapshotValue.status.entries.push({ path: normalized, index: "added", conflicted: false, submodule: true });
    return this.mutation();
  }

  async setSubmoduleUrl(repositoryId: RepositoryId, path: string, url: string): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const submodule = this.requireSubmodule(path);
    this.validateSubmoduleUrl(url);
    submodule.url = url.trim();
    const child = this.findRepository?.(demoSubmodulePath(this.repositoryInfo.root, path));
    if (child) child.snapshotValue.remotes = [{ name: "origin", fetch_url: submodule.url, push_url: submodule.url, forge: "unknown" }];
    this.stageSubmoduleMetadata();
    return this.mutation();
  }

  async updateSubmodule(repositoryId: RepositoryId, path: string): Promise<MutationResult> {
    await delay(140);
    this.ensureRepository(repositoryId);
    const submodule = this.requireSubmodule(path);
    const current = this.currentSubmodules().find((item) => item.path === path)!;
    if (current.dirty || current.conflicted) fail("invalid_request", "Commit or stash the submodule changes before updating");
    const child = this.findRepository?.(current.absolute_path);
    // Commits created in a child tab are local work; a demo update must not
    // silently replace them with the sample remote history.
    if (current.head_oid && !initialDetails.has(current.head_oid)) {
      fail("invalid_request", "Submodule has local commits; open it to review before updating");
    }
    const oid = current.initialized ? oids[0] : current.recorded_oid ?? oids[0];
    const commit = initialCommits.find((item) => item.oid === oid)!;
    submodule.initialized = true;
    submodule.head_oid = oid;
    submodule.head_summary = commit.subject;
    submodule.head_committed_at = commit.committed_at.seconds;
    submodule.head_matches_recorded = oid === submodule.recorded_oid;
    if (child) child.receiveSubmoduleUpdate(oid);
    this.refreshSubmoduleStatus();
    const result = this.mutation();
    if (!current.initialized) result.notice = "Initialized at the commit recorded by the parent repository.";
    return result;
  }

  async removeSubmodule(repositoryId: RepositoryId, path: string, confirmed: boolean): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    this.requireSubmodule(path);
    if (!confirmed) fail("invalid_request", "Confirm removal before deleting a submodule");
    const current = this.currentSubmodules().find((item) => item.path === path)!;
    if (current.dirty || current.conflicted) fail("invalid_request", "Commit or stash the submodule changes before removing it");
    this.submodules = this.submodules.filter((submodule) => submodule.path !== path);
    this.snapshotValue.status.entries = this.snapshotValue.status.entries.filter((entry) => entry.path !== path);
    this.snapshotValue.status.entries.push({ path, index: "deleted", conflicted: false, submodule: true });
    this.stageSubmoduleMetadata();
    return this.mutation();
  }

  async syncSubmodule(repositoryId: RepositoryId, path: string, action: SubmoduleSyncAction): Promise<MutationResult> {
    if (action === "pull") return this.updateSubmodule(repositoryId, path);
    await delay(110);
    this.ensureRepository(repositoryId);
    this.requireSubmodule(path);
    const current = this.currentSubmodules().find((item) => item.path === path)!;
    if (!current.initialized) fail("invalid_request", "Initialize this submodule before pushing");
    if (!current.branch) fail("invalid_request", "Submodule has a detached HEAD; check out a branch before pushing");
    const child = this.findRepository?.(current.absolute_path);
    if (child) child.snapshotValue.status.ahead = 0;
    return this.mutation();
  }

  async snapshot(repositoryId: RepositoryId): Promise<RepositorySnapshot> {
    await delay();
    this.ensureRepository(repositoryId);
    this.refreshSubmoduleStatus();
    return clone(this.snapshotValue);
  }

  async history(repositoryId: RepositoryId, query: HistoryQuery): Promise<HistoryPage> {
    await delay();
    this.ensureRepository(repositoryId);
    const scoped = this.commitsForScope(query);
    const offset = query.cursor?.offset ?? 0;
    const commits = scoped.slice(offset, offset + query.limit);
    const nextOffset = offset + commits.length;
    return clone({
      generation: this.snapshotValue.generation,
      commits,
      next_cursor:
        nextOffset < scoped.length
          ? {
              generation: this.snapshotValue.generation,
              offset: nextOffset,
              lanes: { heads: [] },
            }
          : null,
      has_more: nextOffset < scoped.length,
    });
  }

  async searchCommits(
    repositoryId: RepositoryId,
    query: CommitSearchQuery,
  ): Promise<CommitSearchResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const needle = query.query.trim().toLocaleLowerCase();
    if (!needle) return { total: 0, truncated: false, hits: [] };
    const matches = this.commitsForScope(query).flatMap((commit) => {
      const details = this.details.get(commit.oid);
      const subjectMatch = commit.subject.toLocaleLowerCase().includes(needle);
      const body = details?.body ?? commit.body_preview;
      const bodyMatch = body.toLocaleLowerCase().includes(needle);
      return subjectMatch || bodyMatch
        ? [
            {
              oid: commit.oid,
              subject: commit.subject,
              body_excerpt: bodyMatch ? body.split("\n")[0] : null,
              matched_subject: subjectMatch,
              matched_body: bodyMatch,
            },
          ]
        : [];
    });
    return clone({
      total: matches.length,
      truncated: matches.length > query.limit,
      hits: matches.slice(0, query.limit),
    });
  }

  async commitDetails(
    repositoryId: RepositoryId,
    oid: string,
    _parentIndex = 0,
  ): Promise<CommitDetails> {
    await delay();
    this.ensureRepository(repositoryId);
    const details = this.details.get(oid);
    if (!details) fail("invalid_revision", `Unknown demo commit: ${oid}`);
    return clone(details);
  }

  async diff(repositoryId: RepositoryId, request: DiffRequest): Promise<FileDiff> {
    await delay();
    this.ensureRepository(repositoryId);
    const result = clone(initialDiff);
    result.old_path = request.path;
    result.new_path = request.path;
    result.truncated = request.max_bytes < 1_024;
    return result;
  }

  async conflictPreflight(
    repositoryId: RepositoryId,
    target: string,
  ): Promise<ConflictPreflightResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const targetBranch = [...this.snapshotValue.local_branches, ...this.snapshotValue.remote_branches]
      .find((branch) => branch.name === target);
    if (!targetBranch) fail("invalid_revision", `Unknown demo conflict target: ${target}`);
    return {
      target,
      target_oid: targetBranch.oid,
      state: "clean",
      conflicting_paths: [],
    };
  }

  async stagePaths(repositoryId: RepositoryId, paths: string[]): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    for (const path of paths) {
      const entry = this.snapshotValue.status.entries.find((candidate) => candidate.path === path);
      if (entry) {
        entry.index = entry.worktree === "untracked" ? "added" : (entry.worktree ?? entry.index ?? "modified");
        delete entry.worktree;
        const submodule = this.submodules.find((item) => item.path === path);
        if (submodule) submodule.recorded_oid = this.currentSubmodules().find((item) => item.path === path)!.head_oid;
      }
    }
    return this.mutation();
  }

  async unstagePaths(repositoryId: RepositoryId, paths: string[]): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    for (const path of paths) {
      const entry = this.snapshotValue.status.entries.find((candidate) => candidate.path === path);
      if (entry?.index) {
        entry.worktree = entry.index === "added" ? "untracked" : entry.index;
        delete entry.index;
        const submodule = this.submodules.find((item) => item.path === path);
        if (submodule) submodule.recorded_oid = this.committedSubmoduleOids.get(path) ?? null;
      }
    }
    return this.mutation();
  }

  async discardPaths(repositoryId: RepositoryId, paths: string[]): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const targets = new Set(paths);
    this.snapshotValue.status.entries = this.snapshotValue.status.entries.filter(
      (candidate) => !targets.has(candidate.path),
    );
    return this.mutation();
  }

  // The demo keeps no file contents, so a line-level change only refreshes.
  async applyDiffLines(repositoryId: RepositoryId, _request: LinePatchRequest): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    return this.mutation();
  }

  async stashFile(
    repositoryId: RepositoryId,
    paths: string[],
    _message: string | null,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const targets = new Set(paths);
    this.snapshotValue.status.entries = this.snapshotValue.status.entries.filter(
      (candidate) => !targets.has(candidate.path),
    );
    this.snapshotValue.status.stash_count += 1;
    return this.mutation();
  }

  async appendGitignore(repositoryId: RepositoryId, _patterns: string[]): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    return this.mutation();
  }

  async savePatch(
    repositoryId: RepositoryId,
    _paths: string[],
    _staged: boolean,
    _destination: string,
  ): Promise<void> {
    await delay();
    this.ensureRepository(repositoryId);
  }

  async resolveConflict(
    repositoryId: RepositoryId,
    path: string,
    resolution: ConflictResolution,
    _expectedState: ConflictExpectedState,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const entry = this.snapshotValue.status.entries.find((candidate) => candidate.path === path);
    if (!entry?.conflicted) fail("invalid_request", "Selected file is not conflicted");
    if (resolution === "delete") {
      this.snapshotValue.status.entries = this.snapshotValue.status.entries.filter((candidate) => candidate.path !== path);
    } else {
      entry.conflicted = false;
      entry.index = "modified";
      delete entry.worktree;
    }
    return this.mutation();
  }

  async conflictDetails(repositoryId: RepositoryId, path: string): Promise<ConflictFileDetails> {
    await delay();
    this.ensureRepository(repositoryId);
    const entry = this.snapshotValue.status.entries.find((candidate) => candidate.path === path);
    if (!entry?.conflicted) fail("invalid_request", "Selected file is not conflicted");
    const base = { oid: oids[7], mode: "100644" };
    const ours = { oid: oids[8], mode: "100644" };
    const theirs = { oid: oids[9], mode: "100644" };
    return {
      path,
      expected_state: {
        base,
        ours,
        theirs,
        result: { kind: "regular", size: 78, sha256: "0".repeat(64), line_ending: "lf", mode: 0o644 },
      },
      base: { ...base, content: { kind: "text", size: 13, text: "shared base\n", line_ending: "lf" } },
      ours: { ...ours, content: { kind: "text", size: 16, text: "current version\n", line_ending: "lf" } },
      theirs: { ...theirs, content: { kind: "text", size: 17, text: "incoming version\n", line_ending: "lf" } },
      result: {
        kind: "text",
        size: 78,
        text: "<<<<<<< current\ncurrent version\n=======\nincoming version\n>>>>>>> incoming\n",
        line_ending: "lf",
      },
    };
  }

  async saveConflictResult(
    repositoryId: RepositoryId,
    path: string,
    _text: string,
    _lineEnding: ConflictLineEndingPolicy,
    expectedState: ConflictExpectedState,
  ): Promise<MutationResult> {
    return this.resolveConflict(repositoryId, path, "mark_resolved", expectedState);
  }

  async resolveConflicts(
    repositoryId: RepositoryId,
    paths: string[],
    resolution: ConflictResolution,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const conflicted = this.snapshotValue.status.entries.filter((entry) => entry.conflicted);
    const targets = paths.length
      ? conflicted.filter((entry) => paths.includes(entry.path))
      : conflicted;
    if (paths.length && targets.length !== new Set(paths).size) {
      fail("invalid_request", "Selected paths are not currently conflicted");
    }
    const resolved = new Set(targets.map((entry) => entry.path));
    if (resolution === "delete") {
      this.snapshotValue.status.entries = this.snapshotValue.status.entries.filter(
        (entry) => !resolved.has(entry.path),
      );
    } else {
      for (const entry of targets) {
        entry.conflicted = false;
        entry.index = "modified";
        delete entry.worktree;
      }
    }
    return this.mutation();
  }

  async autoResolveConflicts(repositoryId: RepositoryId): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    return this.mutation();
  }

  async createCommit(
    repositoryId: RepositoryId,
    options: CommitOptions,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    if (!options.message.trim()) fail("invalid_request", "Commit message cannot be empty");
    const before = this.headOid();
    const oid = this.demoOid(this.commits.length + this.generationNumber);
    const [subject, ...bodyLines] = options.message.split("\n");
    const summary: CommitSummary = {
      oid,
      short_oid: oid.slice(0, 7),
      parent_oids: before ? [before] : [],
      subject,
      body_preview: bodyLines.join(" ").trim(),
      author: ris,
      authored_at: { seconds: Math.floor(Date.now() / 1000), offset_minutes: 120 },
      committed_at: { seconds: Math.floor(Date.now() / 1000), offset_minutes: 120 },
      decorations: [localLabel(this.currentBranchName() ?? "main", true)],
      graph: {
        lane: 0,
        edges: before
          ? [{ parent_oid: before, from_lane: 0, to_lane: 0, merge: false }]
          : [],
      },
    };
    for (const commit of this.commits) {
      commit.decorations = commit.decorations.filter((label) => !label.is_head);
    }
    this.commits.unshift(summary);
    this.details.set(oid, {
      oid,
      short_oid: oid.slice(0, 7),
      tree_oid: this.demoOid(this.commits.length + 100),
      parent_oids: summary.parent_oids,
      author: ris,
      committer: ris,
      authored_at: summary.authored_at,
      committed_at: summary.committed_at,
      subject,
      body: bodyLines.join("\n").trim(),
      stats: { files: this.snapshotValue.status.entries.length, additions: 21, deletions: 5 },
      files: clone(changedFiles),
    });
    this.setHeadOid(oid);
    for (const entry of this.snapshotValue.status.entries) {
      if (!entry.submodule || !entry.index) continue;
      const submodule = this.submodules.find((item) => item.path === entry.path);
      if (submodule) this.committedSubmoduleOids.set(entry.path, submodule.recorded_oid);
      else this.committedSubmoduleOids.delete(entry.path);
    }
    this.snapshotValue.status.entries = [];
    this.snapshotValue.status.clean = true;
    return this.mutation(before, oid);
  }

  // The demo repository always has history, so the empty-repository offer
  // never appears here; it commits like any other so the API stays whole.
  async createInitialCommit(
    repositoryId: RepositoryId,
    message: string,
  ): Promise<MutationResult> {
    return this.createCommit(repositoryId, { message, amend: false, signoff: false });
  }

  async createBranch(
    repositoryId: RepositoryId,
    name: string,
    startOid: string,
    checkout: boolean,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const before = this.headOid();
    if (this.snapshotValue.local_branches.some((branch) => branch.name === name)) {
      fail("invalid_ref_name", `Branch already exists: ${name}`);
    }
    const branch: BranchInfo = {
      name,
      full_name: `refs/heads/${name}`,
      oid: startOid,
      kind: "local_branch",
      is_head: false,
    };
    this.snapshotValue.local_branches.push(branch);
    if (checkout) this.checkoutBranchValue(name);
    return this.mutation(before, this.headOid());
  }

  async checkoutBranch(repositoryId: RepositoryId, name: string): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const before = this.headOid();
    this.checkoutBranchValue(name);
    return this.mutation(before, this.headOid());
  }

  async renameBranch(
    repositoryId: RepositoryId,
    oldName: string,
    newName: string,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const branch = this.snapshotValue.local_branches.find((candidate) => candidate.name === oldName);
    if (!branch) fail("invalid_ref_name", `Unknown branch: ${oldName}`);
    branch.name = newName;
    branch.full_name = `refs/heads/${newName}`;
    if (this.snapshotValue.head.kind === "branch" && this.snapshotValue.head.name === oldName) {
      this.snapshotValue.head.name = newName;
    }
    return this.mutation();
  }

  async deleteBranch(
    repositoryId: RepositoryId,
    name: string,
    force: boolean,
    confirmed: boolean,
    expected: ExpectedState,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    this.verifyExpected(expected);
    if (force && !confirmed) fail("protected_operation", "Forced delete requires confirmation");
    if (this.currentBranchName() === name) fail("protected_operation", "Cannot delete checked out branch");
    this.snapshotValue.local_branches = this.snapshotValue.local_branches.filter(
      (branch) => branch.name !== name,
    );
    return this.mutation();
  }

  async setUpstream(
    repositoryId: RepositoryId,
    branchName: string,
    upstream: string,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const branch = this.snapshotValue.local_branches.find(({ name }) => name === branchName);
    if (!branch) fail("invalid_ref_name", `Unknown branch: ${branchName}`);
    branch.upstream = upstream;
    branch.ahead = 0;
    branch.behind = 0;
    return this.mutation();
  }

  async mergeBranch(repositoryId: RepositoryId, _branch: string): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    return this.mutation();
  }

  async addRemote(
    repositoryId: RepositoryId,
    name: string,
    url: string,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    this.snapshotValue.remotes = [
      ...this.snapshotValue.remotes,
      { name, fetch_url: url, push_url: url, forge: "unknown" },
    ];
    return this.mutation();
  }

  async fetch(repositoryId: RepositoryId, _options: FetchOptions): Promise<MutationResult> {
    await delay(90);
    this.ensureRepository(repositoryId);
    return this.mutation();
  }

  async pull(
    repositoryId: RepositoryId,
    options: PullOptions,
    onProgress?: (phase: PullPhase) => void,
  ): Promise<MutationResult> {
    if (options.autostash) {
      onProgress?.({ phase: "stashing" });
      await delay(60);
    }
    onProgress?.({ phase: "pulling" });
    await delay(110);
    if (options.mode === "rebase") {
      onProgress?.({ phase: "rebasing", current: 1, total: 1 });
      await delay(80);
    }
    if (options.autostash) onProgress?.({ phase: "restoring" });
    this.ensureRepository(repositoryId);
    this.snapshotValue.status.behind = 0;
    return this.mutation();
  }

  async push(repositoryId: RepositoryId, _options: PushOptions): Promise<MutationResult> {
    await delay(100);
    this.ensureRepository(repositoryId);
    this.snapshotValue.status.ahead = 0;
    return this.mutation();
  }

  async checkoutCommit(repositoryId: RepositoryId, oid: string): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    if (!this.details.has(oid)) fail("invalid_revision", `Unknown demo commit: ${oid}`);
    const before = this.headOid();
    this.snapshotValue.head = { kind: "detached", oid };
    for (const branch of this.snapshotValue.local_branches) branch.is_head = false;
    return this.mutation(before, oid);
  }

  async createTag(
    repositoryId: RepositoryId,
    name: string,
    oid: string,
    _message: string | null,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const label = tagLabel(name);
    this.snapshotValue.tags.push(label);
    const commit = this.commits.find((candidate) => candidate.oid === oid);
    if (commit) commit.decorations.push(label);
    return this.mutation();
  }

  async cherryPick(
    repositoryId: RepositoryId,
    _oid: string,
    _mainlineParent: number | null = null,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    return this.mutation();
  }

  async revertCommit(
    repositoryId: RepositoryId,
    _oid: string,
    _mainlineParent: number | null = null,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    return this.mutation();
  }

  async resetCommit(
    repositoryId: RepositoryId,
    oid: string,
    mode: ResetMode,
    confirmed: boolean,
    expected: ExpectedState,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    this.verifyExpected(expected);
    if (mode === "hard" && !confirmed) fail("protected_operation", "Hard reset requires confirmation");
    const before = this.headOid();
    this.setHeadOid(oid);
    if (mode === "hard") {
      this.snapshotValue.status.entries = [];
      this.snapshotValue.status.clean = true;
    }
    return this.mutation(before, oid);
  }

  async rewordCommit(
    repositoryId: RepositoryId,
    oid: string,
    message: string,
    expected: ExpectedState,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    this.verifyExpected(expected);
    if (!message.trim()) fail("invalid_settings", "Commit message is empty");
    const detail = this.details.get(oid);
    if (!detail) fail("invalid_revision", `Unknown demo commit: ${oid}`);
    const before = this.headOid();
    const [subject, ...bodyLines] = message.split("\n");
    detail.subject = subject;
    detail.body = bodyLines.join("\n").trim();
    const summary = this.commits.find((candidate) => candidate.oid === oid);
    if (summary) {
      summary.subject = subject;
      summary.body_preview = bodyLines.join(" ").trim();
    }
    return this.mutation(before, before);
  }

  async commitActionAvailability(
    repositoryId: RepositoryId,
    oid: string,
  ): Promise<CommitActionAvailability[]> {
    await delay();
    this.ensureRepository(repositoryId);
    if (!this.details.has(oid)) fail("invalid_revision", `Unknown demo commit: ${oid}`);
    const dirty = !this.snapshotValue.status.clean;
    const operationBusy = this.snapshotValue.operation_state !== "normal";
    const resetUnavailable = this.snapshotValue.head.kind !== "branch";
    const headOid = this.headOid();
    const targetIndex = this.commits.findIndex((commit) => commit.oid === oid);
    const headIndex = headOid ? this.commits.findIndex((commit) => commit.oid === headOid) : -1;
    const targetInHeadHistory = headIndex >= 0 && targetIndex >= headIndex;
    const action = (
      kind: CommitActionAvailability["kind"],
      requiresClean: boolean,
      requiresConfirmation: boolean,
      unavailable: string | null = null,
    ): CommitActionAvailability => {
      const disabledReason = operationBusy
        ? "Finish or abort the current Git operation first"
        : kind === "reset" && resetUnavailable
          ? "Check out a local branch before resetting"
          : unavailable
            ? unavailable
            : requiresClean && dirty
              ? "Working tree must be clean for this action"
              : null;
      return {
        kind,
        enabled: disabledReason === null,
        disabled_reason: disabledReason,
        requires_confirmation: requiresConfirmation,
      };
    };
    const cherryPickUnavailable = !headOid
      ? "Check out a commit before cherry-picking"
      : targetInHeadHistory
        ? "Commit is already in the current HEAD history"
        : null;
    const revertUnavailable = !headOid
      ? "Check out a commit before reverting"
      : targetInHeadHistory
        ? null
        : "Commit is not in the current HEAD history";
    const rewordUnavailable = !headOid
      ? "Check out a commit before editing its message"
      : targetInHeadHistory
        ? null
        : "Only commits reachable from the current branch can be edited";
    return [
      action("checkout", true, false),
      action("create_branch", false, false),
      action("cherry_pick", true, false, cherryPickUnavailable),
      action("revert", true, true, revertUnavailable),
      action("reset", false, true),
      action("create_tag", false, false),
      action("reword", false, false, rewordUnavailable),
      { kind: "copy_sha", enabled: true, disabled_reason: null, requires_confirmation: false },
    ];
  }

  async continueOperation(
    repositoryId: RepositoryId,
    _operation: ContinueOperation,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    this.snapshotValue.operation_state = "normal";
    return this.mutation();
  }

  async abortOperation(
    repositoryId: RepositoryId,
    _operation: ContinueOperation,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    this.snapshotValue.operation_state = "normal";
    return this.mutation();
  }

  async skipOperation(
    repositoryId: RepositoryId,
    operation: ContinueOperation,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    if (operation === "merge") fail("unsupported_operation", "A merge cannot skip a commit");
    this.snapshotValue.status.entries = this.snapshotValue.status.entries.filter(
      (entry) => !entry.conflicted,
    );
    this.snapshotValue.operation_state = "normal";
    return this.mutation();
  }

  async stashList(repositoryId: RepositoryId): Promise<StashEntry[]> {
    await delay();
    this.ensureRepository(repositoryId);
    return clone(this.stashes);
  }

  async stashPush(
    repositoryId: RepositoryId,
    message: string | null,
    _includeUntracked: boolean,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    this.stashes.unshift({
      index: 0,
      oid: this.demoOid(this.generationNumber + 500),
      message: message || `WIP on ${this.currentBranchName() ?? "detached HEAD"}`,
    });
    this.reindexStashes();
    this.snapshotValue.status.entries = [];
    this.snapshotValue.status.clean = true;
    this.snapshotValue.status.stash_count = this.stashes.length;
    return this.mutation();
  }

  async stashApply(
    repositoryId: RepositoryId,
    oid: string,
    pop: boolean,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    const index = this.stashes.findIndex((stash) => stash.oid === oid);
    if (index === -1) fail("invalid_revision", `Selected stash no longer exists: ${oid}`);
    const restored: StatusEntry = {
      path: "apps/desktop/src/App.tsx",
      worktree: "modified",
      conflicted: false,
      submodule: false,
      worktree_stats: { additions: 21, deletions: 5 },
    };
    this.snapshotValue.status.entries = [restored];
    this.snapshotValue.status.clean = false;
    if (pop) this.stashes.splice(index, 1);
    this.reindexStashes();
    this.snapshotValue.status.stash_count = this.stashes.length;
    return this.mutation();
  }

  async stashDrop(
    repositoryId: RepositoryId,
    oid: string,
    confirmed: boolean,
    expected: ExpectedState,
  ): Promise<MutationResult> {
    await delay();
    this.ensureRepository(repositoryId);
    this.verifyExpected(expected);
    if (!confirmed) fail("protected_operation", "Dropping a stash requires confirmation");
    const index = this.stashes.findIndex((stash) => stash.oid === oid);
    if (index === -1) fail("invalid_revision", `Selected stash no longer exists: ${oid}`);
    this.stashes.splice(index, 1);
    this.reindexStashes();
    this.snapshotValue.status.stash_count = this.stashes.length;
    return this.mutation();
  }

  async loadPersistedState(): Promise<PersistedState> {
    await delay();
    if (typeof window === "undefined") return defaultState();
    try {
      const stored = window.localStorage.getItem(STORAGE_KEY);
      return stored ? (JSON.parse(stored) as PersistedState) : defaultState();
    } catch {
      return defaultState();
    }
  }

  async savePersistedState(state: PersistedState): Promise<void> {
    await delay();
    if (typeof window === "undefined") return;
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch {
      // Private browsing/storage denial should not make browser preview unusable.
    }
  }

  async loadRepositoryOverview(
    repositoryId: RepositoryId,
    query: HistoryQuery = { scope: { kind: "all_refs" }, cursor: null, limit: 200 },
  ): Promise<RepositoryOverview> {
    const [snapshot, history, stashes] = await Promise.all([
      this.snapshot(repositoryId),
      this.history(repositoryId, query),
      this.stashList(repositoryId),
    ]);
    return { snapshot, history, stashes };
  }

  async loadCommitPanel(
    repositoryId: RepositoryId,
    oid: string,
    parentIndex = 0,
  ): Promise<CommitPanelData> {
    const [details, actions] = await Promise.all([
      this.commitDetails(repositoryId, oid, parentIndex),
      this.commitActionAvailability(repositoryId, oid),
    ]);
    return { details, actions };
  }

  private infoForPath(path: string): RepositoryInfo {
    const root = path || "C:\\Projects\\GitCat";
    const separator = root.includes("\\") ? "\\" : "/";
    return {
      root,
      git_dir: `${root}${separator}.git`,
      common_dir: `${root}${separator}.git`,
      name: repositoryName(root),
      is_bare: false,
      object_format: "sha1",
    };
  }

  private currentSubmodules(): SubmoduleInfo[] {
    return this.submodules.map((submodule) => {
      const absolute_path = demoSubmodulePath(this.repositoryInfo.root, submodule.path);
      const child = this.findRepository?.(absolute_path);
      if (!child) return {
        ...submodule,
        absolute_path,
        head_matches_recorded: submodule.initialized && submodule.head_oid === submodule.recorded_oid,
      };
      const head_oid = child.headOid();
      const commit = head_oid ? child.details.get(head_oid) : undefined;
      return {
        ...submodule,
        absolute_path,
        head_oid,
        branch: child.currentBranchName(),
        head_summary: commit?.subject ?? null,
        head_committed_at: commit?.committed_at.seconds ?? null,
        dirty: child.snapshotValue.status.entries.length > 0,
        conflicted: child.snapshotValue.status.entries.some((entry) => entry.conflicted),
        head_matches_recorded: head_oid === submodule.recorded_oid,
      };
    });
  }

  private requireSubmodule(path: string): SubmoduleInfo {
    const submodule = this.submodules.find((item) => item.path === path);
    if (!submodule) fail("invalid_path", `Unknown submodule: ${path}`);
    return submodule;
  }

  private validateSubmoduleUrl(url: string): void {
    if (!url.trim() || url.trim().startsWith("-") || /[\r\n\0]/.test(url)) {
      fail("invalid_request", "Enter a repository URL");
    }
  }

  private stageSubmoduleMetadata(): void {
    const existing = this.snapshotValue.status.entries.find((entry) => entry.path === ".gitmodules");
    if (existing) existing.index = "modified";
    else this.snapshotValue.status.entries.push({ path: ".gitmodules", index: "modified", conflicted: false, submodule: false });
  }

  private refreshSubmoduleStatus(): void {
    for (const submodule of this.currentSubmodules()) {
      const entry = this.snapshotValue.status.entries.find((item) => item.path === submodule.path);
      if (submodule.initialized && (!submodule.head_matches_recorded || submodule.dirty)) {
        if (entry) entry.worktree = "modified";
        else this.snapshotValue.status.entries.push({ path: submodule.path, worktree: "modified", conflicted: submodule.conflicted, submodule: true });
      } else if (entry) {
        delete entry.worktree;
        if (!entry.index) this.snapshotValue.status.entries = this.snapshotValue.status.entries.filter((item) => item !== entry);
      }
    }
    this.snapshotValue.status.clean = this.snapshotValue.status.entries.length === 0;
  }

  private receiveSubmoduleUpdate(oid: string): void {
    this.setHeadOid(oid);
    const index = initialCommits.findIndex((commit) => commit.oid === oid);
    this.commits = clone(initialCommits.slice(Math.max(index, 0)));
    const branch = this.currentBranchName();
    for (const commit of this.commits) {
      commit.decorations = commit.oid === oid && branch ? [localLabel(branch, true)] : [];
    }
    for (const remote of this.snapshotValue.remote_branches) remote.oid = oid;
    this.snapshotValue.status.behind = 0;
    this.mutation();
  }

  private ensureRepository(repositoryId: RepositoryId): void {
    if (!this.isOpen || repositoryId !== this.repositoryId) {
      fail("repository_closed", "Demo repository is not open");
    }
  }

  private commitsForScope(query: Pick<HistoryQuery, "scope">): CommitSummary[] {
    if (query.scope.kind !== "ref") return this.commits;
    const scopeValue = query.scope.value;
    const branch = [...this.snapshotValue.local_branches, ...this.snapshotValue.remote_branches].find(
      ({ name, full_name }) => name === scopeValue || full_name === scopeValue,
    );
    const start = branch ? this.commits.findIndex(({ oid }) => oid === branch.oid) : -1;
    return start >= 0 ? this.commits.slice(start) : [];
  }

  private currentBranchName(): string | null {
    return this.snapshotValue.head.kind === "branch" ? this.snapshotValue.head.name : null;
  }

  private headOid(): string | null {
    return this.snapshotValue.head.kind === "unborn" ? null : this.snapshotValue.head.oid;
  }

  private setHeadOid(oid: string): void {
    if (this.snapshotValue.head.kind === "branch") {
      this.snapshotValue.head.oid = oid;
      const branch = this.snapshotValue.local_branches.find(({ is_head }) => is_head);
      if (branch) branch.oid = oid;
    } else {
      this.snapshotValue.head = { kind: "detached", oid };
    }
  }

  private checkoutBranchValue(name: string): void {
    const branch = this.snapshotValue.local_branches.find((candidate) => candidate.name === name);
    if (!branch) fail("invalid_ref_name", `Unknown branch: ${name}`);
    for (const candidate of this.snapshotValue.local_branches) candidate.is_head = false;
    branch.is_head = true;
    this.snapshotValue.head = { kind: "branch", name: branch.name, oid: branch.oid };
  }

  private verifyExpected(expected: ExpectedState): void {
    if (
      expected.generation !== this.snapshotValue.generation ||
      expected.head_oid !== this.headOid()
    ) {
      fail("stale_snapshot", "Repository changed since confirmation opened");
    }
  }

  private mutation(before = this.headOid(), after = this.headOid()): MutationResult {
    this.generationNumber += 1;
    this.snapshotValue.generation = `demo-${this.generationNumber}`;
    this.snapshotValue.status.clean = this.snapshotValue.status.entries.length === 0;
    return {
      before_oid: before,
      after_oid: after,
      generation: this.snapshotValue.generation,
      conflicts: this.snapshotValue.status.entries.filter(({ conflicted }) => conflicted),
      needs_user_action: this.snapshotValue.operation_state !== "normal"
        || this.snapshotValue.status.entries.some(({ conflicted }) => conflicted),
    };
  }

  private reindexStashes(): void {
    this.stashes.forEach((stash, index) => {
      stash.index = index;
    });
  }

  private demoOid(seed: number): string {
    const prefix = Math.abs(seed).toString(16).padStart(8, "0");
    return `${prefix}${"cafe42d19b7e5a03".repeat(2)}`.slice(0, 40);
  }
}

export function createDemoGitCatApi(): GitCatApi {
  const byRoot = new Map<string, DemoGitCatApi>();
  const byId = new Map<RepositoryId, DemoGitCatApi>();
  const findRepository = (path: string) => byRoot.get(demoRepositoryKey(path));
  const primary = new DemoGitCatApi(DEMO_REPOSITORY_ID, undefined, findRepository);
  byId.set(DEMO_REPOSITORY_ID, primary);
  let primaryBound = false;

  const repositoryForOpen = (path: string): DemoGitCatApi => {
    const key = demoRepositoryKey(path || "C:\\Projects\\GitCat");
    const existing = byRoot.get(key);
    if (existing) return existing;
    const submodule = Array.from(byId.values())
      .map((repository) => repository.submoduleForRoot(path))
      .find((candidate) => candidate !== undefined);
    if (submodule && !submodule.initialized) fail("invalid_request", "Initialize this submodule before opening it");
    const id = !primaryBound && !submodule ? DEMO_REPOSITORY_ID : crypto.randomUUID();
    const repository = id === DEMO_REPOSITORY_ID
      ? primary
      : new DemoGitCatApi(id, submodule, findRepository);
    if (repository === primary) primaryBound = true;
    // Register before awaiting the open, because workspace restoration opens
    // several saved tabs concurrently.
    byRoot.set(key, repository);
    byId.set(id, repository);
    return repository;
  };

  // Keep the existing per-repository demo implementation self-contained. Only
  // lifecycle methods choose a repository; all repository commands already
  // carry their handle as the first argument and are routed by that handle.
  return new Proxy(primary, {
    get(target, property: string | symbol) {
      if (property === "openRepository") return async (path: string) => repositoryForOpen(path).openRepository(path);
      if (property === "initRepository") return async (path: string, branch: string) => repositoryForOpen(path).initRepository(path, branch);
      if (property === "cloneRepository") return async (options: CloneOptions) => repositoryForOpen(options.destination).cloneRepository(options);
      const value: unknown = Reflect.get(target, property);
      if (typeof value !== "function") return value;
      return (...args: unknown[]) => {
        const repository = typeof args[0] === "string" ? byId.get(args[0]) ?? target : target;
        return Reflect.apply(Reflect.get(repository, property), repository, args);
      };
    },
  });
}

export { DEMO_REPOSITORY_ID };
