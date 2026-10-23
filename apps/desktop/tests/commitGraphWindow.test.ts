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

test("a merge joins the existing first-parent line before its lane converges", () => {
  const commits = [
    commit("master", ["base"], 1),
    commit("merge", ["feature", "base"], 2),
    commit("unrelated", ["base"], 0),
    commit("feature", [], 2),
    commit("base", [], 0),
  ];
  commits[0].graph.edges[0].to_lane = 0;
  commits[1].graph.edges[1] = {
    parent_oid: "base", from_lane: 2, to_lane: 0, merge: true,
  };
  const geometry = buildGraphGeometry(commits, false, {
    colors: new Map([["master", 1], ["merge", 2], ["base", 0]]),
  });
  const master = geometry.paths.find((route) => route.key === "master:base:0")!;
  const merge = geometry.paths.find((route) => route.key === "merge:base:1")!;

  assert.equal(merge.color, master.color, "the junction belongs to the master span");
  // Lane 1 is x=42; the unrelated row at lane 0 must not be on this route.
  assert.match(merge.data, /L 42 /);
  assert.ok(merge.data.endsWith(master.data.slice(master.data.indexOf(" Q "))));
});

for (const mirrored of [false, true]) {
  test(`a merge preserves the trunk's bend into an adjacent parent (${mirrored ? "right" : "left"})`, () => {
    // WIP reserves lane 0 for base; the active spans occupy lanes 1 through 4.
    const commits = [
      commit("filler", ["root"], 1),
      commit("master", ["base"], 2),
      commit("spacer", ["root"], 3),
      commit("merge", ["feature", "base"], 4),
      commit("base", ["root"], 0),
      commit("feature", ["root"], 4),
      commit("root", [], 0),
    ];
    const lanes = new Map(commits.map((item) => [item.oid, mirrored ? 4 - item.graph.lane : item.graph.lane]));
    for (const item of commits) {
      item.graph.lane = lanes.get(item.oid)!;
      item.graph.edges = item.parent_oids.map((parent, index) => ({
        parent_oid: parent,
        from_lane: item.graph.lane,
        to_lane: lanes.get(parent)!,
        merge: index > 0,
      }));
    }

    // Include fractional clamps, coincident source/trunk lanes, and full collapse.
    for (const maxX of [Infinity, 87, 78, 60, 51, 48.5, 42, 25.5, 24.5, 24]) {
      const geometry = buildGraphGeometry(commits, true, { maxX });
      const master = geometry.paths.find((route) => route.key === "master:base:0")!;
      const merge = geometry.paths.find((route) => route.key === "merge:base:1")!;
      const tail = master.data.slice(master.data.indexOf(" L "));

      assert.equal(merge.color, master.color, `shared branch color at maxX=${maxX}`);
      assert.ok(merge.data.endsWith(tail), `shared bend at maxX=${maxX}: ${merge.data} must end with ${tail}`);
    }
  });
}
