import assert from "node:assert/strict";
import test from "node:test";

import {
  getCommitGraphLastLaneX,
  getCommitGraphMaxX,
  getCommitGraphOverflowEdge,
  getCommitLaneX,
  getCommitLaneXCss,
  isCommitGraphCollapsed,
} from "../src/components/commitGraphGeometry";
import {
  ALL_GRAPH_COLUMNS,
  COMPACT_GRAPH_COLUMN_WIDTHS,
  DEFAULT_GRAPH_COLUMN_WIDTHS,
  effectiveGraphColumnWidth,
  GRAPH_COLUMN_MIN_WIDTH,
  graphColumnOffset,
  graphColumnsMinWidth,
  graphColumnsTemplate,
  graphColumnWidth,
  isRefColumnIconOnly,
  MIN_GRAPH_COLUMN_WIDTH,
  MIN_GRAPH_LANE_EXTENT,
} from "../src/lib/columns";
import type { CommitSummary, GraphColumnSettings } from "../src/lib/types";

const LANE_EXTENT = 300;

function columns(overrides: Partial<GraphColumnSettings> = {}): GraphColumnSettings {
  return { ...ALL_GRAPH_COLUMNS, ...overrides };
}

test("only the last visible column stretches", () => {
  const template = graphColumnsTemplate(columns(), DEFAULT_GRAPH_COLUMN_WIDTHS, LANE_EXTENT);

  assert.equal(
    template,
    "118px 300px 300px 86px 116px minmax(52px, 1fr)",
  );
});

test("columns build left to right as they are switched on", () => {
  const widths = DEFAULT_GRAPH_COLUMN_WIDTHS;
  const withoutSha = graphColumnsTemplate(columns({ sha: false }), widths, LANE_EXTENT);
  const withSha = graphColumnsTemplate(columns(), widths, LANE_EXTENT);

  // Turning SHA on keeps every column left of it at its own width and hands the
  // leftover room to the new trailing column instead of squeezing the message.
  assert.equal(withoutSha, "118px 300px 300px 86px minmax(56px, 1fr)");
  assert.equal(withSha, "118px 300px 300px 86px 116px minmax(52px, 1fr)");
});

test("a trailing graph column keeps its lane width as its floor", () => {
  const template = graphColumnsTemplate(
    columns({ message: false, author: false, date: false, sha: false }),
    { ...DEFAULT_GRAPH_COLUMN_WIDTHS, graph: 150 },
    LANE_EXTENT,
  );

  assert.equal(template, "118px minmax(150px, 1fr)");
});

test("a single-lane history still leaves the graph column room to drag", () => {
  // The lane extent never drops below its own floor, so even a linear history
  // has a range between the drag floor and that extent.
  assert.ok(MIN_GRAPH_COLUMN_WIDTH < MIN_GRAPH_LANE_EXTENT);
  assert.equal(effectiveGraphColumnWidth({ ...DEFAULT_GRAPH_COLUMN_WIDTHS, graph: 60 }, MIN_GRAPH_LANE_EXTENT), 60);
  // Squeezed to the floor, every lane parks on lane 0.
  assert.equal(getCommitGraphMaxX(MIN_GRAPH_COLUMN_WIDTH), getCommitLaneX(0));
});

test("the graph column is bounded by the lane extent", () => {
  assert.equal(effectiveGraphColumnWidth({ ...DEFAULT_GRAPH_COLUMN_WIDTHS, graph: null }, LANE_EXTENT), LANE_EXTENT);
  assert.equal(effectiveGraphColumnWidth({ ...DEFAULT_GRAPH_COLUMN_WIDTHS, graph: 900 }, LANE_EXTENT), LANE_EXTENT);
  assert.equal(
    effectiveGraphColumnWidth({ ...DEFAULT_GRAPH_COLUMN_WIDTHS, graph: 10 }, LANE_EXTENT),
    MIN_GRAPH_COLUMN_WIDTH,
  );
});

