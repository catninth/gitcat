import assert from "node:assert/strict";
import test from "node:test";

import checkoutSwitchOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/checkout-switch.json";
import checkoutWipOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/checkout-wip.json";
import convergenceLaneReuseOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/convergence-lane-reuse.json";
import disconnectedCheckoutOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/disconnected-checkout.json";
import disconnectedInteriorReuseOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/disconnected-interior-reuse.json";
import mergeLadderOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/merge-ladder.json";
import stashCheckoutOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/stash-checkout.json";
import stashIndexCollisionOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/stash-index-collision.json";
import stashLifecycleOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/stash-lifecycle.json";
import stashSingleOracleJson from "../../../crates/gitcat-git-cli/tests/fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/stash-single.json";
import { buildGraphGeometry } from "../src/components/commitGraphGeometry";
import { buildBranchColors, getGraphEdgeColor } from "../src/components/graphPresentation";
import type { CommitSummary, GraphEdge } from "../src/lib/types";

interface OracleRef {
  name: string;
  checked_out: boolean;
  color_slot: number;
}

interface OracleEdge {
  parent: string;
  parent_index: number;
  from_lane: number;
  to_lane: number;
  color_slot: number;
}

interface OracleRow {
  commit: string;
  row: number;
  lane: number;
  color_slot: number;
  refs?: OracleRef[];
  edges: OracleEdge[];
}

interface OracleWip {
  head: string;
  lane: number;
  color_slot: number;
}

interface OracleStash {
  id: string;
  message: string;
  selector: string;
  display_row: number;
  lane: number;
  color_slot: number;
  parent: string;
}

interface OracleCrossing {
  at: string;
  topmost: string;
  note?: string;
}

interface OracleCheckpoint {
  id: string;
  head: string;
  rows?: OracleRow[];
  same_graph_as?: string;
  same_commits_as?: string;
  wip?: OracleWip | null;
  stashes?: OracleStash[];
  crossings?: OracleCrossing[];
}

interface SemanticOracle {
  scenario: string;
  checkpoints: OracleCheckpoint[];
}

interface ResolvedOracleCheckpoint {
  rows: readonly OracleRow[];
  wip: OracleWip | null;
  stashes: readonly OracleStash[];
  crossings: readonly OracleCrossing[];
}

const WIP_COLOR_SLOT = 0;
const ORACLES = [
  checkoutSwitchOracleJson,
  checkoutWipOracleJson,
  convergenceLaneReuseOracleJson,
  disconnectedCheckoutOracleJson,
  disconnectedInteriorReuseOracleJson,
  mergeLadderOracleJson,
  stashCheckoutOracleJson,
  stashIndexCollisionOracleJson,
  stashLifecycleOracleJson,
  stashSingleOracleJson,
] as unknown as SemanticOracle[];

function edge(parentOid: string, fromLane: number, toLane: number): GraphEdge {
  return {
    parent_oid: parentOid,
    from_lane: fromLane,
    to_lane: toLane,
    merge: false,
  };
}

function commit(
  oid: string,
  lane: number,
  edges: GraphEdge[],
  options: { head?: boolean; stashIndex?: number } = {},
): CommitSummary {
  return {
    oid,
    short_oid: oid,
    parent_oids: edges.map((item) => item.parent_oid),
    subject: oid,
    body_preview: "",
    author: { name: "Graph Oracle", email: "graph@example.invalid" },
    authored_at: { seconds: 0, offset_minutes: 0 },
    committed_at: { seconds: 0, offset_minutes: 0 },
    decorations: options.head
      ? [{
          name: oid,
          full_name: `refs/heads/${oid}`,
          kind: "local_branch",
          is_head: true,
        }]
      : [],
    stash: options.stashIndex === undefined
      ? undefined
      : {
          index: options.stashIndex,
          selector: `stash@{${options.stashIndex}}`,
          oid: `stash-outer-${options.stashIndex}`,
        },
    graph: { lane, edges },
  };
}

function colors(commits: readonly CommitSummary[], hasWip: boolean): Record<string, number> {
  return Object.fromEntries(buildBranchColors(commits, hasWip));
}

