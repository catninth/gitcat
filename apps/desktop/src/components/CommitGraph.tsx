import { memo, useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Check, Cloud, Inbox, Monitor, Tag } from "lucide-react";
import type {
  CSSProperties,
  KeyboardEvent as ReactKeyboardEvent,
  MouseEvent as ReactMouseEvent,
  RefObject,
} from "react";

import {
  ALL_GRAPH_COLUMNS,
  DEFAULT_GRAPH_COLUMN_WIDTHS,
  effectiveGraphColumnWidth,
  isRefColumnIconOnly,
} from "../lib/columns";
import type {
  CommitSummary,
  GraphColumnSettings,
  GraphColumnWidths,
  RefLabel,
} from "../lib/types";
import { buildBranchColors as buildPresentationBranchColors } from "./graphPresentation";
import {
  AVATAR_RADIUS,
  FIRST_COLOR_SLOT,
  MERGE_NODE_RADIUS,
  ROW_GAP,
  ROW_HEIGHT,
  ROW_STRIDE,
  ROW_WINDOW_INITIAL,
  STASH_NODE_RADIUS,
  WIP_NODE_EDGE,
  WIP_ROW_Y,
  buildEdgePath,
  buildGraphGeometry,
  clampedLaneXCss,
  colorClass,
  colorVariable,
  getCommitGraphMaxX,
  getCommitGraphRowWindow,
  getCommitGraphWidth,
  getCommitRowBranchOrigin,
  isCommitGraphCollapsed,
  isMergeNode,
  laneX,
  pathsInRowWindow,
  rowY,
  type RowWindow,
} from "./commitGraphGeometry";
import {
  isBranchDecoration,
  remoteBranchNameWithoutRemote,
  remoteNameFromBranchName,
  visibleDecorations,
  type GraphRefLabel,
} from "./commitRefs";

const REVEAL_COMMIT_EVENT = "gitcat:reveal-commit";

const dateFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

export interface CommitContextMenuRequest {
  commit: CommitSummary;
  clientX: number;
  clientY: number;
  // Set when the click landed on a ref label; the menu offers ref-specific
  // actions for it instead of the plain commit actions.
  decoration?: RefLabel;
}

export interface WipConnector {
  lane: number;
  headOid: string | null;
  // The second parent of the merge the working copy is about to commit. Git
  // knows it before the commit exists, and the graph draws it dashed, the same
  // way the connection to HEAD is drawn.
  incomingOid?: string | null;
}

export interface CommitGraphProps {
  commits: readonly CommitSummary[];
  columns?: GraphColumnSettings;
  columnWidths?: GraphColumnWidths;
  selectedOid: string | null;
  wip?: WipConnector;
  beforeFirstSelected?: boolean;
  searchMatchOids?: ReadonlySet<string>;
  hideHeadDecoration?: boolean;
  detachedHeadOid?: string | null;
  remoteIconUrls?: ReadonlyMap<string, string>;
  /** Author avatars keyed by lower-cased email, inlined as `data:` URIs. */
  avatarImages?: ReadonlyMap<string, string>;
  onSelect: (commit: CommitSummary) => void;
  onNavigateBeforeFirst?: () => void;
  onCommitContextMenu?: (request: CommitContextMenuRequest) => void;
  onCopySha?: (oid: string) => void;
  onRefDoubleClick?: (decoration: RefLabel) => void;
  className?: string;
  emptyLabel?: string;
  formatTimestamp?: (seconds: number, offsetMinutes: number) => string;
  /**
   * The element the list scrolls in. With it the graph mounts only the rows
   * near the viewport; without it every row is rendered.
   */
  scrollContainerRef?: RefObject<HTMLElement | null>;
}

interface TimeMarker {
  key: string;
  label: string;
  top: number;
}

interface CommitRowProps {
  commit: CommitSummary;
  color: number;
  columns: GraphColumnSettings;
  compactRefs: boolean;
  id: string;
  index: number;
  selected: boolean;
  searchMatch: boolean;
  searchDimmed: boolean;
  hideHeadDecoration: boolean;
  hasMultipleBranches: boolean;
  detachedHeadOid?: string | null;
  remoteIconUrls?: ReadonlyMap<string, string>;
  avatarImages?: ReadonlyMap<string, string>;
  onSelect: (commit: CommitSummary) => void;
  onCommitContextMenu?: (request: CommitContextMenuRequest) => void;
  onCopySha?: (oid: string) => void;
  onRefDoubleClick?: (decoration: RefLabel) => void;
  formatTimestamp?: (seconds: number, offsetMinutes: number) => string;
}

