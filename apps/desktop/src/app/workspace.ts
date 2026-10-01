import {
    ALL_GRAPH_COLUMNS,
    DEFAULT_GRAPH_COLUMN_WIDTHS,
    GRAPH_COLUMN_MIN_WIDTH,
    GRAPH_COLUMNS,
    MAX_GRAPH_COLUMN_WIDTH,
    MIN_GRAPH_COLUMN_WIDTH,
    visibleGraphColumns,
} from "../lib/columns";
import { isForgeKind } from "../lib/forge";
import { DEFAULT_KEYBINDS, duplicateKeybinds, keybindValidationError } from "../lib/keybinds";
import { samePath } from "../lib/paths";
import { MAX_ZOOM_PERCENT, MIN_ZOOM_PERCENT } from "../lib/zoom";
import type {
    AppSettings,
    AppTheme,
    AvatarSettings,
    ForgeKind,
    ForgeSettings,
    GraphColumnSettings,
    GraphColumnWidths,
    KeybindSettings,
    PersistedState,
    RepositoryTab,
    ThemeColors,
} from "../lib/types";
import { DEFAULT_SETTINGS, RECENT_LIMIT } from "./defaults";
import { BUILTIN_THEME_IDS, cloneDefaultThemes, DEFAULT_THEME_COLORS, DEFAULT_THEME_ID } from "./themePresets";

export function makeId(prefix: string): string {
    return `${prefix}-${crypto.randomUUID()}`;
}

export function workspaceTabs(state: PersistedState["workspace"]): RepositoryTab[] {
    return [
        ...(state.ungrouped_tabs ?? []),
        ...state.groups.flatMap((group) => group.tabs),
    ];
}

/**
 * The open tab holding a repository, or undefined when nothing holds it.
 *
 * Paths are compared by what they name rather than by how they are spelled,
 * because a folder opened from outside GitCat -- the Windows Explorer context
 * menu -- arrives spelled the way the user right-clicked it, which need not
 * match the root Git reported when the tab was opened.
 */
export function findRepositoryTab(
    state: PersistedState["workspace"],
    path: string,
): RepositoryTab | undefined {
    return workspaceTabs(state)
        .find((tab) => tab.kind !== "start" && samePath(tab.repository_path, path));
}

export function normalizePersistedKeybinds(
    keybinds: Partial<KeybindSettings> | undefined,
): KeybindSettings {
    const actions = Object.keys(DEFAULT_KEYBINDS) as (keyof KeybindSettings)[];
    const normalized = { ...DEFAULT_KEYBINDS };
    for (const action of actions) {
        const candidate = keybinds?.[action];
        normalized[action] = typeof candidate === "string" && !keybindValidationError(candidate)
            ? candidate
            : DEFAULT_KEYBINDS[action];
    }
    for (let pass = 0; pass < actions.length; pass += 1) {
        const duplicates = duplicateKeybinds(normalized);
        if (!duplicates.size) break;
        for (const action of duplicates) normalized[action] = DEFAULT_KEYBINDS[action];
    }
    return normalized;
}

// The backend rejects a settings save whose override keys are not bare
// lower-case hosts, so stored junk is dropped rather than carried forward.
function normalizeAvatarSettings(avatars: Partial<AvatarSettings> | undefined): AvatarSettings {
    const defaults = DEFAULT_SETTINGS.avatars;
    return {
        enabled: typeof avatars?.enabled === "boolean" ? avatars.enabled : defaults.enabled,
        gravatar_fallback: typeof avatars?.gravatar_fallback === "boolean"
            ? avatars.gravatar_fallback
            : defaults.gravatar_fallback,
    };
}

function normalizeForgeSettings(forge: Partial<ForgeSettings> | undefined): ForgeSettings {
    const defaults = DEFAULT_SETTINGS.forge;
    return {
        pull_requests: typeof forge?.pull_requests === "boolean"
            ? forge.pull_requests
            : defaults.pull_requests,
        checks: typeof forge?.checks === "boolean" ? forge.checks : defaults.checks,
    };
}