function oracleCheckpoint(oracle: SemanticOracle, id: string): OracleCheckpoint {
  const checkpoint = oracle.checkpoints.find((candidate) => candidate.id === id);
  assert.ok(checkpoint, `${oracle.scenario}: unknown checkpoint alias ${id}`);
  return checkpoint;
}

function assertNoAliasCycle(
  oracle: SemanticOracle,
  checkpoint: OracleCheckpoint,
  trail: readonly string[],
): void {
  assert.ok(
    !trail.includes(checkpoint.id),
    `${oracle.scenario}: checkpoint alias cycle ${[...trail, checkpoint.id].join(" -> ")}`,
  );
}

function resolveRows(
  oracle: SemanticOracle,
  checkpoint: OracleCheckpoint,
  trail: readonly string[] = [],
): readonly OracleRow[] {
  assertNoAliasCycle(oracle, checkpoint, trail);
  if (checkpoint.rows) return checkpoint.rows;

  const target = checkpoint.same_graph_as ?? checkpoint.same_commits_as;
  assert.ok(target, `${oracle.scenario}/${checkpoint.id}: rows or a graph/commit alias are required`);
  return resolveRows(oracle, oracleCheckpoint(oracle, target), [...trail, checkpoint.id]);
}

function resolveWip(
  oracle: SemanticOracle,
  checkpoint: OracleCheckpoint,
  trail: readonly string[] = [],
): OracleWip | null {
  assertNoAliasCycle(oracle, checkpoint, trail);
  if (Object.prototype.hasOwnProperty.call(checkpoint, "wip")) return checkpoint.wip ?? null;
  if (!checkpoint.same_graph_as) return null;
  return resolveWip(
    oracle,
    oracleCheckpoint(oracle, checkpoint.same_graph_as),
    [...trail, checkpoint.id],
  );
}

function resolveStashes(
  oracle: SemanticOracle,
  checkpoint: OracleCheckpoint,
  trail: readonly string[] = [],
): readonly OracleStash[] {
  assertNoAliasCycle(oracle, checkpoint, trail);
  if (Object.prototype.hasOwnProperty.call(checkpoint, "stashes")) {
    return checkpoint.stashes ?? [];
  }
  if (!checkpoint.same_graph_as) return [];
  return resolveStashes(
    oracle,
    oracleCheckpoint(oracle, checkpoint.same_graph_as),
    [...trail, checkpoint.id],
  );
}

function resolveCrossings(
  oracle: SemanticOracle,
  checkpoint: OracleCheckpoint,
  trail: readonly string[] = [],
): readonly OracleCrossing[] {
  assertNoAliasCycle(oracle, checkpoint, trail);
  if (Object.prototype.hasOwnProperty.call(checkpoint, "crossings")) {
    return checkpoint.crossings ?? [];
  }
  if (!checkpoint.same_graph_as) return [];
  return resolveCrossings(
    oracle,
    oracleCheckpoint(oracle, checkpoint.same_graph_as),
    [...trail, checkpoint.id],
  );
}

function resolveCheckpoint(
  oracle: SemanticOracle,
  checkpoint: OracleCheckpoint,
): ResolvedOracleCheckpoint {
  return {
    rows: resolveRows(oracle, checkpoint),
    wip: resolveWip(oracle, checkpoint),
    stashes: resolveStashes(oracle, checkpoint),
    crossings: resolveCrossings(oracle, checkpoint),
  };
}

function headCommitId(checkpoint: OracleCheckpoint, rows: readonly OracleRow[]): string {
  const referencedHead = rows.find((row) => (
    row.refs?.some((reference) => reference.name === checkpoint.head)
  ));
  if (referencedHead) return referencedHead.commit;

  const conventionalTip = rows.find((row) => row.commit === `${checkpoint.head}-tip`);
  assert.ok(conventionalTip, `${checkpoint.id}: cannot locate checked-out branch ${checkpoint.head}`);
  return conventionalTip.commit;
}