test("lanes past the visible width park on the drag limit", () => {
  const maxX = getCommitGraphMaxX(MIN_GRAPH_LANE_EXTENT);

  assert.equal(getCommitLaneX(2, maxX), getCommitLaneX(2));
  assert.equal(getCommitLaneX(9, maxX), maxX);
  // The limit is a pixel, not a lane: one pixel of drag moves the parked lanes
  // by one pixel instead of snapping them a whole lane at a time.
  assert.equal(
    getCommitLaneX(9, getCommitGraphMaxX(MIN_GRAPH_LANE_EXTENT + 1)) - getCommitLaneX(9, maxX),
    1,
  );
  // Dragging the column out to the full extent fans them back out.
  assert.equal(getCommitLaneX(9, getCommitGraphMaxX(LANE_EXTENT)), getCommitLaneX(9));
});

test("rows park their nodes in CSS so a drag does not re-render them", () => {
  assert.equal(getCommitLaneXCss(0), "min(24px, var(--gc-graph-max-x, 100000px))");
  assert.equal(getCommitLaneXCss(3), "min(78px, var(--gc-graph-max-x, 100000px))");
});

test("the scroll floor follows the dragged widths", () => {
  const widths = { ...DEFAULT_GRAPH_COLUMN_WIDTHS, graph: MIN_GRAPH_COLUMN_WIDTH, message: 400 };

  assert.equal(
    graphColumnsMinWidth(columns(), widths, LANE_EXTENT),
    118 + MIN_GRAPH_COLUMN_WIDTH + 400 + 86 + 116 + 52,
  );
});

test("the ref column collapses to icons once it is squeezed", () => {
  assert.equal(isRefColumnIconOnly(columns(), DEFAULT_GRAPH_COLUMN_WIDTHS), false);
  assert.equal(isRefColumnIconOnly(columns(), COMPACT_GRAPH_COLUMN_WIDTHS), true);
  // A trailing ref column stretches instead, so it never renders compact.
  assert.equal(
    isRefColumnIconOnly(
      columns({ graph: false, message: false, author: false, date: false, sha: false }),
      COMPACT_GRAPH_COLUMN_WIDTHS,
    ),
    false,
  );
});

test("rows offset their lanes by the dragged ref width", () => {
  assert.equal(graphColumnOffset(columns(), DEFAULT_GRAPH_COLUMN_WIDTHS, LANE_EXTENT), 118);
  assert.equal(graphColumnOffset(columns(), COMPACT_GRAPH_COLUMN_WIDTHS, LANE_EXTENT), 32);
  assert.equal(graphColumnOffset(columns({ refs: false }), DEFAULT_GRAPH_COLUMN_WIDTHS, LANE_EXTENT), 0);
});

test("compact widths sit at every column floor but the message", () => {
  for (const key of ["refs", "author", "date", "sha"] as const) {
    assert.equal(
      graphColumnWidth(key, COMPACT_GRAPH_COLUMN_WIDTHS, LANE_EXTENT),
      GRAPH_COLUMN_MIN_WIDTH[key],
    );
  }
  assert.equal(effectiveGraphColumnWidth(COMPACT_GRAPH_COLUMN_WIDTHS, LANE_EXTENT), MIN_GRAPH_COLUMN_WIDTH);
  // Compacting the chrome is what buys the subject line its room, so the
  // message column keeps the full width it has in the default layout.
  assert.equal(
    graphColumnWidth("message", COMPACT_GRAPH_COLUMN_WIDTHS, LANE_EXTENT),
    DEFAULT_GRAPH_COLUMN_WIDTHS.message,
  );
});