// The node carries the author's picture once the hosting service has named
// them, and their initial until then. An image that fails to decode falls back
// as well, so a stale cache entry never leaves an empty node.
function CommitNodeFace({ image, initial }: { image?: string; initial: string }) {
  const [failed, setFailed] = useState(false);
  if (!image || failed) return initial;
  return (
    <img
      alt=""
      className="gc-commit-row__avatar-image"
      onError={() => setFailed(true)}
      src={image}
    />
  );
}

// Scrolls the commit list to a row that may not be mounted. A row outside the
// row window has no element to call `scrollIntoView` on, so the list does the
// scrolling from the row's index instead.
export function revealCommitRow(oid: string): void {
  document
    .querySelector<HTMLElement>("[data-commit-list]")
    ?.dispatchEvent(new CustomEvent<string>(REVEAL_COMMIT_EVENT, { detail: oid }));
}

function dateFromUnixSeconds(seconds: number): Date | null {
  const date = new Date(seconds * 1_000);
  return Number.isNaN(date.getTime()) ? null : date;
}

function defaultFormatTimestamp(seconds: number): string {
  const date = dateFromUnixSeconds(seconds);
  return date ? dateFormatter.format(date) : "Unknown date";
}

// The graph row is a single line, so the description body collapses onto it
// with its line breaks rendered as pipes.
function descriptionPreview(commit: CommitSummary): string {
  const preview = commit.body_preview
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .join(" | ");
  return preview === commit.subject.trim() ? "" : preview;
}

function plural(value: number, unit: string): string {
  return `${value} ${unit}${value === 1 ? "" : "s"} ago`;
}

function relativeTimeMarkerLabel(seconds: number, nowSeconds: number): string {
  const elapsedSeconds = Math.max(0, nowSeconds - seconds);
  const elapsedHours = Math.floor(elapsedSeconds / 3_600);
  const elapsedDays = Math.floor(elapsedSeconds / 86_400);

  // Coarse buckets like GitKraken: no sub-hour subdivision, so a batch of
  // commits made minutes apart shares one bucket and produces no divider.
  if (elapsedHours < 1) return "less than an hour ago";
  if (elapsedDays < 1) return elapsedHours === 1 ? "an hour ago" : plural(elapsedHours, "hour");
  if (elapsedDays === 1) return "yesterday";
  if (elapsedDays < 7) return plural(elapsedDays, "day");
  if (elapsedDays < 14) return "a week ago";
  if (elapsedDays < 30) return plural(Math.floor(elapsedDays / 7), "week");
  if (elapsedDays < 60) return "a month ago";
  if (elapsedDays < 365) return plural(Math.floor(elapsedDays / 30), "month");
  if (elapsedDays < 730) return "a year ago";
  return plural(Math.floor(elapsedDays / 365), "year");
}

function buildTimeMarkers(commits: readonly CommitSummary[], nowSeconds: number): TimeMarker[] {
  const markers: TimeMarker[] = [];
  let previousLabel: string | null = null;

  for (let index = 0; index < commits.length; index += 1) {
    const commit = commits[index];
    const label = relativeTimeMarkerLabel(commit.authored_at.seconds, nowSeconds);
    if (index > 0 && label !== previousLabel) {
      markers.push({
        key: `${commit.oid}:${label}`,
        label,
        top: Math.max(0, index * ROW_STRIDE - ROW_GAP / 2),
      });
    }
    previousLabel = label;
  }

  return markers;
}