function commitFromOracleRow(
  row: OracleRow,
  checkpoint: OracleCheckpoint,
  checkedOutOid: string,
): CommitSummary {
  const graphEdges = [...row.edges]
    .sort((left, right) => left.parent_index - right.parent_index)
    .map((item) => ({
      parent_oid: item.parent,
      from_lane: item.from_lane,
      to_lane: item.to_lane,
      merge: item.parent_index > 0,
    }));
  const result = commit(row.commit, row.lane, graphEdges);
  const decorations = (row.refs ?? []).map((reference) => ({
    name: reference.name,
    full_name: `refs/heads/${reference.name}`,
    kind: "local_branch" as const,
    is_head: row.commit === checkedOutOid && reference.name === checkpoint.head,
  }));

  if (row.commit === checkedOutOid && !decorations.some((decoration) => decoration.is_head)) {
    decorations.push({
      name: checkpoint.head,
      full_name: `refs/heads/${checkpoint.head}`,
      kind: "local_branch",
      is_head: true,
    });
  }

  return { ...result, decorations };
}

function stashCommitFromOracle(
  stash: OracleStash,
  rowsByOid: ReadonlyMap<string, OracleRow>,
  context: string,
): CommitSummary {
  const selector = /^stash@\{(\d+)\}$/.exec(stash.selector);
  assert.ok(selector, `${context}: invalid stash selector ${stash.selector}`);
  const parent = rowsByOid.get(stash.parent);
  assert.ok(parent, `${context}: unknown stash parent ${stash.parent}`);

  return {
    ...commit(
      stash.id,
      stash.lane,
      [edge(stash.parent, stash.lane, parent.lane)],
      { stashIndex: Number(selector[1]) },
    ),
    subject: stash.message,
  };
}

function requiredColor(
  assigned: ReadonlyMap<string, number>,
  oid: string,
  context: string,
): number {
  const slot = assigned.get(oid);
  assert.notEqual(slot, undefined, `${context}: no presentation color for ${oid}`);
  return slot as number;
}

function assertOracleCheckpointColors(
  oracle: SemanticOracle,
  checkpoint: OracleCheckpoint,
): void {
  const context = `${oracle.scenario}/${checkpoint.id}`;
  const resolved = resolveCheckpoint(oracle, checkpoint);
  const orderedRows = [...resolved.rows].sort((left, right) => left.row - right.row);
  const rowsByOid = new Map(orderedRows.map((row) => [row.commit, row]));
  const checkedOutOid = headCommitId(checkpoint, orderedRows);
  const rowCommits = orderedRows.map((row) => (
    commitFromOracleRow(row, checkpoint, checkedOutOid)
  ));
  const stashCommits = [...resolved.stashes]
    .sort((left, right) => left.display_row - right.display_row)
    .map((stash) => stashCommitFromOracle(stash, rowsByOid, context));
  const commits = [...stashCommits, ...rowCommits];
  const commitsByOid = new Map(commits.map((item) => [item.oid, item]));
  const assigned = buildBranchColors(commits, resolved.wip !== null);

  for (const row of orderedRows) {
    const nodeColor = requiredColor(assigned, row.commit, context);
    assert.equal(nodeColor, row.color_slot, `${context}: node ${row.commit}`);

    for (const reference of row.refs ?? []) {
      assert.equal(
        nodeColor,
        reference.color_slot,
        `${context}: ref ${reference.name} on ${row.commit}`,
      );
    }

    const source = commitsByOid.get(row.commit);
    assert.ok(source, `${context}: missing commit ${row.commit}`);
    const expectedEdges = [...row.edges].sort((left, right) => (
      left.parent_index - right.parent_index
    ));
    assert.equal(source.graph.edges.length, expectedEdges.length, `${context}: edge count ${row.commit}`);

    for (let edgeIndex = 0; edgeIndex < expectedEdges.length; edgeIndex += 1) {
      const expected = expectedEdges[edgeIndex];
      const actualEdge = source.graph.edges[edgeIndex];
      assert.equal(expected.parent_index, edgeIndex, `${context}: parent index ${row.commit}`);
      assert.equal(actualEdge.parent_oid, expected.parent, `${context}: edge parent ${row.commit}`);
      assert.equal(
        getGraphEdgeColor(source, edgeIndex, assigned),
        expected.color_slot,
        `${context}: edge ${row.commit} -> ${expected.parent}`,
      );
    }
  }

  for (const stash of resolved.stashes) {
    const stashColor = requiredColor(assigned, stash.id, context);
    assert.equal(stashColor, stash.color_slot, `${context}: stash node ${stash.id}`);
    const stashCommit = commitsByOid.get(stash.id);
    assert.ok(stashCommit, `${context}: missing stash commit ${stash.id}`);
    assert.equal(
      getGraphEdgeColor(stashCommit, 0, assigned),
      stash.color_slot,
      `${context}: stash connector ${stash.id}`,
    );
  }

  if (resolved.wip) {
    assert.equal(WIP_COLOR_SLOT, resolved.wip.color_slot, `${context}: WIP node/connector`);
    assert.equal(
      requiredColor(assigned, resolved.wip.head, context),
      resolved.wip.color_slot,
      `${context}: WIP HEAD span ${resolved.wip.head}`,
    );
  }
}

