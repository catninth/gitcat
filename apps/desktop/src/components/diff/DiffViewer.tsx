import { Copy, Minus, Plus, Undo2, X } from "lucide-react";
import { useCallback, useMemo, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent } from "react";

import { cx } from "../../lib";
import type { DiffLine, FileDiff } from "../../lib/types";
import { ContextMenu } from "../ContextMenu";
import type { ContextAction } from "../ContextMenu";
import { IconButton } from "../ui";
import { DiffMinimap } from "./DiffMinimap";
import { ChangeKind, DIFF_VIEW_MODES, DiffState, ModeButton, isWholeFileMode } from "./DiffParts";
import type { DiffViewMode } from "./DiffParts";
import { HighlightContext, useDiffHighlight } from "./highlight";
import { InlineHunk } from "./InlineHunk";
import { LineActionsContext, isChangedLine } from "./lineActions";
import type { DiffLineActions } from "./lineActions";
import { SplitPanes } from "./SplitPanes";
import { buildDiffMap, maxLineWidth } from "./rows";

export interface DiffViewerProps {
  diff: FileDiff | null;
  mode?: DiffViewMode;
  defaultMode?: DiffViewMode;
  loading?: boolean;
  className?: string;
  closeKeybind?: string;
  onClose?: () => void;
  onModeChange?: (mode: DiffViewMode) => void;
  /** Present only for a worktree diff, whose lines can be staged and discarded. */
  lineActions?: DiffLineActions | null;
}

interface LineMenu {
  x: number;
  y: number;
  line: DiffLine;
  // Taken when the menu opens: clicking an item would otherwise clear it.
  selection: string;
}

function lineMenuActions(line: DiffLine, actions: DiffLineActions | null | undefined): ContextAction[] {
  const items: ContextAction[] = [];
  if (actions && isChangedLine(line)) {
    if (actions.side === "unstaged") {
      items.push(
        { id: "discard", label: "Discard this line", icon: <Undo2 size={14} />, danger: true, disabled: actions.busy },
        { id: "stage", label: "Stage this line", icon: <Plus size={14} />, disabled: actions.busy },
      );
    } else {
      items.push({ id: "unstage", label: "Unstage this line", icon: <Minus size={14} />, disabled: actions.busy });
    }
  }
  items.push({ id: "copy", label: "Copy", icon: <Copy size={14} />, separatorBefore: items.length > 0 });
  return items;
}

