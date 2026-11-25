import assert from "node:assert/strict";
import test from "node:test";

import { buildIntralineMap, segmentLine, wordDiff } from "../src/components/diff/intraline";
import { buildDiffMap, toSplitRows } from "../src/components/diff/rows";
import type { DiffHunk, DiffLine } from "../src/lib/types";

function del(old_line: number, content: string): DiffLine {
  return { kind: "deletion", old_line, new_line: null, content };
}
function add(new_line: number, content: string): DiffLine {
  return { kind: "addition", old_line: null, new_line, content };
}
function ctx(line: number, content: string): DiffLine {
  return { kind: "context", old_line: line, new_line: line, content };
}
const NO_NEWLINE: DiffLine = { kind: "no_newline", old_line: null, new_line: null, content: "\ No newline at end of file" };

test("a missing newline on the old side does not split the removed lines from their replacements", () => {
  const lines = [
    ctx(11, "    ingredients:"),
    del(12, "      - {item: 'minecraft:copper_ingot', amount: 7}"),
    del(13, "      - {item: 'minecraft:iron_ingot', amount: 1}"),
    del(14, "  rift-item-rarities: {rarity: uncommon}"),
    NO_NEWLINE,
    add(12, "      - {item: 'minecraft:raw_copper', amount: 7}"),
    add(13, "      - {item: 'minecraft:raw_iron', amount: 1}"),
    add(14, "  rift-item-rarities: {rarity: uncommon}"),
  ];

  const rows = toSplitRows(lines);
  assert.equal(rows.length, 4);
  assert.deepEqual(rows.slice(1).map((row) => [row.left?.old_line, row.right?.new_line]), [[12, 12], [13, 13], [14, 14]]);
});

test("no-newline markers take no row in any view", () => {
  const hunk = { header: "", old_start: 1, old_count: 1, new_start: 1, new_count: 1, lines: [del(1, "a"), NO_NEWLINE, add(1, "b"), NO_NEWLINE] } as DiffHunk;
  assert.equal(toSplitRows(hunk.lines).length, 1);
  assert.equal(buildDiffMap([hunk], "inline").totalRows, 2);
});

test("the changed word is highlighted on both sides", () => {
  const before = "      - {item: 'minecraft:copper_ingot', amount: 7}";
  const after = "      - {item: 'minecraft:raw_copper', amount: 7}";
  const diff = wordDiff(before, after);
  assert.ok(diff);
  assert.deepEqual(diff.old.map(([s, e]) => before.slice(s, e)), ["copper_ingot"]);
  assert.deepEqual(diff.new.map(([s, e]) => after.slice(s, e)), ["raw_copper"]);
});

test("an identical line or an unrelated rewrite gets no word highlight", () => {
  assert.equal(wordDiff("same", "same"), null);
  assert.equal(wordDiff("let total = count;", "return fetchAll();"), null);
});

test("only paired lines are highlighted", () => {
  const removed = del(1, "value = 1");
  const added = add(1, "value = 2");
  const extra = add(2, "value = 3");
  const hunk = { header: "", old_start: 1, old_count: 1, new_start: 1, new_count: 2, lines: [removed, added, extra] } as DiffHunk;
  const map = buildIntralineMap([hunk]);
  assert.deepEqual(map.get(removed), [[8, 9]]);
  assert.deepEqual(map.get(added), [[8, 9]]);
  assert.equal(map.has(extra), false);
});

test("syntax tokens are cut at the change boundaries", () => {
  const segments = segmentLine(
    "foo(bar)",
    [{ text: "foo", cls: "function" }, { text: "(bar)", cls: "punctuation" }],
    [[2, 5]],
  );
  assert.deepEqual(segments, [
    { text: "fo", cls: "function", changed: false },
    { text: "o", cls: "function", changed: true },
    { text: "(b", cls: "punctuation", changed: true },
    { text: "ar)", cls: "punctuation", changed: false },
  ]);
});

