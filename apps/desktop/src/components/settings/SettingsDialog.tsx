import {
  Download,
  Keyboard,
  Palette,
  Plug,
  RotateCcw,
  Settings2,
  Upload,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { exportSettings, importSettingsFromDialog, parseSettingsFile } from "../../app/settingsTransfer";
import { activeTheme, applyTheme } from "../../app/theme";
import { DEFAULT_THEME_ID } from "../../app/themePresets";
import { normalizeAppSettings } from "../../app/workspace";
import { cx } from "../../lib";
import { duplicateKeybinds } from "../../lib/keybinds";
import { isTauriEnvironment } from "../../lib/platform";
import { applyZoom, ZOOM_LEVELS } from "../../lib/zoom";
import type { AppSettings, AppTheme, PullMode, ThemeColors } from "../../lib/types";
import { Button, Input, Modal, ModalSpacer } from "../ui";
import { IntegrationsPage } from "./IntegrationsPage";
import { KeybindEditor } from "./KeybindEditor";
import { CheckField, FIELD_INPUT, Field, SectionHeading } from "./SettingsField";
import { ThemeEditor } from "./ThemeEditor";

type SettingsPage = "general" | "integrations" | "themes" | "keybinds" | "backup";

const PAGES: Array<{ id: SettingsPage; label: string; icon: typeof Settings2 }> = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "integrations", label: "Integrations", icon: Plug },
  { id: "themes", label: "Themes", icon: Palette },
  { id: "keybinds", label: "Keybinds", icon: Keyboard },
  { id: "backup", label: "Import & export", icon: Download },
];

interface SettingsDialogProps {
  settings: AppSettings;
  defaults: AppSettings;
  onSave: (settings: AppSettings) => void;
  onClose: () => void;
}

function uniqueThemeName(base: string, themes: readonly AppTheme[]): string {
  const names = new Set(themes.map((theme) => theme.name.toLowerCase()));
  if (!names.has(base.toLowerCase())) return base;
  let suffix = 2;
  while (names.has(`${base} ${suffix}`.toLowerCase())) suffix += 1;
  return `${base} ${suffix}`;
}

function transferError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") {
    return error.message;
  }
  return String(error);
}