export function DiffViewer({
  diff,
  mode: controlledMode,
  defaultMode = "hunk",
  loading = false,
  className,
  closeKeybind,
  onClose,
  onModeChange,
  lineActions,
}: DiffViewerProps) {
  const [internalMode, setInternalMode] = useState<DiffViewMode>(defaultMode);
  const [lineMenu, setLineMenu] = useState<LineMenu | null>(null);
  const mode = controlledMode ?? internalMode;
  const scrollRef = useRef<HTMLDivElement>(null);
  const diffMap = useMemo(
    () => buildDiffMap(diff?.hunks ?? [], mode),
    [diff, mode],
  );
  const contentColumns = useMemo(() => maxLineWidth(diff?.hunks ?? []), [diff]);
  const highlight = useDiffHighlight(diff);

  const openLineMenu = useCallback((event: ReactMouseEvent, line: DiffLine) => {
    event.preventDefault();
    setLineMenu({
      x: event.clientX,
      y: event.clientY,
      line,
      selection: window.getSelection()?.toString() ?? "",
    });
  }, []);

  const lineActionsContext = useMemo(
    () => ({ actions: lineActions ?? null, openMenu: openLineMenu }),
    [lineActions, openLineMenu],
  );

  const runLineMenuAction = (id: string) => {
    const menu = lineMenu;
    setLineMenu(null);
    if (!menu) return;
    if (id === "copy") {
      void navigator.clipboard?.writeText(menu.selection || menu.line.content);
    } else if (id === "stage" || id === "unstage" || id === "discard") {
      lineActions?.onApply(id, [menu.line]);
    }
  };

  const setMode = (nextMode: DiffViewMode) => {
    if (nextMode === mode) return;
    setInternalMode(nextMode);
    onModeChange?.(nextMode);
  };

  const handleModeKeys = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;

    event.preventDefault();
    const step = event.key === "ArrowLeft" ? -1 : 1;
    const current = DIFF_VIEW_MODES.indexOf(mode);
    const nextIndex = (current + step + DIFF_VIEW_MODES.length) % DIFF_VIEW_MODES.length;
    const nextMode = DIFF_VIEW_MODES[nextIndex];
    setMode(nextMode);
    event.currentTarget
      .querySelector<HTMLButtonElement>(`[data-diff-mode="${nextMode}"]`)
      ?.focus();
  };

  const rootClass = cx("flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background", className);

  if (loading) {
    return (
      <section aria-busy="true" aria-label="Diff viewer" className={rootClass}>
        <DiffState>Loading diff…</DiffState>
      </section>
    );
  }

  if (!diff) {
    return (
      <section aria-label="Diff viewer" className={rootClass}>
        <DiffState>Select a changed file to view its diff.</DiffState>
      </section>
    );
  }

  const oldPath = diff.old_path ?? diff.new_path;
  const renamed = diff.old_path !== null && diff.old_path !== diff.new_path;
  const showHunkHeaders = mode === "hunk" || diff.hunks.length > 1;
  const showMinimap = isWholeFileMode(mode) && !diff.binary && diffMap.marks.length > 0;

  return (
    <section aria-label={`Diff for ${diff.new_path}`} className={rootClass}>
      <header className="flex min-h-12 flex-[0_0_auto] items-center gap-3 border-b border-border bg-[color-mix(in_srgb,var(--gc-panel)_66%,var(--gc-background))] py-1.5 pl-3.25 pr-2.5">
        <div className="flex min-w-0 flex-1 items-center gap-1.75">
          <h2 className="m-0 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[12px] font-[550] leading-[1.4] text-foreground" title={diff.new_path}>
            {renamed ? `${oldPath} → ${diff.new_path}` : diff.new_path}
          </h2>
          <ChangeKind status={diff.status} />
          <span aria-label={`${diff.stats.additions} additions`} className="font-mono text-[10px] font-[650] text-success">
            +{diff.stats.additions}
          </span>
          <span aria-label={`${diff.stats.deletions} deletions`} className="font-mono text-[10px] font-[650] text-danger">
            −{diff.stats.deletions}
          </span>
          {diff.old_mode !== diff.new_mode ? (
            <span className="font-mono text-[10px] text-muted">
              {diff.old_mode ?? "none"} → {diff.new_mode ?? "none"}
            </span>
          ) : null}
        </div>
        <div
          aria-label="Diff layout"
          className="flex shrink-0 rounded-[5px] border border-border bg-background p-0.5"
          onKeyDown={handleModeKeys}
          role="group"
        >
          <ModeButton active={mode === "hunk"} mode="hunk" onSelect={setMode}>Hunk</ModeButton>
          <ModeButton active={mode === "inline"} mode="inline" onSelect={setMode}>Inline</ModeButton>
          <ModeButton active={mode === "split"} mode="split" onSelect={setMode}>Split</ModeButton>
        </div>
        {onClose ? (
          <IconButton
            aria-label="Back to graph"
            onClick={onClose}
            title={closeKeybind ? `Back to graph (${closeKeybind})` : "Back to graph"}
          >
            <X size={16} />
          </IconButton>
        ) : null}
      </header>

      {diff.truncated ? (
        <div className="border-b border-border bg-[color-mix(in_srgb,var(--gc-warning)_10%,var(--gc-panel))] px-3 py-2 text-[11px] text-warning" role="alert">
          Diff truncated at configured size limit.
        </div>
      ) : null}

      {diff.binary ? (
        <DiffState>Binary file. Text preview is unavailable.</DiffState>
      ) : diff.hunks.length === 0 ? (
        <DiffState>No text changes to display.</DiffState>
      ) : (
        <HighlightContext.Provider value={highlight}>
          <LineActionsContext.Provider value={lineActionsContext}>
          <div className="flex min-h-0 min-w-0 flex-1">
            {mode === "split" ? (
              <SplitPanes
                contentColumns={contentColumns}
                hunks={diff.hunks}
                leftRef={scrollRef}
                mapped={showMinimap}
                showHeaders={showHunkHeaders}
              />
            ) : (
              <div
                className={cx("min-w-0 flex-1 overflow-auto", showMinimap && "gc-diff-scroller--mapped")}
                ref={scrollRef}
                style={{ "--gc-diff-cols": contentColumns } as CSSProperties}
              >
                {diff.hunks.map((hunk, index) => (
                  <InlineHunk hunk={hunk} index={index} key={`${hunk.header}:${index}`} showHeader={showHunkHeaders} />
                ))}
              </div>
            )}
            {showMinimap ? <DiffMinimap map={diffMap} scrollRef={scrollRef} /> : null}
          </div>
          </LineActionsContext.Provider>
        </HighlightContext.Provider>
      )}
      {lineMenu ? (
        <ContextMenu
          actions={lineMenuActions(lineMenu.line, lineActions)}
          onAction={runLineMenuAction}
          onClose={() => setLineMenu(null)}
          x={lineMenu.x}
          y={lineMenu.y}
        />
      ) : null}
    </section>
  );
}
