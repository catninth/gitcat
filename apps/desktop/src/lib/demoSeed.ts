// The repository the in-browser demo starts with: its commits, refs, one
// commit's files and diff, and the settings and workspace a first run sees.

import { cloneDefaultThemes, DEFAULT_THEME_ID } from "../app/themePresets";
import { DEFAULT_KEYBINDS } from "./keybinds";
import type {
  ChangedFile,
  CommitDetails,
  CommitSummary,
  FileDiff,
  Identity,
  PersistedState,
  RefLabel,
} from "./types";

export const DEMO_REPOSITORY_ID = "7e43a7bc-d48b-4f8f-92ab-43a41676ce7e";
export const STORAGE_KEY = "gitcat.demo.persisted-state.v1";
export const NETWORK_DELAY_MS = 24;

export const oids = [
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

export const ris: Identity = { name: "Ris", email: "ris@example.com" };
const lina: Identity = { name: "Lina Kovács", email: "lina@example.com" };

export const localLabel = (name: string, isHead = false): RefLabel => ({
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

export const tagLabel = (name: string): RefLabel => ({
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

export const initialCommits: CommitSummary[] = commitSeeds.map((seed, index) => ({
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

export const changedFiles: ChangedFile[] = [
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

export const initialDetails = new Map(
  initialCommits.map((commit, index) => [commit.oid, detailFor(commit, index)]),
);

export const initialDiff: FileDiff = {
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

export const defaultState = (): PersistedState => ({
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
