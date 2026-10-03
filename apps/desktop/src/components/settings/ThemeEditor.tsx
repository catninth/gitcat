import { Check, Copy, Plus, RotateCcw, Trash2 } from "lucide-react";

import { cx } from "../../lib";
import { GRAPH_LANE_SLOTS } from "../../lib/styles";
import type { AppTheme, ThemeColors } from "../../lib/types";
import { Button, Input } from "../ui";
import { FIELD_INPUT, SectionHeading } from "./SettingsField";

const COLOR_FIELDS: Array<[Exclude<keyof ThemeColors, "graph_palette">, string]> = [
  ["background", "Background"],
  ["surface", "Surface"],
  ["panel", "Panel"],
  ["border", "Border"],
  ["text", "Text"],
  ["muted_text", "Muted text"],
  ["accent", "Accent"],
  ["success", "Success"],
  ["warning", "Warning"],
  ["danger", "Danger"],
  ["diff_addition", "Diff addition"],
  ["diff_deletion", "Diff deletion"],
];

function ThemeSwatches({ theme }: { theme: AppTheme }) {
  const swatches = [
    theme.colors.background,
    theme.colors.panel,
    theme.colors.accent,
    theme.colors.success,
    theme.colors.danger,
  ];
  return (
    <span aria-hidden="true" className="flex h-2.5 overflow-hidden rounded-full border border-black/15">
      {swatches.map((color, index) => <span className="w-4" key={`${color}-${index}`} style={{ background: color }} />)}
    </span>
  );
}

function ThemeSpecimen({ theme }: { theme: AppTheme }) {
  const { colors } = theme;
  return (
    <div
      aria-label={`${theme.name} preview`}
      className="grid h-30 grid-cols-[23%_1fr_25%] overflow-hidden rounded-[7px] border shadow-[0_12px_30px_rgb(0_0_0/18%)]"
      style={{ background: colors.background, borderColor: colors.border, color: colors.text }}
    >
      <div className="border-r p-2" style={{ background: colors.surface, borderColor: colors.border }}>
        <div className="mb-2 h-1.5 w-7 rounded-full" style={{ background: colors.muted_text }} />
        {[0.82, 0.65, 0.75, 0.55].map((width, index) => (
          <div className="mb-1.5 h-1 rounded-full opacity-75" key={width} style={{ background: index === 1 ? colors.accent : colors.muted_text, width: `${width * 100}%` }} />
        ))}
      </div>
      <div className="p-2.5">
        <div className="mb-2 flex gap-1">
          {colors.graph_palette.slice(0, 4).map((color) => <span className="size-1.5 rounded-full" key={color} style={{ background: color }} />)}
        </div>
        {[0, 1, 2, 3].map((row) => (
          <div className="mb-2 grid grid-cols-[9px_1fr] items-center gap-2" key={row}>
            <span className="size-1.5 rounded-full" style={{ background: colors.graph_palette[row % colors.graph_palette.length] }} />
            <span className="h-1 rounded-full" style={{ background: row === 0 ? colors.text : colors.muted_text, opacity: row === 0 ? 0.85 : 0.5, width: `${86 - row * 9}%` }} />
          </div>
        ))}
      </div>
      <div className="border-l p-2" style={{ background: colors.panel, borderColor: colors.border }}>
        <div className="mb-2 h-1.5 w-8 rounded-full" style={{ background: colors.text, opacity: 0.8 }} />
        <div className="mb-1 h-1 w-full rounded-full" style={{ background: colors.diff_addition }} />
        <div className="h-1 w-4/5 rounded-full" style={{ background: colors.diff_deletion }} />
      </div>
    </div>
  );
}

