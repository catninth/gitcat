import { useCallback, useMemo, useRef, useState } from "react";

import {
    CommitGraph,
    getCommitGraphLastLaneX,
    getCommitGraphMaxX,
    getCommitGraphOverflowEdge,
    type CommitContextMenuRequest,
} from "../components/CommitGraph";
import { GraphColumnResizer } from "../components/GraphColumnResizer";
import { GraphConflictNotice } from "../components/GraphConflictNotice";
import { MergeEditor } from "../components/conflict";
import { DiffViewer, type DiffLineActions, type DiffViewMode } from "../components/diff";
import { ChangeCountSummary, type FileChangeCounts } from "../components/file-tree";
import { GraphColumnMenu } from "../components/GraphColumnMenu";
import { InitialCommitPrompt } from "../components/InitialCommitPrompt";
import { SearchBar } from "../components/SearchBar";
import { Button, Spinner } from "../components/ui";
import { gitcatApi } from "../lib/api";
import {
    effectiveGraphColumnWidth,
    graphColumnOffset,
    type GraphColumnKey,
    GRAPH_COLUMN_MIN_WIDTH,
    GRAPH_COLUMNS,
    graphColumnsMinWidth,
    graphColumnsTemplate,
    graphColumnWidth as resolveColumnWidth,
    MAX_GRAPH_COLUMN_WIDTH,
    MIN_GRAPH_COLUMN_WIDTH,
    visibleGraphColumns,
} from "../lib/columns";
import type {
    AppSettings,
    BranchInfo,

    CommitSummary,
    ConflictFileDetails,
    ConflictLineEndingPolicy,
    FileDiff,
    GraphColumnSettings,
    GraphColumnWidths,
    HistoryPage,
    RefLabel,
    RepositorySnapshot,
} from "../lib/types";
import { currentBranch } from "./branches";
import type { CenterView, CommitMenuState, RunMutation } from "./state";

export interface HistoryPaneProps {
    activeConflictCount: number;
    busy: boolean;
    centerView: CenterView;
    checkoutRemoteBranch: (branch: BranchInfo) => void;
    closeConflictEditor: () => void;
    closeDiff: () => void;
    conflictEditor: ConflictFileDetails | null;
    columns: GraphColumnSettings;
    columnWidths: GraphColumnWidths;
    copySha: (oid: string) => Promise<void>;
    createInitialCommit: () => void;
    currentHeadOid: string | null;
    diff: FileDiff | null;
    diffLineActions: DiffLineActions | null;
    diffLoading: boolean;
    diffMode: DiffViewMode;
    focusWorktree: () => void;
    graphLaneExtent: number;
    graphMatches: Set<string>;
    history: HistoryPage | null;
    historyLoading: boolean;
    loadMoreHistory: () => void;
    navigateSearch: (direction: 1 | -1) => void;
    overviewLoading: boolean;
    remoteCheckPending: boolean;
    remoteIconUrls: Map<string, string>;
    avatarImages: ReadonlyMap<string, string>;
    repositoryName: string;
    runMutation: RunMutation;
    saveConflictResult: (text: string, lineEnding: ConflictLineEndingPolicy) => void;
    searchBusy: boolean;
    searchFocusToken: number;
    searchIndex: number;
    searchOids: string[];
    searchOpen: boolean;
    searchQuery: string;
    selectCommit: (commit: CommitSummary) => void;
    selectFirstCommitFromWip: () => void;
    selectWip: () => void;
    selectWipFromGraph: () => void;
    selectedOid: string | null;
    setColumns: (columns: GraphColumnSettings) => void;
    setColumnWidths: (widths: GraphColumnWidths) => void;
    setCommitMenu: (menu: CommitMenuState | null) => void;
    setDiffMode: (mode: DiffViewMode) => void;
    setSearchOpen: (open: boolean) => void;
    setSearchQuery: (query: string) => void;
    setWipDraftMessage: (message: string) => void;
    settings: AppSettings;
    snapshot: RepositorySnapshot | null;
    wipDraftMessage: string;
    wipLane: number;
    wipRowRef: React.RefObject<HTMLDivElement | null>;
    wipRowStyle: React.CSSProperties;
    wipSelected: boolean;
    wipStats: FileChangeCounts;
    wipTitleHint: string | null;
    worktreeReachable: boolean;
}

