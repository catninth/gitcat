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