export function ThemeEditor({
  themes,
  activeThemeId,
  defaultPalette,
  onSelect,
  onCreate,
  onDuplicate,
  onDelete,
  onRename,
  onColorChange,
  onPaletteChange,
}: {
  themes: AppTheme[];
  activeThemeId: string;
  defaultPalette: readonly string[];
  onSelect: (id: string) => void;
  onCreate: () => void;
  onDuplicate: (id: string) => void;
  onDelete: (id: string) => void;
  onRename: (id: string, name: string) => void;
  onColorChange: (field: Exclude<keyof ThemeColors, "graph_palette">, value: string) => void;
  onPaletteChange: (palette: string[]) => void;
}) {
  const theme = themes.find((candidate) => candidate.id === activeThemeId) ?? themes[0];
  if (!theme) return null;
  const editable = !theme.built_in;
  const lanes = Array.from(
    { length: GRAPH_LANE_SLOTS },
    (_, index) => theme.colors.graph_palette[index] ?? defaultPalette[index % defaultPalette.length] ?? "#000000",
  );

  return (
    <div className="grid min-h-[510px] grid-cols-[230px_minmax(0,1fr)] gap-5 max-[840px]:grid-cols-1">
      <aside className="min-w-0 border-r border-border pr-4 max-[840px]:border-r-0 max-[840px]:border-b max-[840px]:pb-4 max-[840px]:pr-0">
        <div className="mb-3 flex items-center justify-between gap-2">
          <SectionHeading className="m-0">Theme library</SectionHeading>
          <Button compact icon={<Plus size={13} />} onClick={onCreate} title="Create theme">New</Button>
        </div>
        <div className="flex max-h-[430px] flex-col gap-1.5 overflow-auto pr-1">
          {themes.map((candidate) => {
            const active = candidate.id === theme.id;
            return (
              <button
                aria-pressed={active}
                className={cx(
                  "grid cursor-pointer grid-cols-[minmax(0,1fr)_auto] gap-x-2 gap-y-1 rounded-md border px-2.5 py-2 text-left transition-colors",
                  active
                    ? "border-accent bg-[color-mix(in_srgb,var(--gc-accent)_10%,var(--gc-background))]"
                    : "border-border bg-background/45 hover:border-border-strong hover:bg-row-hover",
                )}
                key={candidate.id}
                onClick={() => onSelect(candidate.id)}
                title={candidate.name}
                type="button"
              >
                <strong className="overflow-hidden text-ellipsis whitespace-nowrap text-[13px]">{candidate.name}</strong>
                {active ? <Check className="text-accent" size={13} /> : null}
                <ThemeSwatches theme={candidate} />
                <small className="self-center text-[10px] uppercase tracking-[0.08em] text-muted">
                  {candidate.built_in ? "Preset" : "Custom"}
                </small>
              </button>
            );
          })}
        </div>
      </aside>

      <section className="min-w-0">
        <div className="mb-3 grid grid-cols-[minmax(0,1fr)_auto] items-end gap-3">
          <label className="flex min-w-0 flex-col gap-1.25 text-[12px] text-muted">
            Theme name
            <Input
              aria-invalid={editable && !theme.name.trim()}
              className={FIELD_INPUT}
              disabled={!editable}
              maxLength={64}
              onChange={(event) => onRename(theme.id, event.target.value)}
              value={theme.name}
            />
            {editable && !theme.name.trim() ? <span className="text-[11px] text-danger">Theme name is required.</span> : null}
          </label>
          <div className="flex gap-1.5">
            <Button compact icon={<Copy size={13} />} onClick={() => onDuplicate(theme.id)}>Duplicate</Button>
            <Button
              aria-label="Delete theme"
              compact
              disabled={!editable}
              icon={<Trash2 size={13} />}
              onClick={() => onDelete(theme.id)}
              tone="danger"
              title={editable ? "Delete theme" : "Built-in themes cannot be deleted"}
            />
          </div>
        </div>

        <ThemeSpecimen theme={theme} />
        {theme.built_in ? (
          <p className="mt-2 rounded-[5px] border border-border bg-background/45 px-2.5 py-2 text-[12px] text-muted">
            Built-in preset. Duplicate it to create an editable version.
          </p>
        ) : null}

        <SectionHeading>Interface colors</SectionHeading>
        <div className="grid grid-cols-3 gap-1.75 max-[960px]:grid-cols-2">
          {COLOR_FIELDS.map(([field, label]) => (
            <label
              className={cx(
                "grid grid-cols-[25px_minmax(0,1fr)] grid-rows-[16px_12px] items-center gap-x-1.75 rounded-[5px] border border-border bg-background/55 p-1.5",
                !editable && "opacity-65",
              )}
              key={field}
            >
              <input
                aria-label={label}
                className="row-span-2 size-6.25 cursor-pointer rounded border-0 bg-transparent p-0 disabled:cursor-not-allowed"
                disabled={!editable}
                onChange={(event) => onColorChange(field, event.target.value)}
                type="color"
                value={theme.colors[field]}
              />
              <span className="overflow-hidden text-ellipsis whitespace-nowrap text-[12px] text-foreground">{label}</span>
              <code className="text-[11px] text-muted">{theme.colors[field]}</code>
            </label>
          ))}
        </div>

        <SectionHeading className="flex items-center justify-between gap-3">
          Graph lanes
          <Button
            compact
            disabled={!editable}
            icon={<RotateCcw size={13} />}
            onClick={() => onPaletteChange([...defaultPalette])}
            title="Restore default lane colors"
          >
            Reset lanes
          </Button>
        </SectionHeading>
        <div className="flex flex-wrap gap-1.75">
          {lanes.map((color, index) => (
            <input
              aria-label={`Graph lane ${index + 1}`}
              className="size-7.75 cursor-pointer overflow-hidden rounded-full border border-border bg-transparent p-0 disabled:cursor-not-allowed disabled:opacity-65"
              disabled={!editable}
              key={index}
              onChange={(event) => {
                const palette = [...lanes];
                palette[index] = event.target.value;
                onPaletteChange(palette);
              }}
              type="color"
              value={color}
            />
          ))}
        </div>
      </section>
    </div>
  );
}