function RefLabelPill({
  compact,
  decoration,
  inactive,
  linkedRemote,
  remoteIconUrl,
  linkedRemoteIconUrl,
  onContextMenu,
  onDoubleClick,
}: {
  compact: boolean;
  decoration: GraphRefLabel;
  inactive: boolean;
  linkedRemote?: GraphRefLabel;
  remoteIconUrl?: string;
  linkedRemoteIconUrl?: string;
  onContextMenu?: (decoration: RefLabel, event: ReactMouseEvent<HTMLElement>) => void;
  onDoubleClick?: (decoration: RefLabel) => void;
}) {
  const [remoteImageFailed, setRemoteImageFailed] = useState(false);
  const [linkedRemoteImageFailed, setLinkedRemoteImageFailed] = useState(false);
  const Icon = decoration.kind === "remote_branch"
    ? Cloud
    : decoration.kind === "tag"
      ? Tag
      : Monitor;
  const classes = [
    "gc-ref-label",
    `gc-ref-label--${decoration.kind}`,
    decoration.is_head ? "gc-ref-label--head" : "",
    inactive ? "gc-ref-label--inactive" : "",
    compact ? "gc-ref-label--compact" : "",
  ].filter(Boolean).join(" ");
  const displayName = decoration.kind === "remote_branch"
    ? remoteBranchNameWithoutRemote(decoration.name)
    : decoration.name;
  const canCheckout = isBranchDecoration(decoration) && !decoration.synthetic;

  return (
    <span
      className={classes}
      onContextMenu={onContextMenu && !decoration.synthetic ? (event) => {
        event.preventDefault();
        event.stopPropagation();
        onContextMenu(decoration, event);
      } : undefined}
      onDoubleClick={canCheckout && onDoubleClick ? (event) => {
        event.stopPropagation();
        onDoubleClick(decoration);
      } : undefined}
      title={linkedRemote ? `${decoration.full_name}\n${linkedRemote.full_name}` : decoration.full_name}
    >
      {decoration.is_head ? <Check aria-hidden="true" size={12} strokeWidth={3} /> : null}
      {compact ? null : <span className="gc-ref-label__name">{displayName}</span>}
      {compact && decoration.is_head ? null : remoteIconUrl && !remoteImageFailed ? (
        <img
          alt=""
          aria-hidden="true"
          className="gc-ref-label__remote-avatar"
          onError={() => setRemoteImageFailed(true)}
          src={remoteIconUrl}
        />
      ) : (
        <Icon aria-hidden="true" size={decoration.is_head ? 12 : 10} strokeWidth={2.4} />
      )}
      {linkedRemote && !compact ? (
        linkedRemoteIconUrl && !linkedRemoteImageFailed ? (
          <img
            alt=""
            aria-hidden="true"
            className="gc-ref-label__remote-avatar gc-ref-label__remote-icon"
            onError={() => setLinkedRemoteImageFailed(true)}
            src={linkedRemoteIconUrl}
          />
        ) : (
          <Cloud aria-hidden="true" className="gc-ref-label__remote-icon" size={12} strokeWidth={2.4} />
        )
      ) : null}
    </span>
  );
}

function CommitRefStack({
  compact,
  decorations,
  hasMultipleBranches,
  remoteIconUrls,
  onRefContextMenu,
  onRefDoubleClick,
}: {
  compact: boolean;
  decorations: readonly GraphRefLabel[];
  hasMultipleBranches: boolean;
  remoteIconUrls?: ReadonlyMap<string, string>;
  onRefContextMenu?: (decoration: RefLabel, event: ReactMouseEvent<HTMLElement>) => void;
  onRefDoubleClick?: (decoration: RefLabel) => void;
}) {
  if (decorations.length === 0) return null;

  const hiddenRemoteIndexes = new Set<number>();
  const linkedRemotes = new Map<string, GraphRefLabel>();
  const localBranches = decorations.filter((decoration) => decoration.kind === "local_branch");
  for (const localBranch of localBranches) {
    const remoteIndex = decorations.findIndex((decoration, index) => (
      !hiddenRemoteIndexes.has(index)
        && decoration.kind === "remote_branch"
        && remoteBranchNameWithoutRemote(decoration.name) === localBranch.name
    ));
    if (remoteIndex >= 0) {
      hiddenRemoteIndexes.add(remoteIndex);
      linkedRemotes.set(decorations[remoteIndex].full_name, decorations[remoteIndex]);
      linkedRemotes.set(localBranch.full_name, decorations[remoteIndex]);
    }
  }

  const displayDecorations = decorations.filter((_, index) => !hiddenRemoteIndexes.has(index));
  const shouldStack = displayDecorations.length > 1;
  const [primary, ...rest] = displayDecorations;
  const isInactive = (decoration: GraphRefLabel) =>
    decoration.kind === "remote_branch"
      || (hasMultipleBranches && decoration.kind === "local_branch" && !decoration.is_head);
  const primaryInactive = isInactive(primary);
  const remoteIconUrl = (decoration?: GraphRefLabel) => {
    if (!decoration) return undefined;
    const remoteName = remoteNameFromBranchName(decoration.name);
    return remoteName ? remoteIconUrls?.get(remoteName) : undefined;
  };

  return (
    <span className={`gc-ref-stack${shouldStack ? " gc-ref-stack--stacked" : ""}${primaryInactive ? " gc-ref-stack--inactive" : ""}${compact ? " gc-ref-stack--compact" : ""}`}>
      <RefLabelPill
        compact={compact}
        decoration={primary}
        inactive={primaryInactive}
        linkedRemote={linkedRemotes.get(primary.full_name)}
        linkedRemoteIconUrl={remoteIconUrl(linkedRemotes.get(primary.full_name))}
        onContextMenu={onRefContextMenu}
        onDoubleClick={onRefDoubleClick}
        remoteIconUrl={remoteIconUrl(primary)}
      />
      {shouldStack ? (
        <>
          {compact ? null : (
            <span aria-hidden="true" className="gc-ref-stack__count">{`+${rest.length}`}</span>
          )}
          <span className="gc-ref-stack__overflow">
            {rest.map((decoration) => (
              <RefLabelPill
                compact={false}
                decoration={decoration}
                inactive={isInactive(decoration)}
                key={decoration.full_name}
                linkedRemote={linkedRemotes.get(decoration.full_name)}
                linkedRemoteIconUrl={remoteIconUrl(linkedRemotes.get(decoration.full_name))}
                onContextMenu={onRefContextMenu}
                onDoubleClick={onRefDoubleClick}
                remoteIconUrl={remoteIconUrl(decoration)}
              />
            ))}
          </span>
        </>
      ) : null}
    </span>
  );
}

