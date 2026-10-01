import assert from "node:assert/strict";
import test from "node:test";

import {
  buildGraphGeometry,
  getCommitGraphRowWindow,
  pathsInRowWindow,
  type GraphPath,
} from "../src/components/CommitGraph";
import type { CommitSummary } from "../src/lib/types";

const ROW_STRIDE = 29;

function commit(oid: string, parents: string[], lane = 0): CommitSummary {
  return {
    oid,
    short_oid: oid,
    subject: oid,
    body_preview: "",
    author: { name: "a", email: "a@example.com" },
    committer: { name: "a", email: "a@example.com" },
    authored_at: { seconds: 0, offset_minutes: 0 },
    committed_at: { seconds: 0, offset_minutes: 0 },
    parent_oids: parents,
    decorations: [],
    graph: {
      lane,
      edges: parents.map((parent) => ({
        parent_oid: parent,
        from_lane: lane,
        to_lane: lane,
        merge: false,
      })),
    },
    stash: null,
  } as unknown as CommitSummary;
}

function path(key: string, fromRow: number, toRow: number): GraphPath {
  return { key, data: "", color: 0, paintLane: 0, merge: false, stash: false, fromRow, toRow };
}

test("the row window covers the viewport with overscan on both sides", () => {
  const top = 300 * ROW_STRIDE;
  const bottom = top + 30 * ROW_STRIDE;
  const window = getCommitGraphRowWindow(top, bottom, 2_000);

  assert.ok(window.first <= 300 - 20, `first ${window.first}`);
  assert.ok(window.last >= 330 + 20, `last ${window.last}`);
  // Bounded: a long history must not leak into the mounted rows.
  assert.ok(window.last - window.first < 140, `size ${window.last - window.first}`);
});

test("scrolling inside a chunk keeps the same window", () => {
  const at = (row: number) => getCommitGraphRowWindow(row * ROW_STRIDE, (row + 30) * ROW_STRIDE, 2_000);

  assert.deepEqual(at(301), at(305));
});

test("the row window is clamped to the history", () => {
  assert.deepEqual(getCommitGraphRowWindow(0, 600, 5), { first: 0, last: 4 });
  assert.deepEqual(getCommitGraphRowWindow(0, 600, 0), { first: 0, last: -1 });
  // A viewport past the end of a history that just got shorter.
  const past = getCommitGraphRowWindow(10_000 * ROW_STRIDE, 10_030 * ROW_STRIDE, 50);
  assert.equal(past.last, 49);
  assert.ok(past.first <= past.last);
  // A list that starts below the viewport, under the WIP row.
  assert.equal(getCommitGraphRowWindow(-40, 400, 100).first, 0);
});

test("a route crossing the window is kept even when both ends are outside it", () => {
  const paths = [
    path("above", 0, 10),
    path("spanning", 5, 500),
    path("inside", 210, 211),
    path("below", 400, 401),
    path("entering", 250, 300),
  ];

  assert.deepEqual(
    pathsInRowWindow(paths, { first: 200, last: 260 }).map((candidate) => candidate.key),
    ["spanning", "inside", "entering"],
  );
});

test("filtering keeps the paint order of the routes", () => {
  const paths = [path("c", 0, 9), path("a", 0, 9), path("b", 0, 9)];

  assert.deepEqual(pathsInRowWindow(paths, { first: 3, last: 4 }).map((candidate) => candidate.key), ["c", "a", "b"]);
});

test("a route to an unloaded parent runs to the end of the list", () => {
  const commits = [commit("a", ["b"]), commit("b", ["c"])];
  const geometry = buildGraphGeometry(commits, false);
  const byKey = new Map(geometry.paths.map((candidate) => [candidate.key, candidate]));

  assert.deepEqual(
    [byKey.get("a:b:0")?.fromRow, byKey.get("a:b:0")?.toRow],
    [0, 1],
  );
  assert.deepEqual(
    [byKey.get("b:c:0")?.fromRow, byKey.get("b:c:0")?.toRow],
    [1, commits.length],
  );
});
