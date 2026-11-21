import type { DiffHunk, DiffLine } from "../../lib/types";
import type { DiffViewMode } from "./DiffParts";
import { hunkParts } from "./intraline";

export interface SplitRow {
  left: DiffLine | null;
  right: DiffLine | null;
}

export type MarkKind = "addition" | "deletion" | "change";

export interface DiffMark {
  kind: MarkKind;
  start: number;
  length: number;
}

export interface DiffMap {
  marks: DiffMark[];
  totalRows: number;
}

// Pairs each run of deletions with the additions that replaced it, so the two
// sides line up even when the runs have different lengths.
export function toSplitRows(lines: readonly DiffLine[]): SplitRow[] {
  const rows: SplitRow[] = [];

  for (const part of hunkParts(lines)) {
    if (part.kind === "context") {
      rows.push({ left: part.line, right: part.line });
      continue;
    }

    const { deletions, additions } = part.block;
    const rowCount = Math.max(deletions.length, additions.length);
    for (let pairIndex = 0; pairIndex < rowCount; pairIndex += 1) {
      rows.push({ left: deletions[pairIndex] ?? null, right: additions[pairIndex] ?? null });
    }
  }

  return rows;
}

const TAB_WIDTH = 4;

function charWidth(char: string): number {
  const code = char.codePointAt(0) ?? 0;
  if (code < 0x1100) return 1;
  return code <= 0x115f
    || (code >= 0x2e80 && code <= 0xa4cf)
    || (code >= 0xac00 && code <= 0xd7a3)
    || (code >= 0xf900 && code <= 0xfaff)
    || (code >= 0xfe30 && code <= 0xfe6f)
    || (code >= 0xff00 && code <= 0xff60)
    || (code >= 0x1f300 && code <= 0x1faff)
    ? 2
    : 1;
}

function visualWidth(content: string): number {
  let width = 0;
  for (const char of content) {
    width += char === "\t" ? TAB_WIDTH - (width % TAB_WIDTH) : charWidth(char);
  }
  return width;
}

export function maxLineWidth(hunks: readonly DiffHunk[]): number {
  let widest = 0;

  for (const hunk of hunks) {
    for (const line of hunk.lines) {
      if (line.kind === "no_newline") continue;
      const width = visualWidth(line.content);
      if (width > widest) widest = width;
    }
  }

  return widest;
}

function rowKinds(hunks: readonly DiffHunk[], mode: DiffViewMode): (MarkKind | null)[] {
  const kinds: (MarkKind | null)[] = [];

  for (const hunk of hunks) {
    if (mode === "split") {
      for (const row of toSplitRows(hunk.lines)) {
        const removed = row.left?.kind === "deletion";
        const added = row.right?.kind === "addition";
        kinds.push(removed && added ? "change" : removed ? "deletion" : added ? "addition" : null);
      }
      continue;
    }

    for (const line of hunk.lines) {
      if (line.kind === "no_newline") continue;
      kinds.push(
        line.kind === "addition" ? "addition" : line.kind === "deletion" ? "deletion" : null,
      );
    }
  }

  return kinds;
}

export function buildDiffMap(hunks: readonly DiffHunk[], mode: DiffViewMode): DiffMap {
  const kinds = rowKinds(hunks, mode);
  const marks: DiffMark[] = [];

  for (let index = 0; index < kinds.length; index += 1) {
    const kind = kinds[index];
    if (!kind) continue;

    const start = index;
    while (index + 1 < kinds.length && kinds[index + 1] === kind) index += 1;
    marks.push({ kind, start, length: index - start + 1 });
  }

  return { marks, totalRows: kinds.length };
}