function commitOnLane(oid: string, lane: number): CommitSummary {
  return {
    oid,
    short_oid: oid.slice(0, 7),
    subject: oid,
    body_preview: "",
    author: { name: "a", email: "a@example.com" },
    committer: { name: "a", email: "a@example.com" },
    authored_at: { seconds: 0, offset_minutes: 0 },
    committed_at: { seconds: 0, offset_minutes: 0 },
    parent_oids: [],
    decorations: [],
    graph: { lane, edges: [] },
    stash: null,
  } as unknown as CommitSummary;
}

test("the overflow edge only shows when a lane is really parked", () => {
  const linear = [commitOnLane("a", 0), commitOnLane("b", 0)];
  const wide = [commitOnLane("a", 0), commitOnLane("b", 5)];

  // A single-lane history hides nothing even at the drag floor, so the fade
  // must stay off however far the column is squeezed.
  assert.equal(getCommitGraphLastLaneX(linear), getCommitLaneX(0));
  assert.ok(getCommitGraphMaxX(MIN_GRAPH_COLUMN_WIDTH) >= getCommitGraphLastLaneX(linear));

  // Six lanes in a floor-width column do park.
  assert.equal(getCommitGraphLastLaneX(wide), getCommitLaneX(5));
  assert.ok(getCommitGraphMaxX(MIN_GRAPH_LANE_EXTENT) < getCommitGraphLastLaneX(wide));
});

test("parking limit reaches lane zero at the floor", () => {
  const maxX = getCommitGraphMaxX(MIN_GRAPH_COLUMN_WIDTH);

  // Fully collapsed there is one column of nodes and nothing standing beside
  // it: lane 0 parks with everything else rather than staying a few pixels off.
  assert.equal(maxX, getCommitLaneX(0));
  assert.equal(getCommitLaneX(9, maxX), getCommitLaneX(0, maxX));
  // The node keeps a few pixels of air before the next column starts.
  assert.equal(MIN_GRAPH_COLUMN_WIDTH - (maxX + 11), 6);
  // And with every route on one column the lane lines are dropped.
  assert.equal(isCommitGraphCollapsed(MIN_GRAPH_COLUMN_WIDTH), true);
  assert.equal(isCommitGraphCollapsed(MIN_GRAPH_COLUMN_WIDTH + 1), false);
});

test("the overflow band narrows and fades as the last hidden lane returns", () => {
  const lastLaneX = getCommitLaneX(6);
  const deep = getCommitGraphOverflowEdge(lastLaneX, MIN_GRAPH_LANE_EXTENT);

  assert.ok(deep);
  assert.equal(deep.width, 22);
  assert.equal(deep.opacity, 1);
  // It ends a node radius short of the parked nodes, leaving them and the
  // lane-coloured column border untouched.
  assert.equal(deep.left + deep.width, getCommitGraphMaxX(MIN_GRAPH_LANE_EXTENT) - 11);

  // Dragged all the way left the routes merge into one column, so the band
  // that marked the overlap is gone with them -- and it thins and fades on the
  // way there rather than vanishing at the last pixel.
  assert.equal(getCommitGraphOverflowEdge(lastLaneX, MIN_GRAPH_COLUMN_WIDTH), null);
  const nearFloor = getCommitGraphOverflowEdge(lastLaneX, MIN_GRAPH_COLUMN_WIDTH + 6);
  assert.ok(nearFloor);
  assert.ok(nearFloor.opacity < deep.opacity / 3);
  assert.ok(nearFloor.width < deep.width);

  // One lane still hidden by a few pixels: a sliver at low opacity.
  const nodeMargin = MIN_GRAPH_COLUMN_WIDTH - getCommitLaneX(0);
  const widthHiding = (pixels: number) => lastLaneX - pixels + nodeMargin;
  const almost = getCommitGraphOverflowEdge(lastLaneX, widthHiding(4));
  assert.ok(almost);
  assert.equal(almost.width, 4);
  assert.ok(almost.opacity < 0.2);

  // Nothing hidden, no band at all.
  assert.equal(getCommitGraphOverflowEdge(lastLaneX, widthHiding(0)), null);
});
