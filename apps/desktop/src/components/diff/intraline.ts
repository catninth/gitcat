import type { DiffHunk, DiffLine } from "../../lib/types";

/** A half-open `[start, end)` range of characters that changed within a line. */
export type ChangeRange = readonly [number, number];

export type IntralineMap = ReadonlyMap<DiffLine, readonly ChangeRange[]>;

export interface ChangeBlock {
  deletions: DiffLine[];
  additions: DiffLine[];
}

export type HunkPart =
  | { kind: "context"; line: DiffLine }
  | { kind: "change"; block: ChangeBlock };

// Splits a hunk into context lines and runs of deletions/additions.
// "\ No newline at end of file" markers are dropped: one can sit between the
// deletions and additions of a run, and must not split it, or the removed and
// added lines stop pairing up.
export function hunkParts(lines: readonly DiffLine[]): HunkPart[] {
  const parts: HunkPart[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];

    if (line.kind === "context") {
      parts.push({ kind: "context", line });
      index += 1;
      continue;
    }

    if (line.kind === "no_newline") {
      index += 1;
      continue;
    }

    const block: ChangeBlock = { deletions: [], additions: [] };
    while (index < lines.length && lines[index].kind !== "context") {
      const changed = lines[index];
      if (changed.kind === "deletion") block.deletions.push(changed);
      else if (changed.kind === "addition") block.additions.push(changed);
      index += 1;
    }
    parts.push({ kind: "change", block });
  }

  return parts;
}

const TOKEN = /[\p{L}\p{N}_]+|\s+|[^\p{L}\p{N}_\s]/gu;

function tokenize(text: string): string[] {
  return text.match(TOKEN) ?? [];
}

// Lines this long, or this different, are not worth an LCS table: the
// highlight would only say "everything changed".
const MAX_CELLS = 250_000;

/**
 * Word-level difference between a removed and an added line. Returns `null`
 * when the lines share nothing worth pointing at -- then the whole-line color
 * already says it all.
 */
export function wordDiff(
  before: string,
  after: string,
): { old: ChangeRange[]; new: ChangeRange[] } | null {
  if (before === after) return null;

  const a = tokenize(before);
  const b = tokenize(after);

  let prefix = 0;
  while (prefix < a.length && prefix < b.length && a[prefix] === b[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < a.length - prefix
    && suffix < b.length - prefix
    && a[a.length - 1 - suffix] === b[b.length - 1 - suffix]
  ) suffix += 1;

  const midA = a.slice(prefix, a.length - suffix);
  const midB = b.slice(prefix, b.length - suffix);
  if ((midA.length + 1) * (midB.length + 1) > MAX_CELLS) return null;

  // keepA[i] / keepB[j]: the token is part of the common subsequence.
  const keepA = new Array<boolean>(midA.length).fill(false);
  const keepB = new Array<boolean>(midB.length).fill(false);
  const width = midB.length + 1;
  const table = new Uint32Array((midA.length + 1) * width);
  for (let i = midA.length - 1; i >= 0; i -= 1) {
    for (let j = midB.length - 1; j >= 0; j -= 1) {
      table[i * width + j] = midA[i] === midB[j]
        ? table[(i + 1) * width + j + 1] + 1
        : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }
  for (let i = 0, j = 0; i < midA.length && j < midB.length;) {
    if (midA[i] === midB[j]) {
      keepA[i] = true;
      keepB[j] = true;
      i += 1;
      j += 1;
    } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
      i += 1;
    } else {
      j += 1;
    }
  }

  const sharedChars = (tokens: string[], keep: boolean[]) => {
    let chars = 0;
    tokens.forEach((token, index) => {
      const middle = index - prefix;
      if (middle < 0 || middle >= keep.length || keep[middle]) chars += token.trim().length;
    });
    return chars;
  };
  const visible = (text: string) => text.replace(/\s+/g, "").length;
  // Two lines that share only a bracket or two were rewritten, not edited.
  const shared = Math.min(sharedChars(a, keepA), sharedChars(b, keepB));
  if (shared === 0 || shared * 4 < Math.min(visible(before), visible(after))) return null;

  return {
    old: changedRanges(before, a, prefix, keepA),
    new: changedRanges(after, b, prefix, keepB),
  };
}

function changedRanges(text: string, tokens: string[], prefix: number, keep: boolean[]): ChangeRange[] {
  const ranges: [number, number][] = [];
  let offset = 0;
  for (let index = 0; index < prefix; index += 1) offset += tokens[index].length;

  keep.forEach((kept, index) => {
    const start = offset;
    offset += tokens[prefix + index].length;
    if (kept) return;
    const last = ranges[ranges.length - 1];
    // A single space between two changed words reads as one change.
    if (last && (last[1] === start || /^\s$/.test(text.slice(last[1], start)))) last[1] = offset;
    else ranges.push([start, offset]);
  });

  return ranges;
}

/** Changed character ranges for every removed/added line that has a partner. */
export function buildIntralineMap(hunks: readonly DiffHunk[]): IntralineMap {
  const map = new Map<DiffLine, readonly ChangeRange[]>();

  for (const hunk of hunks) {
    for (const part of hunkParts(hunk.lines)) {
      if (part.kind !== "change") continue;
      const { deletions, additions } = part.block;
      const pairs = Math.min(deletions.length, additions.length);
      for (let index = 0; index < pairs; index += 1) {
        const diff = wordDiff(deletions[index].content, additions[index].content);
        if (!diff) continue;
        if (diff.old.length > 0) map.set(deletions[index], diff.old);
        if (diff.new.length > 0) map.set(additions[index], diff.new);
      }
    }
  }

  return map;
}

export interface Segment {
  text: string;
  cls: string | null;
  changed: boolean;
}

/** Cuts syntax tokens (or the bare line) at the changed-range boundaries. */
export function segmentLine(
  content: string,
  tokens: readonly { text: string; cls: string }[] | null,
  ranges: readonly ChangeRange[] | undefined,
): Segment[] {
  const pieces = tokens ?? [{ text: content, cls: null as string | null }];
  if (!ranges || ranges.length === 0) {
    return pieces.map((piece) => ({ text: piece.text, cls: piece.cls, changed: false }));
  }

  const segments: Segment[] = [];
  let offset = 0;
  let rangeIndex = 0;

  for (const piece of pieces) {
    const end = offset + piece.text.length;
    let cursor = offset;
    while (cursor < end) {
      while (rangeIndex < ranges.length && ranges[rangeIndex][1] <= cursor) rangeIndex += 1;
      const range = ranges[rangeIndex];
      const inside = range !== undefined && range[0] <= cursor;
      const stop = Math.min(end, range === undefined ? end : inside ? range[1] : range[0]);
      const text = piece.text.slice(cursor - offset, stop - offset);
      const last = segments[segments.length - 1];
      if (last && last.cls === piece.cls && last.changed === inside) last.text += text;
      else segments.push({ text, cls: piece.cls, changed: inside });
      cursor = stop;
    }
    offset = end;
  }

  return segments;
}
