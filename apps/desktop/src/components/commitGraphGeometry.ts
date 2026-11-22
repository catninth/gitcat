// Pure layout of the commit graph: lane positions, edge routes and their
// paint order, and which rows a viewport needs. Nothing here touches the DOM,
// so the SVG layering rules can be tested without rendering a row.

import { MIN_GRAPH_LANE_EXTENT } from "../lib/columns";
import { GRAPH_LANE_SLOTS } from "../lib/styles";
import type { CommitSummary, GraphColumnSettings } from "../lib/types";
import {
  buildBranchColors as buildPresentationBranchColors,
  getGraphEdgeColor,
} from "./graphPresentation";

export const ROW_HEIGHT = 26;
export const ROW_GAP = 3;
export const ROW_STRIDE = ROW_HEIGHT + ROW_GAP;
export const LANE_WIDTH = 18;
export const GRAPH_PADDING = 24;
export const FIRST_COLOR_SLOT = 0;
export const EDGE_CORNER = 12;
export const AVATAR_RADIUS = 11;
// Air between the rightmost node and the end of the graph column. It is what
// the collapsed column shows between the node and the next column, so it is
// deliberately a little wider than a hairline.
export const NODE_END_GAP = 6;
export const MERGE_NODE_RADIUS = 4.5;
export const STASH_NODE_RADIUS = 3;
export const WIP_NODE_RADIUS = 10;
// The dashed WIP circle carries a 2px border, so its outer edge sits one pixel
// past the radius. The connector starts there instead of at the node centre,
// which keeps the dash phase anchored to the circle no matter how short the run.
export const WIP_NODE_EDGE = WIP_NODE_RADIUS + 1;
// Mirrors --gc-wip-row-gap in styles.css.
export const WIP_GAP = 4;
export const WIP_ROW_Y = -(WIP_GAP + ROW_HEIGHT / 2);
// Only the rows around the viewport are mounted, and only the routes crossing
// them are painted. The window moves in whole chunks so a scroll re-renders
// once every chunk rather than once every row, and the overscan keeps the
// chunk edge -- where routes are cut off -- outside the viewport.
export const ROW_WINDOW_OVERSCAN = 20;
export const ROW_WINDOW_CHUNK = 20;
// Rendered before the scroll container has been measured.
export const ROW_WINDOW_INITIAL = 80;
export interface GraphPath {
  key: string;
  data: string;
  color: number;
  paintLane: number;
  merge: boolean;
  stash: boolean;
  // The rows the route spans; `toRow` is the row count when the parent has not
  // been loaded yet and the route runs off the bottom of the list.
  fromRow: number;
  toRow: number;
}

export interface RowWindow {
  first: number;
  last: number;
}

export interface GraphGeometry {
  paths: GraphPath[];
  colors: Map<string, number>;
  width: number;
  height: number;
}
// A graph column narrower than the lane extent parks the lanes that no longer
// fit against `maxX`, so the routes overlap there and flow back out as the
// column is dragged. The limit is a pixel value, not a lane index: the parked
// nodes have to follow the drag continuously instead of snapping lane to lane.
export function laneX(lane: number, maxX = Number.POSITIVE_INFINITY): number {
  return Math.min(GRAPH_PADDING + lane * LANE_WIDTH, maxX);
}

// Rightmost pixel a node can sit on inside a column of this width. At the full
// lane extent this is past the last lane, so nothing parks.
export function getCommitGraphMaxX(width: number): number {
  return Math.max(GRAPH_PADDING, width - AVATAR_RADIUS - NODE_END_GAP);
}

// True once the limit has reached lane 0: every route is stacked on one column
// of nodes, so the lane lines would be a single vertical stripe behind them and
// are dropped entirely.
export function isCommitGraphCollapsed(width: number): boolean {
  return getCommitGraphMaxX(width) <= GRAPH_PADDING;
}