// GraphPath.key is `${childOid}:${parentOid}:${edgeIndex}`; splitting on ":"
// is safe here because every fixture and oracle commit id in this suite is a
// plain slug with no colons in it.
function assertOracleCheckpointCrossings(
  oracle: SemanticOracle,
  checkpoint: OracleCheckpoint,
): void {
  const context = `${oracle.scenario}/${checkpoint.id}`;
  const resolved = resolveCheckpoint(oracle, checkpoint);
  if (resolved.crossings.length === 0) return;

  const orderedRows = [...resolved.rows].sort((left, right) => left.row - right.row);
  const rowsByOid = new Map(orderedRows.map((row) => [row.commit, row]));
  const checkedOutOid = headCommitId(checkpoint, orderedRows);
  const rowCommits = orderedRows.map((row) => (
    commitFromOracleRow(row, checkpoint, checkedOutOid)
  ));
  const stashCommits = [...resolved.stashes]
    .sort((left, right) => left.display_row - right.display_row)
    .map((stash) => stashCommitFromOracle(stash, rowsByOid, context));
  const commits = [...stashCommits, ...rowCommits];

  const geometry = buildGraphGeometry(commits, resolved.wip !== null);

  for (const crossing of resolved.crossings) {
    const incoming = geometry.paths.filter((path) => path.key.split(":")[1] === crossing.at);
    assert.ok(
      incoming.length > 1,
      `${context}: crossing at ${crossing.at} needs at least two converging edges, found ${incoming.length}`,
    );

    const topmostChildOid = incoming[incoming.length - 1].key.split(":")[0];
    assert.equal(
      topmostChildOid,
      crossing.topmost,
      `${context}: crossing at ${crossing.at} expected ${crossing.topmost} painted on top, got ${topmostChildOid}`,
    );
  }
}

test("clean checkout preserves the row-order branch colors", () => {
  const commits = [
    commit("feature", 0, [edge("base", 0, 0)]),
    commit("main", 1, [edge("base", 1, 0)], { head: true }),
    commit("base", 0, []),
  ];

  assert.deepEqual(colors(commits, false), { feature: 0, main: 1, base: 0 });
});

test("WIP gives the checked-out main span and shared base color zero", () => {
  const commits = [
    commit("feature", 1, [edge("base", 1, 0)]),
    commit("main", 0, [edge("base", 0, 0)], { head: true }),
    commit("base", 0, []),
  ];

  assert.deepEqual(colors(commits, true), { feature: 1, main: 0, base: 0 });
});

test("a clean stash can share color zero with its parent branch", () => {
  const commits = [
    commit("stash-main", 0, [edge("main", 0, 0)], { stashIndex: 0 }),
    commit("feature", 1, [edge("base", 1, 0)]),
    commit("main", 0, [edge("base", 0, 0)], { head: true }),
    commit("base", 0, []),
  ];

  assert.deepEqual(colors(commits, false), {
    "stash-main": 0,
    feature: 1,
    main: 0,
    base: 0,
  });
});