function normalizeForgeOverrides(
    overrides: Record<string, string> | undefined,
): Record<string, ForgeKind> {
    const normalized: Record<string, ForgeKind> = {};
    if (!overrides || typeof overrides !== "object") return normalized;
    for (const [rawHost, kind] of Object.entries(overrides)) {
        const host = rawHost.trim().toLowerCase();
        if (!host || host.length > MAX_HOST_LENGTH || /[/:@\s]/.test(host)) continue;
        if (typeof kind === "string" && isForgeKind(kind) && kind !== "unknown") {
            normalized[host] = kind;
        }
    }
    return normalized;
}

function normalizeGraphColumns(columns: Partial<GraphColumnSettings> | undefined): GraphColumnSettings {
    const normalized = { ...ALL_GRAPH_COLUMNS };
    for (const { key } of GRAPH_COLUMNS) {
        const value = columns?.[key];
        if (typeof value === "boolean") normalized[key] = value;
    }
    return visibleGraphColumns(normalized).length ? normalized : { ...ALL_GRAPH_COLUMNS };
}

function normalizeGraphColumnWidths(
    widths: Partial<GraphColumnWidths> | undefined,
): GraphColumnWidths {
    const normalized = { ...DEFAULT_GRAPH_COLUMN_WIDTHS };
    for (const { key } of GRAPH_COLUMNS) {
        const value = widths?.[key];
        if (key === "graph") {
            // Null keeps the graph column following its own lane extent.
            normalized.graph = typeof value === "number" && Number.isFinite(value)
                ? boundedNumber(value, MIN_GRAPH_COLUMN_WIDTH, MIN_GRAPH_COLUMN_WIDTH, MAX_GRAPH_COLUMN_WIDTH)
                : null;
            continue;
        }
        if (typeof value === "number" && Number.isFinite(value)) {
            normalized[key] = boundedNumber(
                value,
                DEFAULT_GRAPH_COLUMN_WIDTHS[key],
                GRAPH_COLUMN_MIN_WIDTH[key],
                MAX_GRAPH_COLUMN_WIDTH,
            );
        }
    }
    return normalized;
}

// Longest a DNS name can be; the backend enforces the same cap.
const MAX_HOST_LENGTH = 253;
const HEX_COLOR = /^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/i;
const THEME_ID = /^[a-zA-Z0-9_-]{1,96}$/;
const THEME_COLOR_KEYS: Array<Exclude<keyof ThemeColors, "graph_palette">> = [
    "background",
    "surface",
    "panel",
    "border",
    "text",
    "muted_text",
    "accent",
    "success",
    "warning",
    "danger",
    "diff_addition",
    "diff_deletion",
];

function normalizeThemeColors(value: unknown, fallback = DEFAULT_THEME_COLORS): ThemeColors | null {
    if (!value || typeof value !== "object") return null;
    const candidate = value as Partial<ThemeColors>;
    const colors = { ...fallback, graph_palette: [...fallback.graph_palette] };
    for (const key of THEME_COLOR_KEYS) {
        if (typeof candidate[key] !== "string" || !HEX_COLOR.test(candidate[key])) return null;
        colors[key] = candidate[key];
    }
    if (!Array.isArray(candidate.graph_palette)
        || candidate.graph_palette.length < 1
        || candidate.graph_palette.length > 64
        || candidate.graph_palette.some((color) => typeof color !== "string" || !HEX_COLOR.test(color))) {
        return null;
    }
    colors.graph_palette = [...candidate.graph_palette];
    return colors;
}

function uniqueThemeId(base: string, used: ReadonlySet<string>): string {
    if (!used.has(base.toLowerCase())) return base;
    let suffix = 2;
    while (used.has(`${base}-${suffix}`.toLowerCase())) suffix += 1;
    return `${base}-${suffix}`;
}

type LegacyAppSettings = Partial<AppSettings> & { theme?: ThemeColors };

function boundedNumber(value: unknown, fallback: number, minimum: number, maximum: number): number {
    const number = Number(value);
    return Number.isFinite(number) ? Math.min(maximum, Math.max(minimum, Math.trunc(number))) : fallback;
}