// Rows read the parking limit and the graph column offset off the header
// element, so dragging the column moves every node without React re-rendering
// a single row.
export function clampedLaneXCss(lane: number): string {
  return `min(${GRAPH_PADDING + lane * LANE_WIDTH}px, var(--gc-graph-max-x, 100000px))`;
}

// A merge draws as a plain junction dot rather than an avatar: its author
// initial says nothing that the edges running into it do not already show.
export function isMergeNode(commit: CommitSummary): boolean {
  return !commit.stash && commit.parent_oids.length > 1;
}

export function getCommitLaneX(lane: number, maxX?: number): number {
  return laneX(lane, maxX);
}

export function getCommitLaneXCss(lane: number): string {
  return clampedLaneXCss(lane);
}

// Where the row's branch stripe starts: the graph column offset plus the node
// position, both left to CSS so a drag never invalidates the row.
export function getCommitRowBranchOrigin(
  lane: number,
  columns: GraphColumnSettings,
  nodeRadius = AVATAR_RADIUS,
): string {
  const offset = "var(--gc-graph-offset, 0px)";
  return columns.graph
    ? `calc(${offset} + ${clampedLaneXCss(lane)} - ${nodeRadius}px)`
    : offset;
}

export function getWipLaneColorVariable(): string {
  return colorVariable(FIRST_COLOR_SLOT);
}

export function colorVariable(slot: number): string {
  return `var(--gc-lane-${slot % GRAPH_LANE_SLOTS})`;
}

export function colorClass(base: string, slot: number): string {
  return `${base} ${base}--lane-${slot % GRAPH_LANE_SLOTS}`;
}

export function rowY(index: number): number {
  return index * ROW_STRIDE + ROW_HEIGHT / 2;
}
// The width at which every lane in this history gets its own column position.
// It is the upper bound of the graph column: past it the column would only add
// blank space to the right of the last lane.
export function getCommitGraphWidth(commits: readonly CommitSummary[]): number {
  let maxLane = 0;

  for (const commit of commits) {
    maxLane = Math.max(maxLane, commit.graph.lane);
    for (const edge of commit.graph.edges) {
      maxLane = Math.max(maxLane, edge.from_lane, edge.to_lane);
    }
  }

  return Math.max(MIN_GRAPH_LANE_EXTENT, GRAPH_PADDING * 2 + maxLane * LANE_WIDTH + LANE_WIDTH);
}

// Where the rightmost lane sits when nothing is parked. Compared against the
// drag limit this is what tells the column whether it is currently hiding
// anything -- the lane extent alone cannot, because it has a floor.
export function getCommitGraphLastLaneX(commits: readonly CommitSummary[]): number {
  let maxLane = 0;

  for (const commit of commits) {
    maxLane = Math.max(maxLane, commit.graph.lane);
    for (const edge of commit.graph.edges) {
      maxLane = Math.max(maxLane, edge.from_lane, edge.to_lane);
    }
  }

  return GRAPH_PADDING + maxLane * LANE_WIDTH;
}

export interface GraphOverflowEdge {
  // Both measured from the graph column's left edge.
  left: number;
  width: number;
  opacity: number;
}

// The band that marks parked routes. It stops one node radius short of where
// the parked nodes sit, so those and the lane-coloured column border stay crisp.
// It fades out at both ends of the drag: to the right as the last hidden lane
// comes back into view, and to the left as the column approaches the floor,
// where the routes merge into one column of nodes and there is no overlap left
// to mark.
export function getCommitGraphOverflowEdge(
  lastLaneX: number,
  graphWidth: number,
): GraphOverflowEdge | null {
  const maxX = getCommitGraphMaxX(graphWidth);
  const hidden = lastLaneX - maxX;
  const room = maxX - GRAPH_PADDING;
  if (hidden <= 0 || room <= 0) return null;

  const full = AVATAR_RADIUS * 2;
  const right = maxX - AVATAR_RADIUS;
  // Near the floor the band also runs out of room, so it narrows as it fades.
  const width = Math.min(full, hidden, right);
  const opacity = Math.min(1, hidden / full) * Math.min(1, room / full);
  return { left: right - width, width, opacity };
}