// The WikiLink rewrite from the GitKraken comparison: Git keeps old line 5 (blank)
// as context against new line 13, which splits the rewrite in two.
function wikiLinkHunk(): DiffHunk {
  const oldLines = [
    "", "/** Link to an article by its stable id. The validator checks every id at build time. */",
    "export function WikiLink({", "  id,", "  anchor,", "  children,", "}: {", "  id: Id;",
    "  anchor?: string;", "  children?: ReactNode;", "}) {",
  ];
  const newLines = [
    'import { CategoryLink } from "./category-link";', "",
    "type WikiLinkProps = { children?: ReactNode } & (",
    "  { id: Id; category?: never; anchor?: string } | { category: Id; id?: never; anchor?: never }",
    ");", "", "export function WikiLink({ id, category, anchor, children }: WikiLinkProps) {",
    "  if (category !== undefined) return <CategoryLink id={category}>{children}</CategoryLink>;",
  ];
  const lines: DiffLine[] = [ctx(4, 'import type { Id } from "@/lib/model/common";')];
  lines[0] = { ...lines[0], old_line: 4, new_line: 4 };
  newLines.forEach((content, index) => lines.push(add(5 + index, content)));
  lines.push({ kind: "context", old_line: 5, new_line: 13, content: "" });
  oldLines.slice(1).forEach((content, index) => lines.push(del(6 + index, content)));
  lines.push({ kind: "context", old_line: 16, new_line: 14, content: "  const target = requireArticle(id);" });
  return { header: "", old_start: 4, old_count: 13, new_start: 4, new_count: 11, lines };
}

test("a lone blank line between two changes does not split the rewrite", () => {
  const rows = toSplitRows(wikiLinkHunk().lines);
  const numbers = rows.map((row) => [row.left?.old_line ?? null, row.right?.new_line ?? null]);
  assert.deepEqual(numbers, [
    [4, 4],
    [5, 5], [6, 6],
    [null, 7], [null, 8], [null, 9], [null, 10],
    [7, 11], [8, 12], [9, 13],
    [10, null], [11, null], [12, null], [13, null], [14, null], [15, null],
    [16, 14],
  ]);
  assert.equal(rows[0].change, false);
  assert.ok(rows.slice(1, -1).every((row) => row.change));
  // The absorbed blank line keeps its kind, so no line action targets it.
  assert.equal(rows[1].left?.kind, "context");
  assert.equal(rows[9].right?.kind, "context");
});

test("the absorbed gap is counted as a change in the split minimap", () => {
  const map = buildDiffMap([wikiLinkHunk()], "split");
  assert.deepEqual(map.marks.map((mark) => [mark.kind, mark.start, mark.length]), [
    ["change", 1, 2], ["addition", 3, 4], ["change", 7, 3], ["deletion", 10, 6],
  ]);
});

test("a gap with real content, or between two small changes, stays context", () => {
  const brace = [del(1, "a = 1"), add(1, "a = 2"), ctx(2, "keepThisLine();"), del(3, "b = 1"), add(3, "b = 2")];
  assert.equal(toSplitRows(brace).filter((row) => !row.change).length, 1);
  const small = [del(1, "a = 1"), add(1, "a = 2"), ctx(2, ""), del(3, "b = 1"), add(3, "b = 2")];
  assert.equal(toSplitRows(small).filter((row) => !row.change).length, 1);
});

test("an edited line is aligned with its new version, not with whatever sits at its index", () => {
  const lines = [
    del(1, "function load(path) {"),
    add(1, "// Reads the cache first."),
    add(2, "const cache = new Map();"),
    add(3, "function load(path, options) {"),
  ];
  assert.deepEqual(
    toSplitRows(lines).map((row) => [row.left?.old_line ?? null, row.right?.new_line ?? null]),
    [[null, 1], [null, 2], [1, 3]],
  );
  const map = buildIntralineMap([{ header: "", old_start: 1, old_count: 1, new_start: 1, new_count: 3, lines } as DiffHunk]);
  assert.deepEqual(map.get(lines[3])?.map(([start, end]) => lines[3].content.slice(start, end)), [", options"]);
});
