import { memo, useEffect, useMemo, useRef } from "react";
import type { CSSProperties, RefObject } from "react";

import { cx } from "../../lib";
import type { DiffHunk } from "../../lib/types";
import { HunkHeader, HunkSection, LineContent, displayLineNumber } from "./DiffParts";
import { LineGutterAction, lineMenuHandler, useLineActions } from "./lineActions";
import { toSplitRows } from "./rows";
import type { SplitRow } from "./rows";

type Side = "old" | "new";

const SIDE_LABEL: Record<Side, string> = { old: "Before", new: "After" };

const SideHunk = memo(function SideHunk({ hunk, index, rows, showHeader, side }: {
  hunk: DiffHunk;
  index: number;
  rows: readonly SplitRow[];
  showHeader: boolean;
  side: Side;
}) {
  const actions = useLineActions();
  const start = side === "old" ? hunk.old_start : hunk.new_start;
  const count = side === "old" ? hunk.old_count : hunk.new_count;

  return (
    <HunkSection
      fallbackLabel={`Whole file, ${SIDE_LABEL[side].toLowerCase()}`}
      label={showHeader ? `gc-split-${side}-hunk-${index}` : undefined}
    >
      {showHeader ? <HunkHeader id={`gc-split-${side}-hunk-${index}`}>{hunk.header}</HunkHeader> : null}
      <table className="gc-diff-table gc-diff-table--side">
        <colgroup>
          <col className="gc-diff-table__line-column" />
          <col className="gc-diff-table__content-column" />
        </colgroup>
        <caption className="sr-only">
          {SIDE_LABEL[side]} side of the diff: lines {start}–{start + Math.max(0, count - 1)}
        </caption>
        <tbody>
          {rows.map((row, rowIndex) => {
            const line = side === "old" ? row.left : row.right;
            const number = side === "old" ? line?.old_line : line?.new_line;
            const rowKind = row.change ? "change" : "context";
            const shownKind = row.change ? (side === "old" ? "deletion" : "addition") : line?.kind;
            const kindClass = line ? ` gc-diff-line__content--${shownKind}` : " gc-diff-line__content--empty";

            return (
              <tr
                className={`gc-diff-line gc-diff-line--${rowKind}${line ? "" : " gc-diff-line--empty"}`}
                key={`line:${rowIndex}`}
                onContextMenu={lineMenuHandler(actions, line)}
              >
                <td
                  aria-label={number == null ? undefined : `${SIDE_LABEL[side]} line ${number}`}
                  className={`gc-diff-line__number gc-diff-line__number--${side}`}
                >
                  <LineGutterAction line={line} />
                  {displayLineNumber(number ?? null)}
                </td>
                <td className={`gc-diff-line__content gc-diff-line__content--${side}${kindClass}`}>
                  {line ? <LineContent kind={shownKind} line={line} /> : null}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </HunkSection>
  );
});

export function SplitPanes({ contentColumns, hunks, leftRef, mapped, showHeaders }: {
  contentColumns: number;
  hunks: readonly DiffHunk[];
  leftRef: RefObject<HTMLDivElement | null>;
  mapped: boolean;
  showHeaders: boolean;
}) {
  const rightRef = useRef<HTMLDivElement>(null);
  const rowsByHunk = useMemo(() => hunks.map((hunk) => toSplitRows(hunk.lines)), [hunks]);

  useEffect(() => {
    const left = leftRef.current;
    const right = rightRef.current;
    if (!left || !right) return;

    // The position each pane was last moved to by the other one. A pane's scroll
    // event arrives a frame after it was set, so echoing it back would drag the pane
    // the user is moving back to where it was a frame ago. Reading the value back
    // after writing it also captures the clamp when the panes' widths differ.
    const echoes = new WeakMap<HTMLDivElement, { top: number; left: number }>();

    const follow = (from: HTMLDivElement, to: HTMLDivElement) => () => {
      const echo = echoes.get(from);
      if (echo && echo.top === from.scrollTop && echo.left === from.scrollLeft) return;
      echoes.delete(from);
      if (to.scrollTop === from.scrollTop && to.scrollLeft === from.scrollLeft) return;
      to.scrollTop = from.scrollTop;
      to.scrollLeft = from.scrollLeft;
      echoes.set(to, { top: to.scrollTop, left: to.scrollLeft });
    };

    const onLeft = follow(left, right);
    const onRight = follow(right, left);
    left.addEventListener("scroll", onLeft, { passive: true });
    right.addEventListener("scroll", onRight, { passive: true });

    return () => {
      left.removeEventListener("scroll", onLeft);
      right.removeEventListener("scroll", onRight);
    };
  }, [leftRef, rowsByHunk]);

  const paneStyle = { "--gc-diff-cols": contentColumns } as CSSProperties;

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <div
        className="gc-diff-scroller--mapped min-w-0 flex-1 overflow-auto"
        ref={leftRef}
        style={paneStyle}
      >
        {hunks.map((hunk, index) => (
          <SideHunk
            hunk={hunk}
            index={index}
            key={`old:${hunk.header}:${index}`}
            rows={rowsByHunk[index]}
            showHeader={showHeaders}
            side="old"
          />
        ))}
      </div>
      <div
        className={cx(
          "min-w-0 flex-1 overflow-auto border-l border-border",
          mapped && "gc-diff-scroller--mapped",
        )}
        ref={rightRef}
        style={paneStyle}
      >
        {hunks.map((hunk, index) => (
          <SideHunk
            hunk={hunk}
            index={index}
            key={`new:${hunk.header}:${index}`}
            rows={rowsByHunk[index]}
            showHeader={showHeaders}
            side="new"
          />
        ))}
      </div>
    </div>
  );
}