test("WIP reserves color zero and moves a foreign stash to color one", () => {
  const commits = [
    commit("stash-main", 1, [edge("main", 1, 1)], { stashIndex: 0 }),
    commit("feature", 0, [edge("base", 0, 0)], { head: true }),
    commit("main", 1, [edge("base", 1, 0)]),
    commit("base", 0, []),
  ];

  assert.deepEqual(colors(commits, true), {
    "stash-main": 1,
    feature: 0,
    main: 1,
    base: 0,
  });
});

test("clean concurrent stash routes use colors zero, one, and two", () => {
  const commits = [
    commit("stash-a", 0, [edge("feature", 0, 0)], { stashIndex: 2 }),
    commit("stash-c", 1, [edge("feature", 1, 0)], { stashIndex: 0 }),
    commit("stash-b", 2, [edge("feature", 2, 0)], { stashIndex: 1 }),
    commit("feature", 0, [edge("base", 0, 0)], { head: true }),
    commit("main", 1, [edge("base", 1, 0)]),
    commit("base", 0, []),
  ];

  const actual = colors(commits, false);
  assert.equal(actual["stash-a"], 0);
  assert.equal(actual["stash-c"], 1);
  assert.equal(actual["stash-b"], 2);
  assert.equal(actual.feature, 0);
  assert.equal(actual.main, 1);
  assert.equal(actual.base, 0);
});

test("a disconnected component in a newly opened low lane keeps its canonical color", () => {
  const commits = [
    commit("main-tip", 0, [edge("main-root", 0, 0)], { head: true }),
    commit("orphan-tip", 7, [edge("orphan-root", 7, 7)]),
    commit("main-root", 0, []),
    commit("orphan-root", 7, []),
  ];

  assert.deepEqual(colors(commits, false), {
    "main-tip": 0,
    "main-root": 0,
    "orphan-tip": 7,
    "orphan-root": 7,
  });
});

test("every palette block restarts, so lanes 0, 10 and 20 share color zero", () => {
  const commits = [
    commit("block-0", 0, [edge("base", 0, 0)], { head: true }),
    commit("block-1", 10, [edge("base", 10, 0)]),
    commit("block-2", 20, [edge("base", 20, 0)]),
    commit("block-2-next", 21, [edge("base", 21, 0)]),
    commit("base", 0, []),
  ];

  assert.deepEqual(colors(commits, false), {
    "block-0": 0,
    "block-1": 0,
    "block-2": 0,
    "block-2-next": 1,
    base: 0,
  });
});

test("overlapping siblings in the third palette block resolve inside that block", () => {
  const commits = [
    commit("low", 1, [edge("base", 1, 0)], { head: true }),
    commit("twin-a", 21, [edge("base", 21, 0)]),
    commit("twin-b", 31, [edge("base", 31, 0)]),
    commit("base", 0, []),
  ];

  const actual = colors(commits, false);
  assert.equal(actual.low, 1);
  assert.equal(actual["twin-a"], 1);
  assert.equal(actual["twin-b"], 1);
});

test("a convergence paints the physically backmost route on top", () => {
  const commits = [
    commit("front", 0, [edge("base", 0, 0)], { head: true }),
    commit("back", 5, [edge("base", 5, 0)]),
    commit("middle", 2, [edge("base", 2, 0)]),
    commit("base", 0, []),
  ];

  const geometry = buildGraphGeometry(commits, false);
  const incoming = geometry.paths
    .filter((path) => path.key.split(":")[1] === "base")
    .map((path) => path.key.split(":")[0]);

  assert.deepEqual(incoming, ["front", "middle", "back"]);
});

for (const oracle of ORACLES) {
  for (const checkpoint of oracle.checkpoints) {
    test(`GitKraken oracle colors: ${oracle.scenario}/${checkpoint.id}`, () => {
      assertOracleCheckpointColors(oracle, checkpoint);
    });
    test(`GitKraken oracle crossings: ${oracle.scenario}/${checkpoint.id}`, () => {
      assertOracleCheckpointCrossings(oracle, checkpoint);
    });
  }
}