export function getWipLane(commits: readonly CommitSummary[], headOid: string | null): number {
  if (commits.length === 0) return 0;

  const headCommit = headOid ? commits.find((commit) => commit.oid === headOid) : undefined;
  return (headCommit ?? commits[0]).graph.lane;
}
export function buildEdgePath(
  startX: number,
  startY: number,
  endX: number,
  endY: number,
  parentVisible: boolean,
  mergeEdge: boolean,
  mergeRoute?: { x: number; corner: number },
): string {
  if (mergeEdge && parentVisible && mergeRoute && mergeRoute.x !== endX) {
    const mergeLaneX = mergeRoute.x;
    const corner = edgeCorner(startX, startY, mergeLaneX, endY);
    const approachX = mergeLaneX < startX ? mergeLaneX + corner : mergeLaneX - corner;
    // Keep the existing span's final bend even when the merge is one row
    // above its parent; the shorter approach must not shrink that bend.
    const endCorner = mergeRoute.corner;
    const turnX = endX < mergeLaneX ? mergeLaneX - endCorner : mergeLaneX + endCorner;
    return `M ${startX} ${startY} L ${approachX} ${startY}`
      + ` Q ${mergeLaneX} ${startY}, ${mergeLaneX} ${startY + corner}`
      + ` L ${mergeLaneX} ${endY - endCorner}`
      + ` Q ${mergeLaneX} ${endY}, ${turnX} ${endY}`
      + ` L ${endX} ${endY}`;
  }
  if (startX === endX) return `M ${startX} ${startY} L ${endX} ${endY}`;

  if (!parentVisible) {
    const bendY = Math.min(startY + ROW_STRIDE * 0.55, endY);
    return `M ${startX} ${startY} C ${startX} ${bendY}, ${endX} ${bendY}, ${endX} ${endY}`;
  }

  const corner = edgeCorner(startX, startY, endX, endY);

  if (mergeEdge) {
    const approachX = endX < startX ? endX + corner : endX - corner;
    return `M ${startX} ${startY} L ${approachX} ${startY}`
      + ` Q ${endX} ${startY}, ${endX} ${startY + corner}`
      + ` L ${endX} ${endY}`;
  }

  const turnX = endX < startX ? startX - corner : startX + corner;
  return `M ${startX} ${startY} L ${startX} ${endY - corner}`
    + ` Q ${startX} ${endY}, ${turnX} ${endY}`
    + ` L ${endX} ${endY}`;
}

export function edgeCorner(startX: number, startY: number, endX: number, endY: number): number {
  return Math.round(Math.min(
    EDGE_CORNER,
    Math.abs(endX - startX) / 3,
    Math.abs(endY - startY) / 3,
  ));
}

