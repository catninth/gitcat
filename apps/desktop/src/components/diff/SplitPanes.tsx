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
      <table className={`gc-diff-table gc-diff-table--side${showHeader ? "" : " gc-diff-table--headless"}`}>
        <caption className="sr-only">
          {SIDE_LABEL[side]} side of the diff: lines {start}–{start + Math.max(0, count - 1)}
        </caption>
        <thead className="gc-diff-table__head">
          <tr>
            <th className="gc-diff-table__line-heading" scope="col">Line</th>
            <th className="gc-diff-table__side-heading" scope="col">{SIDE_LABEL[side]}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row, rowIndex) => {
            if (row.marker) {
              return (
                <tr className="gc-diff-line gc-diff-line--no-newline" key={`marker:${rowIndex}`}>
                  <td className="gc-diff-line__content" colSpan={2}>
                    <LineContent line={row.marker} />
                  </td>
                </tr>
              );
            }

            const line = side === "old" ? row.left : row.right;
            const number = side === "old" ? line?.old_line : line?.new_line;
            const rowKind = row.left?.kind === "deletion" || row.right?.kind === "addition"
              ? "change"
              : "context";
            const kindClass = line ? ` gc-diff-line__content--${line.kind}` : " gc-diff-line__content--empty";

            return (
              <tr
                className={`gc-diff-line gc-diff-line--${rowKind}`}
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
                  {line ? <LineContent line={line} /> : null}
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

    let driver: HTMLDivElement | null = null;
    let frame = 0;

    const follow = (from: HTMLDivElement, to: HTMLDivElement) => () => {
      if (driver && driver !== from) return;
      driver = from;
      to.scrollTop = from.scrollTop;
      to.scrollLeft = from.scrollLeft;
      if (frame) cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        driver = null;
        frame = 0;
      });
    };

    const onLeft = follow(left, right);
    const onRight = follow(right, left);
    left.addEventListener("scroll", onLeft, { passive: true });
    right.addEventListener("scroll", onRight, { passive: true });

    return () => {
      if (frame) cancelAnimationFrame(frame);
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
