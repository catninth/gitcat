import type { DiffHunk, DiffLine } from "../../lib/types";

/** A half-open `[start, end)` range of characters that changed within a line. */
export type ChangeRange = readonly [number, number];

export type IntralineMap = ReadonlyMap<DiffLine, readonly ChangeRange[]>;

/**
 * A run of changed lines. `deletions` holds the old side and `additions` the
 * new side. A short unchanged gap between two changes (see `joinsChanges`) is
 * absorbed into the block, so either list may also hold `context` lines --
 * they appear once on each side, in order.
 */
export interface ChangeBlock {
  deletions: DiffLine[];
  additions: DiffLine[];
}

export type HunkPart =
  | { kind: "context"; line: DiffLine }
  | { kind: "change"; block: ChangeBlock };

const visibleLength = (text: string) => text.replace(/\s+/g, "").length;

// Git keeps a lone blank line (or a stray brace) unchanged between two
// changes, which splits one rewrite into an addition run, a context row and a
// deletion run. Read as a diff that is correct; side by side it pushes the two
// halves of the rewrite apart. The gap is shown as part of the change when it
// holds at most four visible characters and one of its neighbours is larger
// than five lines -- the rule Monaco's diff editor (and GitKraken with it) uses.
function joinsChanges(before: ChangeBlock, gap: readonly DiffLine[], after: ChangeBlock): boolean {
  const gapChars = gap.reduce((sum, line) => sum + visibleLength(line.content), 0);
  return gapChars <= 4
    && (before.deletions.length + before.additions.length > 5
      || after.deletions.length + after.additions.length > 5);
}

