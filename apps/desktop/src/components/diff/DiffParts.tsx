import type { ReactNode } from "react";

import { cx } from "../../lib";
import type { DiffLine, DiffViewMode } from "../../lib/types";
import { fileStatusClass, fileStatusIcon } from "../file-tree";
import { useHighlightedLine } from "./highlight";

export type { DiffViewMode };

export const DIFF_VIEW_MODES: readonly DiffViewMode[] = ["hunk", "inline", "split"];

export function isWholeFileMode(mode: DiffViewMode): boolean {
  return mode !== "hunk";
}

export function linePrefix(kind: DiffLine["kind"]): string {
  switch (kind) {
    case "addition":
      return "+";
    case "deletion":
      return "−";
    case "context":
      return " ";
    case "no_newline":
      return "";
  }
}

export function displayLineNumber(value: number | null): string {
  return value === null ? "" : String(value);
}

export function LineContent({ line }: { line: DiffLine }) {
  const tokens = useHighlightedLine(line);

  return (
    <>
      <span aria-hidden="true" className="gc-diff-line__prefix">
        {linePrefix(line.kind)}
      </span>
      <code className="gc-diff-line__code">
        {tokens === null
          ? line.content || " "
          : tokens.map((token, index) => (
            <span className={`gc-tok gc-tok--${token.cls}`} key={index}>
              {token.text}
            </span>
          ))}
      </code>
    </>
  );
}

// One hunk of a file diff; the label ties the table to its @@ range for AT.
export function HunkSection({ label, fallbackLabel, children }: {
  label?: string;
  fallbackLabel?: string;
  children: ReactNode;
}) {
  return (
    <section
      aria-label={label ? undefined : fallbackLabel}
      aria-labelledby={label}
      className="w-full min-w-full"
    >
      {children}
    </section>
  );
}

// Sticks to the top of the scroller so the @@ range stays visible while reading
// a long hunk.
export function HunkHeader({ id, children }: { id: string; children: string }) {
  return (
    <h3
      className="sticky top-0 z-3 m-0 border-y border-[color-mix(in_srgb,var(--gc-accent)_30%,var(--gc-border))] bg-[color-mix(in_srgb,var(--gc-accent)_9%,var(--gc-panel))] px-2.75 py-1.5 text-[10px] font-medium text-[color-mix(in_srgb,var(--gc-accent)_72%,var(--gc-text))]"
      id={id}
    >
      <code>{children}</code>
    </h3>
  );
}

// The same glyph the file list shows for this change, so the header reads as
// the row that was clicked.
export function ChangeKind({ status }: { status: string }) {
  const Icon = fileStatusIcon(status);
  const label = status.replaceAll("_", " ");
  return (
    <span aria-label={label} className={cx("shrink-0", fileStatusClass(status))} role="img" title={label}>
      <Icon aria-hidden="true" size={12} strokeWidth={2.6} />
    </span>
  );
}

export function ModeButton({ active, children, mode, onSelect }: {
  active: boolean;
  children: string;
  mode: DiffViewMode;
  onSelect: (mode: DiffViewMode) => void;
}) {
  return (
    <button
      aria-pressed={active}
      className={cx(
        "cursor-pointer rounded-[3px] bg-transparent px-2.25 py-1.25 text-[10px] font-bold",
        active ? "bg-[color-mix(in_srgb,var(--gc-accent)_17%,var(--gc-panel))] text-accent" : "text-muted",
      )}
      data-diff-mode={mode}
      onClick={() => onSelect(mode)}
      type="button"
    >
      {children}
    </button>
  );
}

export function DiffState({ children }: { children: string }) {
  return (
    <div className="grid min-h-45 flex-1 place-items-center text-muted" role="status">
      {children}
    </div>
  );
}
