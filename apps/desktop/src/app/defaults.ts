import { ALL_GRAPH_COLUMNS, DEFAULT_GRAPH_COLUMN_WIDTHS } from "../lib/columns";
import { DEFAULT_KEYBINDS } from "../lib/keybinds";
import { DEFAULT_ZOOM_PERCENT } from "../lib/zoom";
import type { AppSettings, PersistedState } from "../lib/types";
import type { CommitDraft } from "../components/worktree";
import { cloneDefaultThemes, DEFAULT_THEME_ID } from "./themePresets";

export const DEFAULT_SETTINGS: AppSettings = {
    default_pull_mode: "merge",
    auto_fetch_interval_minutes: 1,
    auto_prune: true,
    history_page_size: 200,
    diff_context_lines: 3,
    diff_max_bytes: 8 * 1024 * 1024,
    file_view_mode: "path",
    diff_view_mode: "hunk",
    graph_columns: ALL_GRAPH_COLUMNS,
    graph_column_widths: DEFAULT_GRAPH_COLUMN_WIDTHS,
    keybinds: DEFAULT_KEYBINDS,
    ui_zoom_percent: DEFAULT_ZOOM_PERCENT,
    forge_overrides: {},
    avatars: { enabled: true, gravatar_fallback: false },
    forge: { pull_requests: true, checks: true },
    active_theme_id: DEFAULT_THEME_ID,
    themes: cloneDefaultThemes(),
};

export const EMPTY_STATE: PersistedState = {
    settings: DEFAULT_SETTINGS,
    workspace: { version: 2, ungrouped_tabs: [], groups: [], active_tab_id: null },
    recents: [],
    last_directory: null,
};

export const RECENT_LIMIT = 30;

export const EMPTY_COMMIT_DRAFT: CommitDraft = { message: "", description: "", amend: false, signoff: false };