// Splits a hunk into context lines and runs of deletions/additions.
// "\ No newline at end of file" markers are dropped: one can sit between the
// deletions and additions of a run, and must not split it, or the removed and
// added lines stop pairing up.
export function hunkParts(lines: readonly DiffLine[]): HunkPart[] {
  const raw: HunkPart[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];

    if (line.kind === "context") {
      raw.push({ kind: "context", line });
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
    raw.push({ kind: "change", block });
  }

  const parts: HunkPart[] = [];
  for (let at = 0; at < raw.length; at += 1) {
    const part = raw[at];
    const last = parts[parts.length - 1];
    if (part.kind === "change" || last?.kind !== "change") {
      parts.push(part);
      continue;
    }

    let end = at;
    const gap: DiffLine[] = [];
    while (end < raw.length && raw[end].kind === "context") {
      gap.push((raw[end] as { kind: "context"; line: DiffLine }).line);
      end += 1;
    }
    const next = raw[end];
    if (next?.kind === "change" && joinsChanges(last.block, gap, next.block)) {
      last.block.deletions.push(...gap, ...next.block.deletions);
      last.block.additions.push(...gap, ...next.block.additions);
      at = end;
      continue;
    }
    parts.push(part);
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
  // Two lines that share only a bracket or two were rewritten, not edited.
  const shared = Math.min(sharedChars(a, keepA), sharedChars(b, keepB));
  if (shared === 0 || shared * 4 < Math.min(visibleLength(before), visibleLength(after))) return null;

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

// Characters of the longest common token subsequence of two lines, whitespace
// excluded. Blank lines share nothing, so they never anchor an alignment.
function sharedTokenChars(a: readonly string[], b: readonly string[]): number {
  if (a.length === 0 || b.length === 0 || (a.length + 1) * (b.length + 1) > MAX_CELLS) return 0;
  const width = b.length + 1;
  const table = new Uint32Array((a.length + 1) * width);
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      table[i * width + j] = a[i] === b[j]
        ? table[(i + 1) * width + j + 1] + a[i].length
        : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }
  return table[0];
}

// Above this many line pairs a block is paired by position: the alignment
// would cost more than it is worth on a rewrite that large.
const MAX_ALIGN_PAIRS = 40_000;
// A pair anchors the alignment only when the lines share at least this
// fraction of their visible characters (Dice coefficient).
const ANCHOR_SIMILARITY = 0.4;

export type AlignedPair = readonly [DiffLine | null, DiffLine | null];

function pairByPosition(old: readonly DiffLine[], next: readonly DiffLine[], into: AlignedPair[]) {
  const count = Math.max(old.length, next.length);
  for (let index = 0; index < count; index += 1) into.push([old[index] ?? null, next[index] ?? null]);
}

/**
 * Lays the two sides of a change block next to each other. Lines that are
 * clearly the same line edited are put on one row (the anchors); the lines
 * between two anchors pair up by position, and the shorter side is padded at
 * the end of that stretch. The anchors are the pairs of the line alignment
 * that keeps the most characters in common, so one similar-looking line
 * cannot pull the whole block out of order.
 */
export function alignBlock(block: ChangeBlock): AlignedPair[] {
  const old = block.deletions;
  const next = block.additions;
  const rows: AlignedPair[] = [];
  if (old.length === 0 || next.length === 0 || old.length * next.length > MAX_ALIGN_PAIRS) {
    pairByPosition(old, next, rows);
    return rows;
  }

  // An absorbed context line is not an edit of anything; anchoring it to its
  // own copy on the other side would split the block apart again.
  const tokens = (line: DiffLine) => line.kind === "context"
    ? []
    : tokenize(line.content).filter((token) => token.trim() !== "");
  const oldTokens = old.map(tokens);
  const nextTokens = next.map(tokens);
  const width = next.length + 1;
  const weight = new Uint32Array(old.length * next.length);
  for (let i = 0; i < old.length; i += 1) {
    for (let j = 0; j < next.length; j += 1) {
      weight[i * next.length + j] = sharedTokenChars(oldTokens[i], nextTokens[j]);
    }
  }

  const best = new Uint32Array((old.length + 1) * width);
  for (let i = old.length - 1; i >= 0; i -= 1) {
    for (let j = next.length - 1; j >= 0; j -= 1) {
      const pair = weight[i * next.length + j];
      best[i * width + j] = Math.max(
        best[(i + 1) * width + j],
        best[i * width + j + 1],
        pair > 0 ? pair + best[(i + 1) * width + j + 1] : 0,
      );
    }
  }

  let fromOld = 0;
  let fromNext = 0;
  const anchor = (i: number, j: number) => {
    pairByPosition(old.slice(fromOld, i), next.slice(fromNext, j), rows);
    rows.push([old[i], next[j]]);
    fromOld = i + 1;
    fromNext = j + 1;
  };

  for (let i = 0, j = 0; i < old.length && j < next.length;) {
    const pair = weight[i * next.length + j];
    if (pair > 0 && best[i * width + j] === pair + best[(i + 1) * width + j + 1]) {
      const similarity = (2 * pair) / (visibleLength(old[i].content) + visibleLength(next[j].content));
      if (similarity >= ANCHOR_SIMILARITY) anchor(i, j);
      i += 1;
      j += 1;
    } else if (best[(i + 1) * width + j] >= best[i * width + j + 1]) {
      i += 1;
    } else {
      j += 1;
    }
  }
  pairByPosition(old.slice(fromOld), next.slice(fromNext), rows);

  return rows;
}

/** Changed character ranges for every removed/added line that has a partner. */
export function buildIntralineMap(hunks: readonly DiffHunk[]): IntralineMap {
  const map = new Map<DiffLine, readonly ChangeRange[]>();

  for (const hunk of hunks) {
    for (const part of hunkParts(hunk.lines)) {
      if (part.kind !== "change") continue;
      for (const [removed, added] of alignBlock(part.block)) {
        if (removed?.kind !== "deletion" || added?.kind !== "addition") continue;
        const diff = wordDiff(removed.content, added.content);
        if (!diff) continue;
        if (diff.old.length > 0) map.set(removed, diff.old);
        if (diff.new.length > 0) map.set(added, diff.new);
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