export function buildGraphGeometry(
  commits: readonly CommitSummary[],
  hasWip: boolean,
  // Omitted while the column is wide enough for every lane; otherwise it is the
  // pixel the parked lanes sit on. Colors are handed in during a column drag,
  // where they cannot have changed and the allocation pass is the expensive
  // half of this function.
  view?: { maxX?: number; width?: number; colors?: Map<string, number> },
): GraphGeometry {
  const maxX = view?.maxX ?? Number.POSITIVE_INFINITY;
  const commitIndex = new Map<string, number>();
  const colors = view?.colors ?? buildPresentationBranchColors(commits, hasWip);

  for (let index = 0; index < commits.length; index += 1) {
    const commit = commits[index];
    commitIndex.set(commit.oid, index);
  }

  const paths: GraphPath[] = [];
  const firstParentRoutes = new Map<string, { lane: number; color: number; corner: number }>();
  for (let index = 0; index < commits.length; index += 1) {
    const commit = commits[index];
    const commitPaths: GraphPath[] = [];
    let firstParentCorner = 0;

    for (let edgeIndex = 0; edgeIndex < commit.graph.edges.length; edgeIndex += 1) {
      const edge = commit.graph.edges[edgeIndex];
      const parentIndex = commitIndex.get(edge.parent_oid) ?? -1;
      const parentVisible = parentIndex > index;
      const targetIndex = parentVisible ? parentIndex : commits.length;
      const endLane = parentVisible ? commits[parentIndex].graph.lane : edge.to_lane;
      const startX = laneX(edge.from_lane, maxX);
      const startY = rowY(index);
      const endX = laneX(endLane, maxX);
      const endY = Math.min(rowY(targetIndex), commits.length * ROW_STRIDE - ROW_GAP);
      if (edgeIndex === 0) firstParentCorner = edgeCorner(startX, startY, endX, endY);
      // Join the existing branch span, which may bend into the parent's lane
      // only at its endpoint; turning early can run through unrelated nodes.
      const trunk = edge.merge && parentVisible ? firstParentRoutes.get(edge.parent_oid) : undefined;
      const mergeLane = trunk?.lane ?? endLane;
      const data = buildEdgePath(
        startX, startY, endX, endY, parentVisible, edge.merge,
        trunk ? { x: laneX(trunk.lane, maxX), corner: trunk.corner } : undefined,
      );
      const color = trunk?.color ?? getGraphEdgeColor(commit, edgeIndex, colors);

      const path = {
        key: `${commit.oid}:${edge.parent_oid}:${edgeIndex}`,
        data,
        color,
        paintLane: Math.max(edge.from_lane, endLane, mergeLane),
        merge: edge.merge,
        stash: Boolean(commit.stash),
        fromRow: index,
        toRow: targetIndex,
      };
      commitPaths.push(path);
    }

    const firstParent = commit.graph.edges[0];
    if (firstParent && !commit.stash) {
      firstParentRoutes.set(firstParent.parent_oid, {
        lane: firstParent.from_lane,
        color: commitPaths[0].color,
        corner: firstParentCorner,
      });
    }

    // SVG paints later paths on top. Prepending each older row keeps the
    // newest visible line above the older lines where several routes overlap.
    paths.unshift(...commitPaths);
  }

  paths.sort((left, right) => left.paintLane - right.paintLane);

  return {
    paths,
    colors,
    width: view?.width ?? getCommitGraphWidth(commits),
    height: commits.length * ROW_STRIDE - ROW_GAP,
  };
}

// The rows to mount for a viewport spanning `top`..`bottom`, both measured from
// the top of the list. Empty when there are no rows.
export function getCommitGraphRowWindow(top: number, bottom: number, count: number): RowWindow {
  if (count <= 0) return { first: 0, last: -1 };

  const firstVisible = Math.floor(Math.max(0, top) / ROW_STRIDE);
  const lastVisible = Math.floor(Math.max(0, bottom) / ROW_STRIDE);
  const last = Math.min(
    count - 1,
    (Math.floor((lastVisible + ROW_WINDOW_OVERSCAN) / ROW_WINDOW_CHUNK) + 1) * ROW_WINDOW_CHUNK - 1,
  );
  const first = Math.min(
    last,
    Math.max(0, Math.floor((firstVisible - ROW_WINDOW_OVERSCAN) / ROW_WINDOW_CHUNK) * ROW_WINDOW_CHUNK),
  );
  return { first, last };
}

// Routes that cross the window, in their original paint order: filtering never
// reorders, so the backmost-lane-on-top layering is untouched.
export function pathsInRowWindow(paths: readonly GraphPath[], window: RowWindow): GraphPath[] {
  return paths.filter((path) => path.fromRow <= window.last && path.toRow >= window.first);
}