export function SettingsDialog({ settings, defaults, onSave, onClose }: SettingsDialogProps) {
  const [draft, setDraft] = useState<AppSettings>(() => structuredClone(settings));
  const [page, setPage] = useState<SettingsPage>("general");
  const [transferring, setTransferring] = useState(false);
  const [transferNotice, setTransferNotice] = useState<{ tone: "success" | "error"; message: string } | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const duplicateActions = useMemo(() => duplicateKeybinds(draft.keybinds), [draft.keybinds]);
  const selectedTheme = activeTheme(draft);
  const invalidThemeName = !selectedTheme?.name.trim();
  const canSave = duplicateActions.size === 0 && !invalidThemeName;

  useEffect(() => {
    applyTheme(draft);
  }, [draft]);
  useEffect(() => () => applyTheme(settings), [settings]);
  useEffect(() => {
    applyZoom(draft.ui_zoom_percent).catch(() => undefined);
  }, [draft.ui_zoom_percent]);
  useEffect(() => () => {
    applyZoom(settings.ui_zoom_percent).catch(() => undefined);
  }, [settings.ui_zoom_percent]);

  const updateSelectedTheme = (mutate: (theme: AppTheme) => AppTheme) => {
    setDraft((current) => ({
      ...current,
      themes: current.themes.map((theme) => theme.id === current.active_theme_id ? mutate(theme) : theme),
    }));
  };

  const createTheme = (source = selectedTheme, copyLabel = "Custom theme") => {
    if (!source) return;
    const theme: AppTheme = {
      id: `theme-${crypto.randomUUID()}`,
      name: uniqueThemeName(copyLabel, draft.themes),
      built_in: false,
      colors: structuredClone(source.colors),
    };
    setDraft((current) => ({
      ...current,
      active_theme_id: theme.id,
      themes: [...current.themes, theme],
    }));
  };

  const acceptImport = (imported: AppSettings) => {
    const normalized = normalizeAppSettings(imported);
    setDraft(normalized);
    setTransferNotice({
      tone: "success",
      message: `Imported ${normalized.themes.length} themes and ${Object.keys(normalized.keybinds).length} keybinds. Save changes to apply.`,
    });
  };

  const runNativeImport = async () => {
    setTransferring(true);
    setTransferNotice(null);
    try {
      const imported = await importSettingsFromDialog();
      if (imported) acceptImport(imported.settings);
    } catch (error) {
      setTransferNotice({ tone: "error", message: transferError(error) });
    } finally {
      setTransferring(false);
    }
  };

  const startImport = () => {
    if (isTauriEnvironment()) {
      void runNativeImport();
    } else {
      fileInputRef.current?.click();
    }
  };

  const runExport = async () => {
    setTransferring(true);
    setTransferNotice(null);
    try {
      const destination = await exportSettings(draft);
      if (destination) setTransferNotice({ tone: "success", message: "Settings exported." });
    } catch (error) {
      setTransferNotice({ tone: "error", message: transferError(error) });
    } finally {
      setTransferring(false);
    }
  };

  return (
    <Modal
      description="Git behavior, hosting services, reusable themes, keyboard shortcuts, and portable backups."
      footer={
        <>
          <Button icon={<RotateCcw size={15} />} onClick={() => setDraft(structuredClone(defaults))}>Reset defaults</Button>
          <ModalSpacer />
          <Button onClick={onClose}>Cancel</Button>
          <Button disabled={!canSave} onClick={() => onSave(normalizeAppSettings(draft))} tone="accent">Save changes</Button>
        </>
      }
      onClose={onClose}
      title="Preferences"
      width="wide"
    >
      <div className="grid min-h-[545px] grid-cols-[180px_minmax(0,1fr)] gap-5 max-[720px]:grid-cols-1">
        <nav aria-label="Preference sections" className="flex flex-col gap-1 border-r border-border pr-4 max-[720px]:flex-row max-[720px]:overflow-auto max-[720px]:border-r-0 max-[720px]:border-b max-[720px]:pb-3 max-[720px]:pr-0">
          {PAGES.map(({ id, label, icon: Icon }) => (
            <button
              aria-current={page === id ? "page" : undefined}
              className={cx(
                "flex min-h-9 cursor-pointer items-center gap-2.25 rounded-[5px] border px-2.5 text-left text-[11px] font-semibold whitespace-nowrap transition-colors",
                page === id
                  ? "border-[color-mix(in_srgb,var(--gc-accent)_45%,var(--gc-border))] bg-[color-mix(in_srgb,var(--gc-accent)_10%,var(--gc-background))] text-foreground"
                  : "border-transparent bg-transparent text-muted hover:bg-row-hover hover:text-foreground",
              )}
              key={id}
              onClick={() => setPage(id)}
              type="button"
            >
              <Icon className={page === id ? "text-accent" : ""} size={15} />
              {label}
            </button>
          ))}
          <div className="mt-auto hidden rounded-md border border-border bg-background/36 p-2.5 text-[9px] leading-[1.45] text-muted min-[721px]:block">
            Changes stay in preview until saved.
          </div>
        </nav>

        <div className="min-w-0">
          {page === "general" ? (
            <div className="grid grid-cols-2 gap-x-7 max-[840px]:grid-cols-1">
              <section>
                <SectionHeading>Git behavior</SectionHeading>
                <Field label="Default pull mode">
                  <select
                    className={FIELD_INPUT}
                    onChange={(event) => setDraft((current) => ({ ...current, default_pull_mode: event.target.value as PullMode }))}
                    value={draft.default_pull_mode}
                  >
                    <option value="merge">Merge (fast-forward if possible)</option>
                    <option value="fast_forward_only">Fast-forward only</option>
                    <option value="rebase">Rebase</option>
                  </select>
                </Field>
                <Field hint="minutes, 0 disables" label="Auto-fetch interval">
                  <Input
                    className={FIELD_INPUT}
                    max={60}
                    min={0}
                    onChange={(event) => setDraft((current) => ({ ...current, auto_fetch_interval_minutes: Number(event.target.value) }))}
                    type="number"
                    value={draft.auto_fetch_interval_minutes}
                  />
                </Field>
                <CheckField
                  checked={draft.auto_prune}
                  onChange={(auto_prune) => setDraft((current) => ({ ...current, auto_prune }))}
                >
                  Prune stale remote branches when fetching
                </CheckField>
              </section>
              <section>
                <SectionHeading>Performance</SectionHeading>
                <Field label="Commits per page">
                  <Input
                    className={FIELD_INPUT}
                    max={500}
                    min={1}
                    onChange={(event) => setDraft((current) => ({ ...current, history_page_size: Number(event.target.value) }))}
                    type="number"
                    value={draft.history_page_size}
                  />
                </Field>
                <Field label="Diff context lines">
                  <Input
                    className={FIELD_INPUT}
                    max={100}
                    min={0}
                    onChange={(event) => setDraft((current) => ({ ...current, diff_context_lines: Number(event.target.value) }))}
                    type="number"
                    value={draft.diff_context_lines}
                  />
                </Field>
              </section>
              <section>
                <SectionHeading>Appearance</SectionHeading>
                <Field hint="Ctrl+= / Ctrl+- / Ctrl+0" label="Interface zoom">
                  <select
                    className={FIELD_INPUT}
                    onChange={(event) => setDraft((current) => ({ ...current, ui_zoom_percent: Number(event.target.value) }))}
                    value={draft.ui_zoom_percent}
                  >
                    {(ZOOM_LEVELS as readonly number[]).includes(draft.ui_zoom_percent) ? null : (
                      <option value={draft.ui_zoom_percent}>{draft.ui_zoom_percent}%</option>
                    )}
                    {ZOOM_LEVELS.map((level) => (
                      <option key={level} value={level}>{level}%</option>
                    ))}
                  </select>
                </Field>
              </section>
              <section className="col-span-full">
                <SectionHeading>Author pictures</SectionHeading>
                <CheckField
                  checked={draft.avatars.enabled}
                  onChange={(enabled) => setDraft((current) => ({
                    ...current,
                    avatars: { ...current.avatars, enabled },
                  }))}
                >
                  Show author pictures from the repository's hosting service
                </CheckField>
                <CheckField
                  checked={draft.avatars.gravatar_fallback}
                  onChange={(gravatar_fallback) => setDraft((current) => ({
                    ...current,
                    avatars: { ...current.avatars, gravatar_fallback },
                  }))}
                >
                  Also ask Gravatar for authors the hosting service does not know
                </CheckField>
                <p className="mt-1.5 text-[10px] leading-[1.45] text-muted/72">
                  Gravatar is a third party that is not hosting the repository, and the lookup sends
                  it a hash of the author's email address.
                </p>
              </section>
              <section className="col-span-full">
                <SectionHeading>Pull requests and checks</SectionHeading>
                <CheckField
                  checked={draft.forge.pull_requests}
                  onChange={(pull_requests) => setDraft((current) => ({
                    ...current,
                    forge: { ...current.forge, pull_requests },
                  }))}
                >
                  Show the open pull request a branch belongs to
                </CheckField>
                <CheckField
                  checked={draft.forge.checks}
                  disabled={!draft.forge.pull_requests}
                  onChange={(checks) => setDraft((current) => ({
                    ...current,
                    forge: { ...current.forge, checks },
                  }))}
                >
                  Show the check state of branches with a pull request
                </CheckField>
                <p className="mt-1.5 text-[10px] leading-[1.45] text-muted/72">
                  Both ask the service that already hosts the repository. Checks are only looked up
                  for branches with a pull request and for the checked-out branch.
                </p>
              </section>
            </div>
          ) : null}

          {page === "integrations" ? (
            <IntegrationsPage
              onOverridesChange={(forge_overrides) => setDraft((current) => ({ ...current, forge_overrides }))}
              overrides={draft.forge_overrides}
            />
          ) : null}

          {page === "themes" ? (
            <ThemeEditor
              activeThemeId={draft.active_theme_id}
              defaultPalette={defaults.themes.find((theme) => theme.id === DEFAULT_THEME_ID)?.colors.graph_palette ?? []}
              onColorChange={(field: Exclude<keyof ThemeColors, "graph_palette">, value: string) =>
                updateSelectedTheme((theme) => ({ ...theme, colors: { ...theme.colors, [field]: value } }))}
              onCreate={() => createTheme()}
              onDelete={(id) => setDraft((current) => ({
                ...current,
                active_theme_id: current.active_theme_id === id ? DEFAULT_THEME_ID : current.active_theme_id,
                themes: current.themes.filter((theme) => theme.id !== id || theme.built_in),
              }))}
              onDuplicate={(id) => {
                const source = draft.themes.find((theme) => theme.id === id);
                if (source) createTheme(source, `${source.name} copy`);
              }}
              onPaletteChange={(graph_palette) =>
                updateSelectedTheme((theme) => ({ ...theme, colors: { ...theme.colors, graph_palette } }))}
              onRename={(id, name) => setDraft((current) => ({
                ...current,
                themes: current.themes.map((theme) => theme.id === id && !theme.built_in ? { ...theme, name } : theme),
              }))}
              onSelect={(active_theme_id) => setDraft((current) => ({ ...current, active_theme_id }))}
              themes={draft.themes}
            />
          ) : null}

          {page === "keybinds" ? (
            <KeybindEditor
              duplicateActions={duplicateActions}
              keybinds={draft.keybinds}
              onChange={(action, binding) =>
                setDraft((current) => ({ ...current, keybinds: { ...current.keybinds, [action]: binding } }))}
            />
          ) : null}

          {page === "backup" ? (
            <section className="max-w-[720px]">
              <SectionHeading>Portable settings</SectionHeading>
              <p className="max-w-[620px] text-[11px] leading-[1.6] text-muted">
                One versioned JSON file contains every preference, all themes, graph display choices, and every keybind. Repository contents and credentials are never included.
              </p>
              <div className="mt-5 grid grid-cols-2 gap-3 max-[840px]:grid-cols-1">
                <div className="rounded-[7px] border border-border bg-background/45 p-4">
                  <Download className="mb-3 text-accent" size={21} />
                  <h3 className="mb-1.5 text-[13px]">Export settings</h3>
                  <p className="mb-4 min-h-8 text-[10px] leading-[1.5] text-muted">
                    Save current draft as a portable <code>.json</code> backup.
                  </p>
                  <Button disabled={transferring || !canSave} icon={<Download size={14} />} onClick={() => void runExport()}>
                    Export file
                  </Button>
                </div>
                <div className="rounded-[7px] border border-border bg-background/45 p-4">
                  <Upload className="mb-3 text-accent" size={21} />
                  <h3 className="mb-1.5 text-[13px]">Import settings</h3>
                  <p className="mb-4 min-h-8 text-[10px] leading-[1.5] text-muted">
                    Preview a GitCat backup, then save to replace preferences.
                  </p>
                  <Button disabled={transferring} icon={<Upload size={14} />} onClick={startImport}>
                    Import file
                  </Button>
                </div>
              </div>
              <input
                accept="application/json,.json"
                className="hidden"
                onChange={(event) => {
                  const file = event.currentTarget.files?.[0];
                  event.currentTarget.value = "";
                  if (!file) return;
                  setTransferring(true);
                  setTransferNotice(null);
                  void file.text()
                    .then((text) => acceptImport(parseSettingsFile(text, file.size)))
                    .catch((error) => setTransferNotice({ tone: "error", message: transferError(error) }))
                    .finally(() => setTransferring(false));
                }}
                ref={fileInputRef}
                type="file"
              />
              {transferNotice ? (
                <p
                  aria-live="polite"
                  className={cx(
                    "mt-4 rounded-[5px] border px-3 py-2.5 text-[10px]",
                    transferNotice.tone === "success"
                      ? "border-[color-mix(in_srgb,var(--gc-success)_50%,var(--gc-border))] bg-[color-mix(in_srgb,var(--gc-success)_9%,var(--gc-background))] text-success"
                      : "border-[color-mix(in_srgb,var(--gc-danger)_50%,var(--gc-border))] bg-[color-mix(in_srgb,var(--gc-danger)_9%,var(--gc-background))] text-danger",
                  )}
                >
                  {transferNotice.message}
                </p>
              ) : null}
            </section>
          ) : null}
        </div>
      </div>
    </Modal>
  );
}