function RowShaButton({ oid, shortOid, onCopy }: { oid: string; shortOid: string; onCopy: (oid: string) => void }) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const [tooltipPosition, setTooltipPosition] = useState<{ left: number; top: number } | null>(null);
  const showTooltip = () => {
    const bounds = buttonRef.current?.getBoundingClientRect();
    if (!bounds) return;
    const width = Math.min(340, window.innerWidth - 16);
    const left = Math.max(8, Math.min(bounds.right - width, window.innerWidth - width - 8));
    const top = bounds.bottom + 58 <= window.innerHeight ? bounds.bottom + 7 : Math.max(8, bounds.top - 55);
    setTooltipPosition({ left, top });
  };

  return (
    <>
      <button
        aria-label={`Copy full commit SHA ${oid}`}
        className="relative cursor-pointer rounded-[3px] bg-transparent px-0.75 py-0.5 font-mono text-[9px] leading-none text-muted/78 hover:bg-accent/11 hover:text-accent focus-visible:bg-accent/11 focus-visible:text-accent"
        onBlur={() => setTooltipPosition(null)}
        onClick={(event) => {
          event.stopPropagation();
          onCopy(oid);
        }}
        onFocus={showTooltip}
        onMouseEnter={showTooltip}
        onMouseLeave={() => setTooltipPosition(null)}
        ref={buttonRef}
        type="button"
      >
        {shortOid}
      </button>
      {tooltipPosition ? createPortal(
        <span
          className="pointer-events-none fixed z-260 flex w-[min(340px,calc(100vw-16px))] flex-col gap-0.75 rounded-[5px] border border-border bg-menu px-2.25 py-1.75 text-foreground shadow-panel"
          role="tooltip"
          style={tooltipPosition}
        >
          <code className="font-mono text-[10px] leading-[1.45] wrap-anywhere">{oid}</code>
          <small className="text-[9px] text-muted">Click to copy</small>
        </span>,
        document.body,
      ) : null}
    </>
  );
}