export function normalizeAppSettings(value: unknown): AppSettings {
    const source = value && typeof value === "object" ? value as LegacyAppSettings : {};
    const themes = cloneDefaultThemes();
    const usedIds = new Set(themes.map((theme) => theme.id.toLowerCase()));

    if (Array.isArray(source.themes)) {
        for (const item of source.themes) {
            if (!item || typeof item !== "object") continue;
            const candidate = item as Partial<AppTheme>;
            if (typeof candidate.id !== "string"
                || !THEME_ID.test(candidate.id)
                || BUILTIN_THEME_IDS.has(candidate.id)) continue;
            const name = typeof candidate.name === "string" ? candidate.name.trim() : "";
            const colors = normalizeThemeColors(candidate.colors);
            if (!name || name.length > 64 || !colors) continue;
            const id = uniqueThemeId(candidate.id, usedIds);
            themes.push({ id, name, built_in: false, colors });
            usedIds.add(id.toLowerCase());
        }
    }

    let activeThemeId = typeof source.active_theme_id === "string"
        && themes.some((theme) => theme.id === source.active_theme_id)
        ? source.active_theme_id
        : DEFAULT_THEME_ID;

    if (!Array.isArray(source.themes)) {
        const legacyColors = normalizeThemeColors(source.theme);
        if (legacyColors && JSON.stringify(legacyColors) !== JSON.stringify(DEFAULT_THEME_COLORS)) {
            const id = uniqueThemeId("migrated-theme", usedIds);
            themes.push({ id, name: "Migrated theme", built_in: false, colors: legacyColors });
            activeThemeId = id;
        }
    }

    return {
        default_pull_mode: ["merge", "fast_forward_only", "rebase"].includes(source.default_pull_mode ?? "")
            ? source.default_pull_mode!
            : DEFAULT_SETTINGS.default_pull_mode,
        auto_fetch_interval_minutes: boundedNumber(source.auto_fetch_interval_minutes, DEFAULT_SETTINGS.auto_fetch_interval_minutes, 0, 60),
        auto_prune: typeof source.auto_prune === "boolean" ? source.auto_prune : DEFAULT_SETTINGS.auto_prune,
        history_page_size: boundedNumber(source.history_page_size, DEFAULT_SETTINGS.history_page_size, 1, 500),
        diff_context_lines: boundedNumber(source.diff_context_lines, DEFAULT_SETTINGS.diff_context_lines, 0, 100),
        diff_max_bytes: boundedNumber(source.diff_max_bytes, DEFAULT_SETTINGS.diff_max_bytes, 1, 128 * 1024 * 1024),
        file_view_mode: source.file_view_mode === "tree" ? "tree" : "path",
        diff_view_mode: ["hunk", "inline", "split"].includes(source.diff_view_mode ?? "")
            ? source.diff_view_mode!
            : DEFAULT_SETTINGS.diff_view_mode,
        graph_columns: normalizeGraphColumns(source.graph_columns),
        graph_column_widths: normalizeGraphColumnWidths(source.graph_column_widths),
        forge_overrides: normalizeForgeOverrides(source.forge_overrides),
        avatars: normalizeAvatarSettings(source.avatars),
        forge: normalizeForgeSettings(source.forge),
        keybinds: normalizePersistedKeybinds(source.keybinds),
        ui_zoom_percent: boundedNumber(source.ui_zoom_percent, DEFAULT_SETTINGS.ui_zoom_percent, MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT),
        active_theme_id: activeThemeId,
        themes,
    };
}

export function normalizePersistedState(state: PersistedState): PersistedState {
    const workspace: PersistedState["workspace"] = {
        version: 2,
        active_tab_id: state.workspace?.active_tab_id ?? null,
        groups: state.workspace?.groups ?? [],
        ungrouped_tabs: state.workspace?.ungrouped_tabs ?? [],
    };
    const recents = state.recents?.length
        ? state.recents
        : workspaceTabs(workspace)
            .filter((tab) => tab.kind !== "start")
            .map((tab) => ({ path: tab.repository_path, name: tab.display_name, opened_at: 0 }));
    // A hand-edited state file can carry anything here, and the value is handed
    // to a folder picker, so only a non-empty string survives.
    const lastDirectory = typeof state.last_directory === "string" && state.last_directory.trim()
        ? state.last_directory
        : null;
    return {
        settings: normalizeAppSettings(state.settings),
        workspace,
        recents: recents.slice(0, RECENT_LIMIT),
        last_directory: lastDirectory,
    };
}
