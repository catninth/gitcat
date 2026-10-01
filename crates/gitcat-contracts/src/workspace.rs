use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{AvatarSettings, ForgeKind, ForgeSettings, PullMode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindSettings {
    pub next_repository: String,
    pub previous_repository: String,
    pub repository_1: String,
    pub repository_2: String,
    pub repository_3: String,
    pub repository_4: String,
    pub repository_5: String,
    pub repository_6: String,
    pub repository_7: String,
    pub repository_8: String,
    pub repository_9: String,
    pub new_repository_tab: String,
    pub close_repository: String,
    pub open_repository: String,
    pub open_repository_folder: String,
    pub search_commits: String,
    pub open_settings: String,
    pub refresh_repository: String,
    pub toggle_left_panel: String,
    pub toggle_right_panel: String,
    pub fetch: String,
    pub pull: String,
    pub push: String,
    pub create_branch: String,
    pub stash: String,
    pub show_worktree: String,
    pub show_graph: String,
    pub diff_inline: String,
    pub diff_split: String,
    pub copy_selected_sha: String,
    pub continue_operation: String,
    pub abort_operation: String,
    pub stage_all: String,
    pub unstage_all: String,
    pub focus_commit_message: String,
    pub auto_resolve_conflicts: String,
    pub commit: String,
}

impl Default for KeybindSettings {
    fn default() -> Self {
        Self {
            next_repository: "Ctrl+Tab".into(),
            previous_repository: "Ctrl+Shift+Tab".into(),
            repository_1: "Ctrl+1".into(),
            repository_2: "Ctrl+2".into(),
            repository_3: "Ctrl+3".into(),
            repository_4: "Ctrl+4".into(),
            repository_5: "Ctrl+5".into(),
            repository_6: "Ctrl+6".into(),
            repository_7: "Ctrl+7".into(),
            repository_8: "Ctrl+8".into(),
            repository_9: "Ctrl+9".into(),
            new_repository_tab: "Ctrl+T".into(),
            close_repository: "Ctrl+W".into(),
            open_repository: "Ctrl+Shift+O".into(),
            open_repository_folder: "Alt+O".into(),
            search_commits: "Ctrl+F".into(),
            open_settings: "Ctrl+,".into(),
            refresh_repository: "F5".into(),
            toggle_left_panel: "Ctrl+J".into(),
            toggle_right_panel: "Ctrl+K".into(),
            fetch: "Ctrl+L".into(),
            pull: "Ctrl+Alt+P".into(),
            push: "Ctrl+Shift+P".into(),
            create_branch: "Ctrl+B".into(),
            stash: "Ctrl+Alt+S".into(),
            show_worktree: "Ctrl+Shift+W".into(),
            show_graph: "Alt+Left".into(),
            diff_inline: "Alt+1".into(),
            diff_split: "Alt+2".into(),
            copy_selected_sha: "Ctrl+Shift+C".into(),
            continue_operation: "Ctrl+Alt+Enter".into(),
            abort_operation: "Ctrl+Shift+Backspace".into(),
            stage_all: "Ctrl+Shift+S".into(),
            unstage_all: "Ctrl+Shift+U".into(),
            focus_commit_message: "Ctrl+Shift+M".into(),
            auto_resolve_conflicts: "Ctrl+Alt+R".into(),
            commit: "Ctrl+Enter".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeColors {
    pub background: String,
    pub surface: String,
    pub panel: String,
    pub border: String,
    pub text: String,
    pub muted_text: String,
    pub accent: String,
    pub success: String,
    pub warning: String,
    pub danger: String,
    pub diff_addition: String,
    pub diff_deletion: String,
    pub graph_palette: Vec<String>,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            background: "#17191f".into(),
            surface: "#1d2027".into(),
            panel: "#242832".into(),
            border: "#343946".into(),
            text: "#f2f4f8".into(),
            muted_text: "#9aa3b2".into(),
            accent: "#20b8d8".into(),
            success: "#4dbd74".into(),
            warning: "#f0ad4e".into(),
            danger: "#e05d6f".into(),
            diff_addition: "#244d33".into(),
            diff_deletion: "#562e32".into(),
            graph_palette: vec![
                "#15a0bf".into(),
                "#0669f7".into(),
                "#8e00c2".into(),
                "#c517b6".into(),
                "#d90171".into(),
                "#cd0101".into(),
                "#f25d2e".into(),
                "#f2ca33".into(),
                "#7bd938".into(),
                "#2ece9d".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppTheme {
    pub id: String,
    pub name: String,
    pub built_in: bool,
    pub colors: ThemeColors,
}

impl Default for AppTheme {
    fn default() -> Self {
        default_themes().into_iter().next().unwrap_or_else(|| Self {
            id: "gitcat-midnight".into(),
            name: "GitCat Midnight".into(),
            built_in: true,
            colors: ThemeColors::default(),
        })
    }
}

fn colors(values: [&str; 12], graph_palette: &[&str]) -> ThemeColors {
    let [
        background,
        surface,
        panel,
        border,
        text,
        muted_text,
        accent,
        success,
        warning,
        danger,
        diff_addition,
        diff_deletion,
    ] = values;
    ThemeColors {
        background: background.into(),
        surface: surface.into(),
        panel: panel.into(),
        border: border.into(),
        text: text.into(),
        muted_text: muted_text.into(),
        accent: accent.into(),
        success: success.into(),
        warning: warning.into(),
        danger: danger.into(),
        diff_addition: diff_addition.into(),
        diff_deletion: diff_deletion.into(),
        graph_palette: graph_palette.iter().map(|color| (*color).into()).collect(),
    }
}

pub fn default_themes() -> Vec<AppTheme> {
    vec![
        AppTheme {
            id: "gitcat-midnight".into(),
            name: "GitCat Midnight".into(),
            built_in: true,
            colors: ThemeColors::default(),
        },
        AppTheme {
            id: "gitkraken-dark".into(),
            name: "GitKraken Dark".into(),
            built_in: true,
            colors: colors(
                [
                    "#141422", "#1b1b2b", "#242438", "#3b3b54", "#f5f3ff", "#a5a2bb", "#18d6b3",
                    "#42d392", "#f7b955", "#ff6680", "#183f35", "#4b2935",
                ],
                &[
                    "#18d6b3", "#7b61ff", "#f252d2", "#ff9f43", "#3fc5f0", "#7ed957",
                ],
            ),
        },
        AppTheme {
            id: "github-desktop-dark".into(),
            name: "GitHub Desktop Dark".into(),
            built_in: true,
            colors: colors(
                [
                    "#1e1f22", "#25262a", "#2b2d31", "#41434a", "#f3f4f6", "#a7abb4", "#8a63d2",
                    "#3fb950", "#d29922", "#f85149", "#183d25", "#4b2226",
                ],
                &[
                    "#8a63d2", "#58a6ff", "#3fb950", "#d29922", "#f85149", "#db61a2",
                ],
            ),
        },
        AppTheme {
            id: "github-desktop-light".into(),
            name: "GitHub Desktop Light".into(),
            built_in: true,
            colors: colors(
                [
                    "#f6f8fa", "#ffffff", "#eef1f4", "#d0d7de", "#24292f", "#57606a", "#8250df",
                    "#1a7f37", "#9a6700", "#cf222e", "#dafbe1", "#ffebe9",
                ],
                &[
                    "#8250df", "#0969da", "#1a7f37", "#bf8700", "#cf222e", "#bf3989",
                ],
            ),
        },
        AppTheme {
            id: "paper-light".into(),
            name: "Paper Light".into(),
            built_in: true,
            colors: colors(
                [
                    "#f4f2ed", "#fffefa", "#e9e6df", "#cbc6bc", "#26241f", "#6e6a61", "#087f8c",
                    "#2f855a", "#a35b00", "#c43d4b", "#dcefe3", "#f7dddd",
                ],
                &[
                    "#087f8c", "#4c6fff", "#8b5cf6", "#cc5de8", "#e8590c", "#2f9e44",
                ],
            ),
        },
        AppTheme {
            id: "oled-black".into(),
            name: "OLED Black".into(),
            built_in: true,
            colors: colors(
                [
                    "#000000", "#080808", "#101010", "#292929", "#f5f5f5", "#929292", "#35cfff",
                    "#4ade80", "#fbbf24", "#fb7185", "#123821", "#421821",
                ],
                &[
                    "#35cfff", "#818cf8", "#c084fc", "#f472b6", "#fb923c", "#4ade80",
                ],
            ),
        },
    ]
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileViewMode {
    #[default]
    Path,
    Tree,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffViewMode {
    #[default]
    Hunk,
    Inline,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphColumnSettings {
    pub refs: bool,
    pub graph: bool,
    pub message: bool,
    pub author: bool,
    pub date: bool,
    pub sha: bool,
}

impl Default for GraphColumnSettings {
    fn default() -> Self {
        Self {
            refs: true,
            graph: true,
            message: true,
            author: true,
            date: true,
            sha: true,
        }
    }
}

/// Pixel width of each commit list column. The last visible column always
/// stretches to fill the row, so its stored width only matters once another
/// column is toggled on to its right. `graph` is `None` while the graph column
/// follows the lane extent it computes for itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphColumnWidths {
    pub refs: u16,
    pub graph: Option<u16>,
    pub message: u16,
    pub author: u16,
    pub date: u16,
    pub sha: u16,
}

impl Default for GraphColumnWidths {
    fn default() -> Self {
        Self {
            refs: 118,
            graph: None,
            message: 300,
            author: 86,
            date: 116,
            sha: 64,
        }
    }
}

/// GitCat's own sizes were drawn for a dense layout; one step up reads like
/// GitKraken at its 100%.
pub const DEFAULT_UI_ZOOM_PERCENT: u16 = 110;
pub const MIN_UI_ZOOM_PERCENT: u16 = 80;
pub const MAX_UI_ZOOM_PERCENT: u16 = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub default_pull_mode: PullMode,
    pub auto_fetch_interval_minutes: u16,
    pub auto_prune: bool,
    pub history_page_size: usize,
    pub diff_context_lines: u16,
    pub diff_max_bytes: usize,
    pub file_view_mode: FileViewMode,
    pub diff_view_mode: DiffViewMode,
    pub graph_columns: GraphColumnSettings,
    pub graph_column_widths: GraphColumnWidths,
    pub keybinds: KeybindSettings,
    /// Whole-interface zoom, in percent of the webview's natural size.
    pub ui_zoom_percent: u16,
    /// Hosting service for URL hosts the backend cannot recognise on its
    /// own, keyed by lower-cased host. A self-hosted GitHub Enterprise or
    /// GitLab install looks like any other domain until it is named here.
    pub forge_overrides: BTreeMap<String, ForgeKind>,
    pub avatars: AvatarSettings,
    pub forge: ForgeSettings,
    pub active_theme_id: String,
    pub themes: Vec<AppTheme>,
    #[serde(default, rename = "theme", skip_serializing)]
    pub legacy_theme: Option<ThemeColors>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_pull_mode: PullMode::Merge,
            auto_fetch_interval_minutes: 1,
            auto_prune: true,
            history_page_size: 200,
            diff_context_lines: 3,
            diff_max_bytes: 8 * 1024 * 1024,
            file_view_mode: FileViewMode::default(),
            diff_view_mode: DiffViewMode::default(),
            graph_columns: GraphColumnSettings::default(),
            graph_column_widths: GraphColumnWidths::default(),
            keybinds: KeybindSettings::default(),
            ui_zoom_percent: DEFAULT_UI_ZOOM_PERCENT,
            forge_overrides: BTreeMap::new(),
            avatars: AvatarSettings::default(),
            forge: ForgeSettings::default(),
            active_theme_id: "gitcat-midnight".into(),
            themes: default_themes(),
            legacy_theme: None,
        }
    }
}

impl AppSettings {
    pub fn migrate_legacy_theme(&mut self) {
        if self.themes.is_empty() {
            self.themes = default_themes();
        }
        if self.active_theme_id.is_empty()
            || !self
                .themes
                .iter()
                .any(|theme| theme.id == self.active_theme_id)
        {
            self.active_theme_id = self
                .themes
                .first()
                .map(|theme| theme.id.clone())
                .unwrap_or_else(|| "gitcat-midnight".into());
        }

        let Some(legacy) = self.legacy_theme.take() else {
            return;
        };
        let active_matches = self
            .themes
            .iter()
            .find(|theme| theme.id == self.active_theme_id)
            .is_some_and(|theme| theme.colors == legacy);
        if active_matches {
            return;
        }

        let mut id = "migrated-theme".to_string();
        let mut suffix = 2;
        while self.themes.iter().any(|theme| theme.id == id) {
            id = format!("migrated-theme-{suffix}");
            suffix += 1;
        }
        self.themes.push(AppTheme {
            id: id.clone(),
            name: "Migrated theme".into(),
            built_in: false,
            colors: legacy,
        });
        self.active_theme_id = id;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TabKind {
    #[default]
    Repository,
    Start,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryTab {
    pub id: String,
    pub repository_path: String,
    pub display_name: String,
    pub order: i32,
    #[serde(default)]
    pub kind: TabKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict_target: Option<String>,
    #[serde(default)]
    pub conflict_target_disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryGroup {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    pub order: i32,
    pub tabs: Vec<RepositoryTab>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceState {
    pub version: u32,
    pub ungrouped_tabs: Vec<RepositoryTab>,
    pub groups: Vec<RepositoryGroup>,
    pub active_tab_id: Option<String>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            version: 2,
            ungrouped_tabs: Vec::new(),
            groups: Vec::new(),
            active_tab_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentRepository {
    pub path: String,
    pub name: String,
    pub opened_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PersistedState {
    pub settings: AppSettings,
    pub workspace: WorkspaceState,
    pub recents: Vec<RecentRepository>,
    /// Folder the user last pointed a repository at -- where a clone landed, where
    /// a repository was initialised, or the parent of one that was opened. It is
    /// where the next folder picker starts, so the same place is not hunted down
    /// again on every page. It stays out of `AppSettings` because settings are
    /// exported to a shareable file and a path on this machine is not portable.
    pub last_directory: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppMetadata {
    pub version: String,
    pub commit: String,
}