const CommitRow = memo(function CommitRow({
  commit,
  color,
  columns,
  compactRefs,
  id,
  index,
  selected,
  searchMatch,
  searchDimmed,
  hideHeadDecoration,
  hasMultipleBranches,
  detachedHeadOid,
  remoteIconUrls,
  avatarImages,
  onSelect,
  onCommitContextMenu,
  onCopySha,
  onRefDoubleClick,
  formatTimestamp,
}: CommitRowProps) {
  const openContextMenu = (clientX: number, clientY: number, decoration?: RefLabel) => {
    onCommitContextMenu?.({ commit, clientX, clientY, decoration });
  };

  const handleContextMenu = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (!onCommitContextMenu) return;

    event.preventDefault();
    openContextMenu(event.clientX, event.clientY);
  };

  const timestamp = formatTimestamp
    ? formatTimestamp(commit.authored_at.seconds, commit.authored_at.offset_minutes)
    : defaultFormatTimestamp(commit.authored_at.seconds);
  const authoredDate = dateFromUnixSeconds(commit.authored_at.seconds);
  const description = descriptionPreview(commit);
  const rowTitle = commit.body_preview && commit.body_preview !== commit.subject
    ? `${commit.subject}\n\n${commit.body_preview}`
    : commit.subject;
  const initials = commit.author.name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join("");
  const stateClasses = [
    "gc-commit-row",
    selected ? "gc-commit-row--selected" : "",
    searchMatch ? "gc-commit-row--search-match" : "",
    searchDimmed ? "gc-commit-row--search-dimmed" : "",
  ].filter(Boolean).join(" ");
  const accessibleLabel = [
    commit.subject,
    commit.stash ? `stash ${commit.stash.selector}` : `commit ${commit.short_oid}`,
    commit.author.name,
    timestamp,
    searchMatch ? "search result" : "",
  ].filter(Boolean).join(", ");
  const branchOrigin = clampedLaneXCss(commit.graph.lane);
  const nodeRadius = isMergeNode(commit) ? MERGE_NODE_RADIUS : AVATAR_RADIUS;
  const rowStyle = {
    "--gc-branch-row-origin": getCommitRowBranchOrigin(commit.graph.lane, columns, nodeRadius),
    "--gc-row-branch-color": colorVariable(color),
  } as CSSProperties;
  const graphSlotStyle = {
    "--gc-branch-origin": branchOrigin,
    "--gc-branch-interactive-origin": `calc(${branchOrigin} + ${AVATAR_RADIUS}px)`,
  } as CSSProperties;
  const decorations = visibleDecorations(commit, hideHeadDecoration, detachedHeadOid);

  return (
    <div
      aria-label={accessibleLabel}
      aria-rowindex={index + 1}
      aria-selected={selected}
      className={stateClasses}
      data-commit-index={index}
      data-oid={commit.oid}
      id={id}
      onClick={(event) => {
        event.currentTarget.closest<HTMLElement>("[data-commit-list]")?.focus({ preventScroll: true });
        onSelect(commit);
      }}
      onContextMenu={handleContextMenu}
      role="row"
      style={rowStyle}
    >
      {columns.refs ? (
        <span aria-label="References" className="gc-commit-row__decorations" role="cell">
          <CommitRefStack
            compact={compactRefs}
            decorations={decorations}
            hasMultipleBranches={hasMultipleBranches}
            onRefContextMenu={onCommitContextMenu
              ? (decoration, event) => openContextMenu(event.clientX, event.clientY, decoration)
              : undefined}
            onRefDoubleClick={onRefDoubleClick}
            remoteIconUrls={remoteIconUrls}
          />
        </span>
      ) : null}
      {columns.graph ? (
        <span
          aria-label="Graph"
          className={colorClass("gc-commit-row__graph-slot", color)}
          role="cell"
          style={graphSlotStyle}
        >
          {isMergeNode(commit) ? (
            <span
              aria-hidden="true"
              className="gc-commit-row__merge-node"
              style={{ left: branchOrigin }}
            />
          ) : (
            <span
              aria-hidden="true"
              className={[
                "gc-commit-row__avatar",
                selected ? "gc-commit-row__avatar--selected" : "",
                commit.stash ? "gc-commit-row__avatar--stash" : "",
              ].filter(Boolean).join(" ")}
              style={{ left: branchOrigin }}
            >
              {commit.stash ? (
                <Inbox size={11} strokeWidth={2.4} />
              ) : (
                <CommitNodeFace
                  image={avatarImages?.get(commit.author.email.trim().toLowerCase())}
                  initial={initials.slice(0, 1) || "?"}
                />
              )}
            </span>
          )}
        </span>
      ) : null}
      {columns.message ? (
        <span className="gc-commit-row__subject" role="cell" title={rowTitle}>
          {commit.subject || "(no commit message)"}
          {description ? <span className="gc-commit-row__description">{description}</span> : null}
        </span>
      ) : null}
      {columns.author ? (
        <span className="gc-commit-row__author" role="cell" title={commit.author.email}>
          {commit.author.name}
        </span>
      ) : null}
      {columns.date ? (
        <time className="gc-commit-row__time" dateTime={authoredDate?.toISOString()} role="cell" title={timestamp}>
          {timestamp}
        </time>
      ) : null}
      {columns.sha ? (
        <span className="gc-commit-row__oid-wrap" role="cell">
          {onCopySha ? (
            <RowShaButton oid={commit.oid} onCopy={onCopySha} shortOid={commit.short_oid} />
          ) : <span className="font-mono text-muted/70">{commit.short_oid}</span>}
        </span>
      ) : null}
    </div>
  );
});