export function HistoryPane({
    activeConflictCount,
    busy,
    centerView,
    checkoutRemoteBranch,
    closeConflictEditor,
    closeDiff,
    conflictEditor,
    columns,
    columnWidths,
    copySha,
    createInitialCommit,
    currentHeadOid,
    diff,
    diffLineActions,
    diffLoading,
    diffMode,
    focusWorktree,
    graphLaneExtent,
    graphMatches,
    history,
    historyLoading,
    loadMoreHistory,
    navigateSearch,
    overviewLoading,
    remoteCheckPending,
    remoteIconUrls,
    avatarImages,
    repositoryName,
    runMutation,
    saveConflictResult,
    searchBusy,
    searchFocusToken,
    searchIndex,
    searchOids,
    searchOpen,
    searchQuery,
    selectCommit,
    selectFirstCommitFromWip,
    selectWip,
    selectWipFromGraph,
    selectedOid,
    setColumns,
    setColumnWidths,
    setCommitMenu,
    setDiffMode,
    setSearchOpen,
    setSearchQuery,
    setWipDraftMessage,
    settings,
    snapshot,
    wipDraftMessage,
    wipLane,
    wipRowRef,
    wipRowStyle,
    wipSelected,
    wipStats,
    wipTitleHint,
    worktreeReachable,
}: HistoryPaneProps) {
    const graphHeaderRef = useRef<HTMLDivElement | null>(null);
    const graphScrollRef = useRef<HTMLDivElement | null>(null);
    // A drag stays local to this pane: writing every frame into the persisted
    // settings would re-render the whole app and restart the state save.
    const [draftWidths, setDraftWidths] = useState<GraphColumnWidths | null>(null);
    const draftWidthsRef = useRef<GraphColumnWidths | null>(null);
    const activeWidths = draftWidths ?? columnWidths;
    const graphColumnWidth = effectiveGraphColumnWidth(activeWidths, graphLaneExtent);
    const visibleColumns = visibleGraphColumns(columns);
    const graphOffset = graphColumnOffset(columns, activeWidths, graphLaneExtent);

    // With the column squeezed the lanes that no longer fit sit on the limit;
    // the fade at that edge is what says the routes are stacked rather than
    // genuinely converged.
    const lastLaneX = useMemo(
        () => getCommitGraphLastLaneX(history?.commits ?? []),
        [history],
    );
    const overflowEdge = columns.graph
        ? getCommitGraphOverflowEdge(lastLaneX, graphColumnWidth)
        : null;

    const resizeColumn = (key: GraphColumnKey, width: number) => {
        const next = { ...(draftWidthsRef.current ?? columnWidths), [key]: width };
        draftWidthsRef.current = next;
        setDraftWidths(next);
    };

    // Stable identities: the commit rows are memoized on their props, and an
    // inline arrow here would defeat that on every render -- including every
    // frame of a column drag.
    const openCommitMenu = useCallback((request: CommitContextMenuRequest) => {
        setCommitMenu({
            x: request.clientX,
            y: request.clientY,
            commit: request.commit,
            decoration: request.decoration ?? null,
        });
    }, [setCommitMenu]);

    const copyShaFromRow = useCallback((oid: string) => { void copySha(oid); }, [copySha]);

    const checkoutFromRefLabel = useCallback((decoration: RefLabel) => {
        if (decoration.kind === "local_branch" && !decoration.is_head) {
            void runMutation(`Checked out ${decoration.name}`, (repository) => gitcatApi.checkoutBranch(repository.repository_id, decoration.name));
            return;
        }
        if (decoration.kind === "remote_branch") {
            const branch = snapshot?.remote_branches.find((candidate) => candidate.full_name === decoration.full_name);
            if (branch) checkoutRemoteBranch(branch);
        }
    }, [checkoutRemoteBranch, runMutation, snapshot]);

    // A branch only starts existing with its first commit, so an unborn HEAD
    // over an empty history is the one state where there is no graph to draw
    // and an offer to make that commit is more use than an empty panel.
    const emptyRepository = snapshot?.head.kind === "unborn" && history?.commits.length === 0;
    const stagedCount = snapshot?.status.entries.filter((entry) => entry.index).length ?? 0;

    const commitResize = () => {
        const next = draftWidthsRef.current;
        draftWidthsRef.current = null;
        setDraftWidths(null);
        if (next) setColumnWidths(next);
    };

    return (
        <section className="flex min-h-0 min-w-0 flex-col overflow-hidden bg-background" aria-label="Repository history" style={{ gridColumn: 3 }}>
            {searchOpen && centerView === "graph" ? (
                <SearchBar
                    activeIndex={searchIndex}
                    busy={searchBusy}
                    count={searchOids.length}
                    focusToken={searchFocusToken}
                    onChange={setSearchQuery}
                    onClose={() => { setSearchOpen(false); setSearchQuery(""); }}
                    onNext={() => navigateSearch(1)}
                    onPrevious={() => navigateSearch(-1)}
                    value={searchQuery}
                />
            ) : null}
            {centerView === "merge" && conflictEditor ? (
                <MergeEditor
                    branchName={currentBranch(snapshot)}
                    busy={busy}
                    closeKeybind={settings.keybinds.show_graph}
                    details={conflictEditor}
                    onClose={closeConflictEditor}
                    onSave={saveConflictResult}
                    operation={snapshot?.operation_state ?? "normal"}
                />
            ) : centerView === "diff" ? (
                <DiffViewer
                    closeKeybind={settings.keybinds.show_graph}
                    diff={diff}
                    lineActions={diffLineActions}
                    loading={diffLoading}
                    mode={diffMode}
                    onClose={closeDiff}
                    onModeChange={setDiffMode}
                />
            ) : (
                <div
                    className={`gc-no-select flex min-h-0 min-w-0 flex-1 flex-col${draftWidths ? " gc-graph-resizing" : ""}`}
                    style={{
                        "--gc-graph-column-width": `${graphColumnWidth}px`,
                        // Rows read these two instead of taking the width as a
                        // prop, so dragging a column never re-renders them.
                        "--gc-graph-offset": `${graphOffset}px`,
                        "--gc-graph-max-x": `${getCommitGraphMaxX(graphColumnWidth)}px`,
                        "--gc-graph-columns-template": graphColumnsTemplate(columns, activeWidths, graphLaneExtent),
                        "--gc-graph-columns-min-width": `${graphColumnsMinWidth(columns, activeWidths, graphLaneExtent)}px`,
                    } as React.CSSProperties}
                >
                    <div className="gc-graph-header">
                        <div className="gc-graph-header__labels" ref={graphHeaderRef}>
                            <div className="gc-graph-columns">
                                {GRAPH_COLUMNS.filter((column) => columns[column.key]).map((column) => (
                                    <span key={column.key}>
                                        <span aria-hidden="true" className="gc-graph-columns__label">{column.label}</span>
                                        {column.key === visibleColumns[visibleColumns.length - 1] ? null : (
                                            <GraphColumnResizer
                                                label={column.label}
                                                // The graph stops where every lane has its own
                                                // position; wider would only add empty space.
                                                maxWidth={column.key === "graph" ? graphLaneExtent : MAX_GRAPH_COLUMN_WIDTH}
                                                minWidth={column.key === "graph" ? MIN_GRAPH_COLUMN_WIDTH : GRAPH_COLUMN_MIN_WIDTH[column.key]}
                                                onResize={(width) => resizeColumn(column.key, width)}
                                                onResizeEnd={commitResize}
                                                width={resolveColumnWidth(column.key, activeWidths, graphLaneExtent)}
                                            />
                                        )}
                                    </span>
                                ))}
                            </div>
                        </div>
                        <GraphColumnMenu columns={columns} onChange={setColumns} onWidthsChange={setColumnWidths} />
                    </div>
                    {emptyRepository ? (
                        <InitialCommitPrompt
                            busy={busy || overviewLoading || historyLoading}
                            checkingRemote={remoteCheckPending}
                            onInitialize={createInitialCommit}
                            remoteName={snapshot?.remotes[0]?.name ?? null}
                            remoteCount={snapshot?.remotes.length ?? 0}
                            repositoryName={repositoryName}
                            stagedCount={stagedCount}
                        />
                    ) : null}
                    {/* No reserved scrollbar gutter: it left a permanent ten
                        pixel strip the selected row could not reach, so the
                        highlight stopped short of the pane edge even with
                        nothing to scroll. */}
                    <div
                        className="min-h-0 min-w-0 flex-1 overflow-auto"
                        onScroll={(event) => {
                            if (graphHeaderRef.current) graphHeaderRef.current.scrollLeft = event.currentTarget.scrollLeft;
                        }}
                        ref={graphScrollRef}
                    >
                    {/* Spans the scrolled content so the overflow edge can run
                        the full height of the list rather than the viewport. */}
                    <div className="relative">
                    {worktreeReachable ? (
                        <div
                            aria-label="Working copy changes"
                            className={`gc-wip-row ${wipSelected ? "gc-wip-row--selected" : ""}`}
                            onClick={selectWip}
                            onKeyDown={(event) => {
                                if (event.target !== event.currentTarget) return;
                                if (event.key === "ArrowDown") {
                                    event.preventDefault();
                                    selectFirstCommitFromWip();
                                    return;
                                }
                                if (event.key === "Enter" || event.key === " ") {
                                    event.preventDefault();
                                    selectWip();
                                }
                            }}
                            ref={wipRowRef}
                            style={wipRowStyle}
                            tabIndex={0}
                        >
                            {columns.refs ? <span className="gc-wip-row__refs" /> : null}
                            {columns.graph ? <span className="gc-wip-row__rail"><i /></span> : null}
                            {/* A stopped operation takes the row over: there is
                                nothing to summarise until the conflicts are
                                resolved, and this is the row they belong to. */}
                            {activeConflictCount && snapshot ? (
                                <GraphConflictNotice
                                    branchName={currentBranch(snapshot)}
                                    columns={columns}
                                    count={activeConflictCount}
                                    onSelect={focusWorktree}
                                    operation={snapshot.operation_state}
                                    source={snapshot.operation_source ?? null}
                                />
                            ) : (
                                <>
                                    {columns.message ? (
                                        <span className="gc-wip-row__message">
                                            <span
                                                className="gc-wip-row__summary"
                                                data-value={wipDraftMessage || (wipTitleHint ?? "// WIP")}
                                            >
                                                <input
                                                    aria-label="Commit summary"
                                                    onChange={(event) => setWipDraftMessage(event.target.value)}
                                                    onClick={(event) => event.stopPropagation()}
                                                    onFocus={() => { if (!wipSelected) selectWip(); }}
                                                    onKeyDown={(event) => event.stopPropagation()}
                                                    placeholder={wipTitleHint ?? "// WIP"}
                                                    size={1}
                                                    type="text"
                                                    value={wipDraftMessage}
                                                />
                                            </span>
                                            <ChangeCountSummary counts={wipStats} size="md" />
                                        </span>
                                    ) : null}
                                    {columns.author ? <span /> : null}
                                    {columns.date ? <span /> : null}
                                    {columns.sha ? <span /> : null}
                                </>
                            )}
                        </div>
                    ) : null}
                    {history ? (
                        <CommitGraph
                            beforeFirstSelected={wipSelected}
                            columnWidths={activeWidths}
                            columns={columns}
                            commits={history.commits}
                            detachedHeadOid={snapshot?.head.kind === "detached" ? snapshot.head.oid : null}
                            hideHeadDecoration={false}
                            onNavigateBeforeFirst={worktreeReachable ? selectWipFromGraph : undefined}
                            onCommitContextMenu={openCommitMenu}
                            onCopySha={copyShaFromRow}
                            onRefDoubleClick={checkoutFromRefLabel}
                            onSelect={selectCommit}
                            remoteIconUrls={remoteIconUrls}
                            avatarImages={avatarImages}
                            scrollContainerRef={graphScrollRef}
                            searchMatchOids={graphMatches}
                            selectedOid={selectedOid}
                            wip={snapshot && !snapshot.status.clean
                                ? {
                                    lane: wipLane,
                                    headOid: currentHeadOid,
                                    // Only a merge gives the working copy a
                                    // second parent; a rebase or a cherry-pick
                                    // replays onto the one it already has.
                                    incomingOid: snapshot.operation_state === "merge"
                                        ? snapshot.operation_source?.incoming_oid ?? null
                                        : null,
                                }
                                : undefined}
                        />
                    ) : (
                        <div className="flex h-40 items-center justify-center gap-2 text-muted">
                            <Spinner label="Loading history" /> Loading history…
                        </div>
                    )}
                    {history?.has_more && history.next_cursor ? (
                        <Button
                            className="mx-auto mb-5 mt-3 flex"
                            disabled={busy || overviewLoading || historyLoading}
                            onClick={loadMoreHistory}
                        >{historyLoading ? "Loading older commits…" : "Load older commits"}</Button>
                    ) : null}
                    {overflowEdge ? (
                        <span
                            aria-hidden="true"
                            className="gc-graph-overflow-edge"
                            style={{
                                left: graphOffset + overflowEdge.left,
                                opacity: overflowEdge.opacity,
                                width: overflowEdge.width,
                            }}
                        />
                    ) : null}
                    </div>
                    </div>
                </div>
            )}
        </section>
    );
}
