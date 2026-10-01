import { Minus, Plus } from "lucide-react";
import { createContext, useContext } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";

import { cx } from "../../lib";
import type { DiffLine, LinePatchAction } from "../../lib/types";

/**
 * What a worktree diff lets the user do with one of its lines. `side` is the
 * half of the worktree the diff shows: unstaged lines can be staged or
 * discarded, staged ones unstaged. A commit diff has no line actions at all.
 */
export interface DiffLineActions {
  side: "unstaged" | "staged";
  /** A line change is running; line numbers on screen are about to move. */
  busy: boolean;
  onApply: (action: LinePatchAction, lines: DiffLine[]) => void;
}

interface LineActionsContextValue {
  actions: DiffLineActions | null;
  openMenu: (event: ReactMouseEvent, line: DiffLine) => void;
}

export const LineActionsContext = createContext<LineActionsContextValue | null>(null);

export function isChangedLine(line: DiffLine | null | undefined): line is DiffLine {
  return line?.kind === "addition" || line?.kind === "deletion";
}

export function useLineActions() {
  return useContext(LineActionsContext);
}

// Every line offers at least Copy, so the menu is the diff's own even where
// there is nothing to stage.
export function lineMenuHandler(
  context: LineActionsContextValue | null,
  line: DiffLine | null | undefined,
) {
  if (!context || !line || line.kind === "no_newline") return undefined;
  return (event: ReactMouseEvent) => context.openMenu(event, line);
}

// Sits in the line-number cell and shows on row hover, like GitKraken's.
export function LineGutterAction({ line }: { line: DiffLine | null | undefined }) {
  const actions = useContext(LineActionsContext)?.actions;
  if (!actions || !isChangedLine(line)) return null;

  const staging = actions.side === "unstaged";
  const label = staging ? "Stage this line" : "Unstage this line";
  const Icon = staging ? Plus : Minus;

  return (
    <button
      aria-label={label}
      className={cx("gc-diff-line__action", staging ? "gc-diff-line__action--stage" : "gc-diff-line__action--unstage")}
      disabled={actions.busy}
      onClick={(event) => {
        event.stopPropagation();
        actions.onApply(staging ? "stage" : "unstage", [line]);
      }}
      onMouseDown={(event) => event.preventDefault()}
      title={label}
      type="button"
    >
      <Icon aria-hidden="true" size={10} strokeWidth={3.5} />
    </button>
  );
}