export function CommitGraph({
  commits,
  columns = ALL_GRAPH_COLUMNS,
  columnWidths = DEFAULT_GRAPH_COLUMN_WIDTHS,
  selectedOid,
  wip,
  beforeFirstSelected = false,
  searchMatchOids,
  hideHeadDecoration = false,
  detachedHeadOid = null,
  remoteIconUrls,
  avatarImages,
  onSelect,
  onNavigateBeforeFirst,
  onCommitContextMenu,
  onCopySha,
  onRefDoubleClick,
  className,
  emptyLabel = "No commits to display.",
  formatTimestamp,
  scrollContainerRef,
}: CommitGraphProps) {
  const listRef = useRef<HTMLDivElement>(null);
  const [measuredWindow, setMeasuredWindow] = useState<RowWindow>({ first: 0, last: ROW_WINDOW_INITIAL - 1 });
  const commitCount = commits.length;
  // Clamped here as well as when measured: a shorter history arrives one render
  // before the effect below gets to measure it.
  const rowWindow = useMemo<RowWindow>(() => {
    if (!scrollContainerRef) return { first: 0, last: commitCount - 1 };
    const last = Math.min(measuredWindow.last, commitCount - 1);
    return { first: Math.max(0, Math.min(measuredWindow.first, last)), last };
  }, [commitCount, measuredWindow, scrollContainerRef]);

  useLayoutEffect(() => {
    const scroller = scrollContainerRef?.current;
    if (!scroller) return undefined;

    const measure = () => {
      const list = listRef.current;
      if (!list) return;
      // The list sits below the WIP row inside the scrolled content, so its
      // offset is read rather than assumed.
      const top = scroller.getBoundingClientRect().top - list.getBoundingClientRect().top;
      const next = getCommitGraphRowWindow(top, top + scroller.clientHeight, commitCount);
      setMeasuredWindow((current) => (
        current.first === next.first && current.last === next.last ? current : next
      ));
    };

    measure();
    scroller.addEventListener("scroll", measure, { passive: true });
    const observer = new ResizeObserver(measure);
    observer.observe(scroller);
    return () => {
      scroller.removeEventListener("scroll", measure);
      observer.disconnect();
    };
  }, [commitCount, scrollContainerRef]);

  const scrollRowIntoView = useCallback((index: number, block: "nearest" | "center") => {
    const scroller = scrollContainerRef?.current;
    const list = listRef.current;
    if (!scroller || !list) {
      list?.querySelector<HTMLElement>(`[data-commit-index="${index}"]`)?.scrollIntoView({ block });
      return;
    }

    const listTop = list.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
    const rowTop = listTop + index * ROW_STRIDE;
    const rowBottom = rowTop + ROW_HEIGHT;
    if (block === "center") {
      scroller.scrollTop = rowTop + ROW_HEIGHT / 2 - scroller.clientHeight / 2;
    } else if (rowTop < scroller.scrollTop) {
      scroller.scrollTop = rowTop;
    } else if (rowBottom > scroller.scrollTop + scroller.clientHeight) {
      scroller.scrollTop = rowBottom - scroller.clientHeight;
    }
  }, [scrollContainerRef]);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return undefined;

    const reveal = (event: Event) => {
      const index = commits.findIndex((commit) => commit.oid === (event as CustomEvent<string>).detail);
      if (index >= 0) scrollRowIntoView(index, "center");
    };
    list.addEventListener(REVEAL_COMMIT_EVENT, reveal);
    return () => list.removeEventListener(REVEAL_COMMIT_EVENT, reveal);
  }, [commits, scrollRowIntoView]);
  const nodeMaskId = `gc-node-mask-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const hasWip = Boolean(wip);
  const laneExtentWidth = useMemo(() => getCommitGraphWidth(commits), [commits]);
  const graphWidth = effectiveGraphColumnWidth(columnWidths, laneExtentWidth);
  const maxX = getCommitGraphMaxX(graphWidth);
  // Lane colors depend only on the history, so a column drag reuses them.
  const colors = useMemo(() => buildPresentationBranchColors(commits, hasWip), [commits, hasWip]);
  const geometry = useMemo(
    () => buildGraphGeometry(commits, hasWip, { colors, maxX, width: laneExtentWidth }),
    [colors, commits, hasWip, laneExtentWidth, maxX],
  );
  const compactRefs = isRefColumnIconOnly(columns, columnWidths);
  const lanesCollapsed = isCommitGraphCollapsed(graphWidth);
  const wipPath = useMemo(() => {
    if (!wip?.headOid) return null;

    const headIndex = commits.findIndex((commit) => commit.oid === wip.headOid);
    if (headIndex < 0) return null;

    const headLane = commits[headIndex].graph.lane;
    return {
      data: buildEdgePath(
        laneX(wip.lane, maxX),
        WIP_ROW_Y + WIP_NODE_EDGE,
        laneX(headLane, maxX),
        rowY(headIndex),
        true,
        false,
      ),
      lane: Math.max(wip.lane, headLane),
      color: FIRST_COLOR_SLOT,
    };
  }, [commits, maxX, wip?.headOid, wip?.lane]);
  // The incoming side leaves the WIP node sideways and drops into its own lane,
  // exactly like a committed merge, and keeps that lane's colour: the working
  // copy is the merge, and this is the parent it has not committed yet.
  const wipMergePath = useMemo(() => {
    if (!wip?.incomingOid) return null;

    const incomingIndex = commits.findIndex((commit) => commit.oid === wip.incomingOid);
    if (incomingIndex < 0) return null;

    const incomingLane = commits[incomingIndex].graph.lane;
    const startX = laneX(wip.lane, maxX);
    const endX = laneX(incomingLane, maxX);
    if (startX === endX) return null;

    return {
      data: buildEdgePath(
        startX + (endX < startX ? -WIP_NODE_EDGE : WIP_NODE_EDGE),
        WIP_ROW_Y,
        endX,
        rowY(incomingIndex),
        true,
        true,
      ),
      color: geometry.colors.get(wip.incomingOid) ?? FIRST_COLOR_SLOT,
    };
  }, [commits, geometry, maxX, wip?.incomingOid, wip?.lane]);
  // The mask reaches the WIP node's own row when the incoming edge starts
  // there; everything above the mask is painted away.
  const maskTop = wipMergePath ? WIP_ROW_Y : wipPath ? WIP_ROW_Y + WIP_NODE_EDGE : 0;
  const timeMarkers = useMemo(() => buildTimeMarkers(commits, Math.floor(Date.now() / 1_000)), [commits]);
  const hasMultipleBranches = useMemo(() => {
    const branchNames = new Set<string>();
    for (const commit of commits) {
      for (const decoration of commit.decorations) {
        if (isBranchDecoration(decoration)) branchNames.add(decoration.full_name);
      }
    }
    return branchNames.size > 1;
  }, [commits]);
  const selectedIndex = useMemo(
    () => commits.findIndex((commit) => commit.oid === selectedOid),
    [commits, selectedOid],
  );
  const searchActive = (searchMatchOids?.size ?? 0) > 0;
  const activeCommit = selectedIndex >= 0 ? commits[selectedIndex] : beforeFirstSelected ? undefined : commits[0];
  const activeDescendant = activeCommit ? `commit-row-${activeCommit.oid}` : undefined;

  // Nothing in a row depends on the graph column width -- the node position and
  // the branch stripe are CSS expressions over --gc-graph-max-x. Memoizing the
  // list therefore lets a column drag skip the whole row subtree, provided the
  // callers keep their handler identities stable.
  const rows = useMemo(() => commits.slice(rowWindow.first, rowWindow.last + 1).map((commit, offset) => {
    const index = rowWindow.first + offset;
    return (
    <CommitRow
      color={geometry.colors.get(commit.oid) ?? FIRST_COLOR_SLOT}
      columns={columns}
      compactRefs={compactRefs}
      commit={commit}
      detachedHeadOid={detachedHeadOid}
      formatTimestamp={formatTimestamp}
      hasMultipleBranches={hasMultipleBranches}
      hideHeadDecoration={hideHeadDecoration}
      id={`commit-row-${commit.oid}`}
      index={index}
      key={commit.oid}
      onCommitContextMenu={onCommitContextMenu}
      onCopySha={onCopySha}
      onRefDoubleClick={onRefDoubleClick}
      onSelect={onSelect}
      remoteIconUrls={remoteIconUrls}
      avatarImages={avatarImages}
      searchDimmed={
        searchActive
        && commit.oid !== selectedOid
        && !searchMatchOids?.has(commit.oid)
      }
      searchMatch={searchMatchOids?.has(commit.oid) ?? false}
      selected={commit.oid === selectedOid}
    />
    );
  }), [
    columns,
    commits,
    compactRefs,
    detachedHeadOid,
    formatTimestamp,
    geometry.colors,
    hasMultipleBranches,
    hideHeadDecoration,
    onCommitContextMenu,
    onCopySha,
    onRefDoubleClick,
    onSelect,
    remoteIconUrls,
    avatarImages,
    searchActive,
    searchMatchOids,
    selectedOid,
    rowWindow,
  ]);
  const windowPaths = useMemo(
    () => pathsInRowWindow(geometry.paths, rowWindow),
    [geometry.paths, rowWindow],
  );
  // The svg covers the row window only, so the mask and every route raster at
  // the size of the viewport rather than of the whole loaded history. The top
  // window keeps its origin at zero, where the WIP connectors reach above it.
  const svgTop = rowWindow.first * ROW_STRIDE;
  const svgBottom = rowWindow.last >= commits.length - 1
    ? geometry.height
    : (rowWindow.last + 1) * ROW_STRIDE;
  const svgHeight = Math.max(0, svgBottom - svgTop);
  const windowMaskTop = rowWindow.first === 0 ? maskTop : svgTop;

  const selectIndex = (index: number) => {
    const commit = commits[index];
    if (!commit) return;

    onSelect(commit);
    scrollRowIntoView(index, "nearest");
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;

    const currentIndex = selectedIndex >= 0 ? selectedIndex : beforeFirstSelected ? -1 : 0;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        selectIndex(Math.min(currentIndex + 1, commits.length - 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        if (currentIndex <= 0 && onNavigateBeforeFirst) onNavigateBeforeFirst();
        else selectIndex(Math.max(currentIndex - 1, 0));
        break;
      case "Home":
        event.preventDefault();
        selectIndex(0);
        break;
      case "End":
        event.preventDefault();
        selectIndex(commits.length - 1);
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        selectIndex(currentIndex);
        break;
      case "F10":
        if (event.shiftKey && onCommitContextMenu) {
          event.preventDefault();
          const selectedCommit = activeCommit ?? commits[0];
          const selectedRow = listRef.current
            ?.querySelector<HTMLElement>(`[data-oid="${selectedCommit.oid}"]`);
          const bounds = selectedRow?.getBoundingClientRect() ?? event.currentTarget.getBoundingClientRect();
          onSelect(selectedCommit);
          onCommitContextMenu({
            commit: selectedCommit,
            clientX: bounds.left + Math.min(geometry.width + 24, bounds.width / 2),
            clientY: bounds.top + bounds.height / 2,
          });
        }
        break;
      default:
        break;
    }
  };

  if (commits.length === 0) {
    return (
      <div className={`gc-commit-graph gc-commit-graph--empty${className ? ` ${className}` : ""}`} role="status">
        {emptyLabel}
      </div>
    );
  }

  return (
    <div
      aria-label="Commit history"
      aria-activedescendant={activeDescendant}
      aria-rowcount={commits.length}
      className={`gc-commit-graph${className ? ` ${className}` : ""}`}
      data-commit-list
      onKeyDown={handleKeyDown}
      ref={listRef}
      role="grid"
      tabIndex={0}
    >
      <div aria-hidden="true" className="gc-commit-time-markers gc-commit-time-markers--lines">
        {timeMarkers.map((marker) => (
          <span className="gc-commit-time-marker" key={marker.key} style={{ top: marker.top }} />
        ))}
      </div>
      {columns.graph && !lanesCollapsed ? (
        <svg
          aria-hidden="true"
          className="gc-commit-graph__lanes"
          focusable="false"
          height={svgHeight}
          style={{ left: "var(--gc-graph-offset, 0px)", top: svgTop }}
          viewBox={`0 ${svgTop} ${geometry.width} ${svgHeight}`}
          width={geometry.width}
        >
          <defs>
            <mask
              height={svgBottom - windowMaskTop}
              id={nodeMaskId}
              maskUnits="userSpaceOnUse"
              width={geometry.width}
              x={0}
              y={windowMaskTop}
            >
              <rect fill="white" height={svgBottom - windowMaskTop} width={geometry.width} x={0} y={windowMaskTop} />
              {commits.slice(rowWindow.first, rowWindow.last + 1).map((commit, offset) => {
                const index = rowWindow.first + offset;
                return commit.stash ? (
                <rect
                  fill="black"
                  height={AVATAR_RADIUS * 2}
                  key={commit.oid}
                  rx={STASH_NODE_RADIUS}
                  width={AVATAR_RADIUS * 2}
                  x={laneX(commit.graph.lane, maxX) - AVATAR_RADIUS}
                  y={rowY(index) - AVATAR_RADIUS}
                />
              ) : (
                <circle
                  cx={laneX(commit.graph.lane, maxX)}
                  cy={rowY(index)}
                  fill="black"
                  key={commit.oid}
                  r={isMergeNode(commit) ? MERGE_NODE_RADIUS - 1 : AVATAR_RADIUS}
                />
              );
              })}
            </mask>
          </defs>
          <g mask={`url(#${nodeMaskId})`}>
            {wipPath ? (
              <path
                className={`${colorClass("gc-commit-graph__edge", wipPath.color)} gc-commit-graph__edge--wip`}
                d={wipPath.data}
                fill="none"
                vectorEffect="non-scaling-stroke"
              />
            ) : null}
            {wipMergePath ? (
              <path
                className={`${colorClass("gc-commit-graph__edge", wipMergePath.color)} gc-commit-graph__edge--wip`}
                d={wipMergePath.data}
                fill="none"
                vectorEffect="non-scaling-stroke"
              />
            ) : null}
            {windowPaths.map((path) => (
              <path
                className={`${colorClass("gc-commit-graph__edge", path.color)}${path.merge ? " gc-commit-graph__edge--merge" : ""}${path.stash ? " gc-commit-graph__edge--stash" : ""}`}
                d={path.data}
                fill="none"
                key={path.key}
                vectorEffect="non-scaling-stroke"
              />
            ))}
          </g>
        </svg>
      ) : null}
      <div
        className="gc-commit-graph__rows"
        // Unmounted rows keep their space, so the scroll height, the time
        // markers and every row's position match the fully rendered list.
        style={{
          paddingTop: rowWindow.first * ROW_STRIDE,
          paddingBottom: Math.max(0, commits.length - 1 - rowWindow.last) * ROW_STRIDE,
        }}
      >
        {rows}
      </div>
      <div aria-hidden="true" className="gc-commit-time-markers gc-commit-time-markers--labels">
        {timeMarkers.map((marker) => (
          <span className="gc-commit-time-marker__label" key={marker.key} style={{ top: marker.top }}>
            {marker.label}
          </span>
        ))}
      </div>
    </div>
  );
}
