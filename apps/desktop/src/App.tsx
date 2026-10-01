import { AlertTriangle, LoaderCircle, Tag, } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, } from "react";

import { getCommitGraphWidth, getCommitLaneXCss, getCommitRowBranchOrigin, getWipLane, getWipLaneColorVariable, } from "./components/CommitGraph";
import { emptyChangeCounts, fileChangeCounts, sumChangeCounts } from "./components/file-tree";
import type { FolderCollapseTarget } from "./components/file-tree";
import { OperationBanner } from "./components/OperationBanner";
import { AddRemoteDialog, type AddRemoteRequest } from "./components/remotes";
import { REF_RAIL_WIDTH, RefPanel, type BranchContextMenuRequest } from "./components/ref-sidebar";
import { StartPage } from "./components/start-page";
import { GitHubAccessDialog } from "./components/forge/GitHubAccessDialog";
import { AppShell, Resizer } from "./components/shell";
import { ConfirmBar, Toolbar } from "./components/toolbar";
import {
    TopTabs,
    type RepositoryTabContextMenuRequest,
    type TabGroupView,
    type TabView,
} from "./components/top-tabs";
import { UnavailableRepositoryView } from "./components/UnavailableRepositoryView";
import { WelcomeView } from "./components/WelcomeView";
import { buildCommitMessage, type CommitDraft } from "./components/worktree";
import type { DiffViewMode } from "./components/diff";
import { gitcatApi } from "./lib/api";
import { createForgeRepository } from "./lib/forgeAuth";
import { useAppUpdate } from "./lib/updates";
import type {
    CommitActionAvailability,
    ConflictFileDetails,
    DiffRequest,
    FileDiff,
    FileViewMode,
    GraphColumnSettings,
    GraphColumnWidths,
    HistoryPage,
    PersistedState,
    PullRequestInfo,
    RepositorySnapshot,
    RepositoryTab,
    StashEntry,
} from "./lib/types";
import { currentBranch, remoteIconUrls } from "./app/branches";
import { withForgeOverrides } from "./lib/forge";
import { openExternal } from "./lib/platform";
import { useAvatars } from "./app/useAvatars";
import { useForgeStatus } from "./app/useForgeStatus";
import { connectForge } from "./app/forgeConnections";
import { EMPTY_COMMIT_DRAFT, EMPTY_STATE } from "./app/defaults";
import { continuableOperation } from "./app/snapshot";
import type { BranchMenuState, CenterView, CommitMenuState, ConfirmState, PromptState, RuntimeRepository, TabMenuState } from "./app/state";
import { AppDialogs } from "./app/AppDialogs";
import { AppStatusBar } from "./app/AppStatusBar";
import { HistoryPane } from "./app/HistoryPane";
import { InspectorPane } from "./app/InspectorPane";
import { useAppChrome } from "./app/useAppChrome";
import { useAutoRefresh } from "./app/useAutoRefresh";
import { useCommitSearch } from "./app/useCommitSearch";
import { useCommitSelection } from "./app/useCommitSelection";
import { useConflictActions } from "./app/useConflictActions";
import { useConflictPreflight } from "./app/useConflictPreflight";
import { useContextMenuActions } from "./app/useContextMenuActions";
import { useDialogActions } from "./app/useDialogActions";
import { useDiffPane } from "./app/useDiffPane";
import { useGlobalKeybinds } from "./app/useGlobalKeybinds";
import { useLaunchRepository } from "./app/useLaunchRepository";
import { usePanelLayout } from "./app/usePanelLayout";
import { useRepositoryCommands } from "./app/useRepositoryCommands";
import { useRepositoryOverview } from "./app/useRepositoryOverview";
import { useRepositoryTabs } from "./app/useRepositoryTabs";
import { useToasts } from "./app/useToasts";
import { useWorkspaceBootstrap } from "./app/useWorkspaceBootstrap";
import { useWorktreeMutations } from "./app/useWorktreeMutations";
import { workspaceTabs } from "./app/workspace";
import { wipChangeKind } from "./app/worktree";

function App() {
    const [persisted, setPersisted] = useState<PersistedState>(EMPTY_STATE);
    const [hydrated, setHydrated] = useState(false);
    const [runtime, setRuntime] = useState<Record<string, RuntimeRepository>>({});
    const [tabErrors, setTabErrors] = useState<Record<string, string>>({});
    const [openingTabIds, setOpeningTabIds] = useState<string[]>([]);
    const [rawSnapshot, setSnapshot] = useState<RepositorySnapshot | null>(null);
    // Settings name the forge for hosts the backend cannot recognise, so
    // every consumer below reads an already-corrected snapshot.
    const forgeOverrides = persisted.settings.forge_overrides;
    const snapshot = useMemo(
        () => withForgeOverrides(rawSnapshot, forgeOverrides),
        [forgeOverrides, rawSnapshot],
    );
    const [history, setHistory] = useState<HistoryPage | null>(null);
    const [stashes, setStashes] = useState<StashEntry[]>([]);
    const [selectedOid, setSelectedOid] = useState<string | null>(null);
    const selectedOidRef = useRef<string | null>(null);
    const pendingSelectionRef = useRef<{ index: number; subject: string } | null>(null);
    const [wipSelected, setWipSelected] = useState(false);
    const [wipTitleHint, setWipTitleHint] = useState<string | null>(null);
    const wipRowRef = useRef<HTMLDivElement>(null);
    const [details, setDetails] = useState<Awaited<ReturnType<typeof gitcatApi.commitDetails>> | null>(null);
    const [commitActions, setCommitActions] = useState<CommitActionAvailability[]>([]);
    const [diff, setDiff] = useState<FileDiff | null>(null);
    const [diffLoading, setDiffLoading] = useState(false);
    const [selectedPath, setSelectedPath] = useState<string | undefined>();
    const [selectedWorktreeFile, setSelectedWorktreeFile] = useState<{ path: string; staged: boolean } | null>(null);
    const [centerView, setCenterView] = useState<CenterView>("graph");
    const [stageCollapse, setStageCollapse] = useState<{ target: FolderCollapseTarget; staged: boolean; token: number } | null>(null);
    const [busy, setBusy] = useState(false);
    const [overviewLoading, setOverviewLoading] = useState(false);
    const [historyLoading, setHistoryLoading] = useState(false);
    const [initializing, setInitializing] = useState(true);
    const [settingsOpen, setSettingsOpen] = useState(false);
    const [gitHubAccessOpen, setGitHubAccessOpen] = useState(false);
    const [prompt, setPrompt] = useState<PromptState>(null);
    const [startDialog, setStartDialog] = useState<"clone" | "create" | null>(null);
    const [confirmRequest, setConfirmRequest] = useState<ConfirmState>(null);
    const [addRemoteOpen, setAddRemoteOpen] = useState(false);
    const [commitMenu, setCommitMenu] = useState<CommitMenuState | null>(null);
    // Opening the message editor from the graph has to wait for the commit's
    // details to load, so the request travels as an (oid, token) pair.
    const [rewordRequest, setRewordRequest] = useState<{ oid: string; token: number } | null>(null);
    const [tabMenu, setTabMenu] = useState<TabMenuState | null>(null);
    const [branchMenu, setBranchMenu] = useState<BranchMenuState | null>(null);
    const {
        beginResize,
        detailsWidth,
        leftPanelVisible,
        rightPanelVisible,
        setLeftPanelVisible,
        setRightPanelVisible,
        sidebarWidth,
    } = usePanelLayout();
    const appUpdate = useAppUpdate();
    const [conflictEditor, setConflictEditor] = useState<ConflictFileDetails | null>(null);
    const [commitDrafts, setCommitDrafts] = useState<Record<string, CommitDraft>>({});
    const [overviewRepositoryId, setOverviewRepositoryId] = useState<string | null>(null);
    const activeRepositoryIdRef = useRef<string | null>(null);
    const autoRefreshRef = useRef<() => void>(() => { });
    const autoReloadDiffRef = useRef<() => void>(() => { });
    const openWorktreeDiffRef = useRef<(path: string, staged: boolean) => void>(() => { });
    const swapWorktreeDiffSideRef = useRef<(path: string, staged: boolean) => void>(() => { });
    const openDiffRequestRef = useRef<{ sequence: number; request: DiffRequest } | null>(null);
    const diffWholeFileRef = useRef(false);
    const autoFetchRef = useRef<() => void>(() => { });
    const lastAutoFetchRef = useRef<Map<string, number>>(new Map());
    const closedTabsRef = useRef<RepositoryTab[]>([]);
    const openingTabsRef = useRef<Set<string>>(new Set());
    const workspaceRef = useRef(persisted.workspace);
    const overviewLoadSequence = useRef(0);
    const detailsLoadSequence = useRef(0);
    const diffLoadSequence = useRef(0);
    const historyLoadSequence = useRef(0);

    const changeFileViewMode = useCallback((mode: FileViewMode) => {
        setPersisted((current) => (
            current.settings.file_view_mode === mode
                ? current
                : { ...current, settings: { ...current.settings, file_view_mode: mode } }
        ));
    }, []);

    const graphColumns = persisted.settings.graph_columns;
    const setGraphColumns = useCallback((columns: GraphColumnSettings) => {
        setPersisted((current) => ({ ...current, settings: { ...current.settings, graph_columns: columns } }));
    }, []);

    const graphColumnWidths = persisted.settings.graph_column_widths;
    const setGraphColumnWidths = useCallback((widths: GraphColumnWidths) => {
        setPersisted((current) => ({ ...current, settings: { ...current.settings, graph_column_widths: widths } }));
    }, []);

    const diffMode = persisted.settings.diff_view_mode;
    const setDiffMode = useCallback((mode: DiffViewMode) => {
        setPersisted((current) => (
            current.settings.diff_view_mode === mode
                ? current
                : { ...current, settings: { ...current.settings, diff_view_mode: mode } }
        ));
    }, []);

    const activeTabId = persisted.workspace.active_tab_id;
    const activeRepository = activeTabId ? runtime[activeTabId] : undefined;
    const activeTab = activeTabId
        ? workspaceTabs(persisted.workspace).find((tab) => tab.id === activeTabId)
        : undefined;
    const activeTabKind = activeTab?.kind;
    const activeTabPath = activeTab?.repository_path;

    const appMetadata = useAppChrome({
        activeRepository,
        activeRepositoryIdRef,
        activeTabId,
        setBranchMenu,
        setCommitMenu,
        setConfirmRequest,
        setConflictEditor,
        setPrompt,
        setStartDialog,
        setTabMenu,
    });

    const { addToast, dismissToast, registerRecoveryHandlers, showError, toasts } = useToasts();

    const { openTabRepository } = useWorkspaceBootstrap({
        activeRepository,
        activeTabId,
        activeTabKind,
        activeTabPath,
        hydrated,
        initializing,
        openingTabsRef,
        persisted,
        setHydrated,
        setInitializing,
        setOpeningTabIds,
        setPersisted,
        setRuntime,
        setTabErrors,
        showError,
    });

    useEffect(() => { selectedOidRef.current = selectedOid; }, [selectedOid]);
    useEffect(() => { workspaceRef.current = persisted.workspace; }, [persisted.workspace]);

    const { loadMoreHistory, loadOverview } = useRepositoryOverview({
        activeRepository,
        activeRepositoryIdRef,
        busy,
        detailsLoadSequence,
        diffLoadSequence,
        history,
        historyLoadSequence,
        historyLoading,
        historyPageSize: persisted.settings.history_page_size,
        overviewLoadSequence,
        overviewLoading,
        overviewRepositoryId,
        pendingSelectionRef,
        selectedOid,
        selectedOidRef,
        setCenterView,
        setCommitActions,
        setConflictEditor,
        setDetails,
        setDiff,
        setDiffLoading,
        setHistory,
        setHistoryLoading,
        setOverviewLoading,
        setOverviewRepositoryId,
        setSelectedOid,
        setSelectedPath,
        setSelectedWorktreeFile,
        setSnapshot,
        setStashes,
        setWipSelected,
        setWipTitleHint,
        showError,
        wipSelected,
    });

    const {
        closeDiff,
        loadDiff,
        openCommitFile,
        openWorktreeDiff,
        openWorktreeFileDiff,
        reloadOpenWorktreeDiff,
        swapWorktreeDiffSide,
    } = useDiffPane({
        activeRepository,
        activeRepositoryIdRef,
        autoReloadDiffRef,
        diffContextLines: persisted.settings.diff_context_lines,
        diffLoadSequence,
        diffLoading,
        diffMaxBytes: persisted.settings.diff_max_bytes,
        diffMode,
        diffWholeFileRef,
        openDiffRequestRef,
        openWorktreeDiffRef,
        selectedOid,
        selectedWorktreeFile,
        setCenterView,
        setDiff,
        setDiffLoading,
        setSelectedPath,
        setSelectedWorktreeFile,
        showError,
        snapshot,
        swapWorktreeDiffSideRef,
    });

    const {
        navigateSearch,
        openSearch,
        searchBusy,
        searchFocusToken,
        searchIndex,
        searchOids,
        searchOpen,
        searchQuery,
        setSearchOpen,
        setSearchQuery,
    } = useCommitSearch({
        activeRepository,
        activeRepositoryIdRef,
        diffLoadSequence,
        setCenterView,
        setCommitActions,
        setDetails,
        setDiff,
        setDiffLoading,
        setSelectedOid,
        setSelectedPath,
        setSelectedWorktreeFile,
        setWipSelected,
        showError,
    });

    const { createPatchFile, rewordCommit, runMutation, stagePaths, unstagePaths } = useWorktreeMutations({
        activeRepository,
        activeRepositoryIdRef,
        addToast,
        busy,
        centerView,
        fileViewMode: persisted.settings.file_view_mode,
        history,
        loadOverview,
        openWorktreeDiffRef,
        overviewLoadSequence,
        overviewLoading,
        pendingSelectionRef,
        selectedWorktreeFile,
        setBusy,
        setSelectedWorktreeFile,
        setSnapshot,
        setStageCollapse,
        setWipTitleHint,
        showError,
        snapshot,
        swapWorktreeDiffSideRef,
    });

    const {
        chooseRepository,
        cloneRepository,
        closeTab,
        createRepository,
        forgetRecentRepository,
        moveRepositoryTab,
        openRepositoryPath,
        openStartTab,
        reopenClosedRepository,
    } = useRepositoryTabs({
        addToast,
        busy,
        closedTabsRef,
        lastDirectory: persisted.last_directory,
        runtime,
        setBusy,
        setCommitDrafts,
        setPersisted,
        setRuntime,
        setTabErrors,
        showError,
        workspace: persisted.workspace,
        workspaceRef,
    });

    useLaunchRepository({ initializing, openRepositoryPath });

    const {
        focusWorktree,
        jumpToCommit,
        selectCommit,
        selectCommitOid,
        selectFirstCommitFromWip,
        selectWip,
        selectWipFromGraph,
    } = useCommitSelection({
        diffLoadSequence,
        history,
        selectedOidRef,
        setCenterView,
        setCommitActions,
        setDetails,
        setDiff,
        setDiffLoading,
        setRightPanelVisible,
        setSelectedOid,
        setSelectedPath,
        setSelectedWorktreeFile,
        setWipSelected,
        wipRowRef,
        wipSelected,
    });

    const startCommitReword = useCallback((oid: string) => {
        jumpToCommit(oid);
        setRewordRequest((current) => ({ oid, token: (current?.token ?? 0) + 1 }));
    }, [jumpToCommit]);

    const {
        closeConflictEditor,
        openConflictEditor,
        resolveConflictEntry,
        resolveConflictPaths,
        saveConflictResult,
    } = useConflictActions({
        activeRepository,
        activeRepositoryIdRef,
        busy,
        conflictEditor,
        runMutation,
        setBusy,
        setCenterView,
        setConflictEditor,
        setSelectedWorktreeFile,
        showError,
        snapshot,
    });

    const requestRemote = useCallback(() => setConfirmRequest({ kind: "add_remote_for_push" }), []);

    // The remote is added first and the branch pushed after it, as two
    // commands: a push that fails leaves a remote that was added, and saying
    // only that the push failed would hide that.
    const addRemoteAndPush = useCallback((request: AddRemoteRequest) => {
        setAddRemoteOpen(false);
        const branch = snapshot?.head.kind === "branch" ? snapshot.head.name : null;
        void runMutation(`Remote ${request.remote} added`, async (repository) => {
            const url = request.kind === "url"
                ? request.url
                : (await createForgeRepository(request.repository)).clone_url;
            return gitcatApi.addRemote(repository.repository_id, request.remote, url);
        }, { queueKey: "add-remote", silent: Boolean(branch) }).then((added) => {
            if (!added || !branch) return;
            void runMutation(`Pushed ${branch} to ${request.remote}`, (repository) => gitcatApi.push(repository.repository_id, {
                remote: request.remote,
                branch,
                set_upstream: true,
            }), { queueKey: `push:${request.remote}:${branch}` });
        });
    }, [runMutation, snapshot]);

    const copySha = useCallback(async (oid: string) => {
        try {
            await navigator.clipboard.writeText(oid);
            addToast({ tone: "success", title: "Commit SHA copied" });
        } catch (error) {
            showError("Could not copy SHA", error);
        }
    }, [addToast, showError]);


    const {
        abortActiveOperation,
        autoResolveActiveConflicts,
        continueActiveOperation,
        createBranchAtHead,
        fetchActiveRepository,
        focusCommitMessage,
        popLatestStash,
        pullActiveRepository,
        pushActiveRepository,
        skipActiveOperation,
        stashActiveRepository,
    } = useRepositoryCommands({
        addToast,
        autoPrune: persisted.settings.auto_prune,
        defaultPullMode: persisted.settings.default_pull_mode,
        requestRemote,
        runMutation,
        selectWip,
        setRightPanelVisible,
        setPrompt,
        snapshot,
        stashes,
    });

    const { refreshActiveRepository } = useAutoRefresh({
        activeRepository,
        activeRepositoryIdRef,
        autoFetchIntervalMinutes: persisted.settings.auto_fetch_interval_minutes,
        autoFetchRef,
        autoPrune: persisted.settings.auto_prune,
        autoRefreshRef,
        autoReloadDiffRef,
        busy,
        lastAutoFetchRef,
        loadOverview,
        overviewLoading,
        overviewRepositoryId,
        reloadOpenWorktreeDiff,
        showError,
        snapshot,
    });

    const orderedTabIds = useMemo(
        () => workspaceTabs(persisted.workspace).map((tab) => tab.id),
        [persisted.workspace],
    );

    const activateRepositoryTab = useCallback((nextId: string | undefined) => {
        if (!nextId) return;
        setPersisted((current) => ({
            ...current,
            workspace: {
                ...current.workspace,
                active_tab_id: nextId,
                groups: current.workspace.groups.map((group) => (
                    group.tabs.some((tab) => tab.id === nextId) ? { ...group, collapsed: false } : group
                )),
            },
        }));
    }, []);

    const cycleRepository = useCallback((direction: 1 | -1) => {
        if (orderedTabIds.length < 2) return;
        const currentIndex = activeTabId ? orderedTabIds.indexOf(activeTabId) : -1;
        const nextIndex = (currentIndex + direction + orderedTabIds.length) % orderedTabIds.length;
        activateRepositoryTab(orderedTabIds[nextIndex]);
    }, [activateRepositoryTab, activeTabId, orderedTabIds]);

    useGlobalKeybinds({
        abortActiveOperation,
        activateRepositoryTab,
        activeRepository,
        activeTabId,
        autoResolveActiveConflicts,
        busy,
        centerView,
        chooseRepository,
        closeConflictEditor,
        closeDiff,
        closeTab,
        commitMenu,
        continueActiveOperation,
        copySha,
        createBranchAtHead,
        cycleRepository,
        diff,
        diffLoading,
        fetchActiveRepository,
        focusCommitMessage,
        keybinds: persisted.settings.keybinds,
        openSearch,
        openStartTab,
        orderedTabIds,
        overviewLoading,
        prompt,
        pullActiveRepository,
        pushActiveRepository,
        refreshActiveRepository,
        reopenClosedRepository,
        runMutation,
        selectWip,
        selectedOid,
        selectedWorktreeFile,
        setBranchMenu,
        setCenterView,
        setCommitMenu,
        setDiffMode,
        setLeftPanelVisible,
        setRightPanelVisible,
        setSettingsOpen,
        setTabMenu,
        settingsOpen: settingsOpen || gitHubAccessOpen,
        showError,
        snapshot,
        stagePaths,
        stashActiveRepository,
        startDialog,
        tabMenu,
        unstagePaths,
        wipSelected,
    });

    const {
        branchContextActions,
        checkoutRemoteBranch,
        contextActions,
        executeBranchAction,
        executeCommitAction,
        executeTabAction,
        tabContextActions,
    } = useContextMenuActions({
        activateRepositoryTab,
        activeRepository,
        activeRepositoryIdRef,
        addToast,
        branchMenu,
        closeTab,
        commitActions,
        commitMenu,
        copySha,
        defaultPullMode: persisted.settings.default_pull_mode,
        detailsOid: details?.oid,
        moveRepositoryTab,
        pullActiveRepository,
        runMutation,
        setBranchMenu,
        setCommitMenu,
        setConfirmRequest,
        setPersisted,
        setPrompt,
        setTabMenu,
        showError,
        snapshot,
        startCommitReword,
        tabMenu,
        workspace: persisted.workspace,
    });

    const { confirmConfig, escalateForcePush, promptConfig, submitConfirm, submitPrompt } = useDialogActions({
        confirmRequest,
        openAddRemote: () => setAddRemoteOpen(true),
        prompt,
        runMutation,
        setConfirmRequest,
        setPersisted,
        setPrompt,
        snapshot,
    });

    // The Git failure classifier names the step that unblocks a failure; this
    // is where those names get something to run. It sits below the dialogs
    // because one of the steps is a confirmation rather than a command.
    useEffect(() => {
        registerRecoveryHandlers({
            sign_in_github: () => {
                setGitHubAccessOpen(true);
                void connectForge("github.com");
            },
            open_settings: () => setSettingsOpen(true),
            pull: () => pullActiveRepository(),
            push_force: () => escalateForcePush(),
            push_set_upstream: () => pushActiveRepository({ setUpstream: true }),
        });
    }, [escalateForcePush, pullActiveRepository, pushActiveRepository, registerRecoveryHandlers]);

    const activeConflictCount =snapshot?.status.entries.filter((entry) => entry.conflicted).length ?? 0;
    const wipStats = useMemo(() => {
        const entries = snapshot?.status.entries ?? [];
        return sumChangeCounts(...entries.map((entry) => {
            const kind = wipChangeKind(entry);
            return kind ? fileChangeCounts(kind) : emptyChangeCounts;
        }));
    }, [snapshot?.status.entries]);
    const {
        conflictIndicator,
        conflictTarget,
        conflictTargets,
        selectConflictTarget,
        showConflictIndicator,
    } = useConflictPreflight({
        activeConflictCount,
        activeRepository,
        activeRepositoryIdRef,
        activeTab,
        activeTabId,
        addToast,
        setCenterView,
        setPersisted,
        setRightPanelVisible,
        setSelectedOid,
        setWipSelected,
        snapshot,
    });


    const operationPending = snapshot ? continuableOperation(snapshot.operation_state) !== null : false;
    const worktreeReachable = snapshot ? !snapshot.status.clean || operationPending : false;

    const toTabView = useCallback((tab: RepositoryTab): TabView => ({
        id: tab.id,
        label: tab.display_name,
        path: tab.repository_path,
        kind: tab.kind ?? "repository",
        dirty: tab.id === activeTabId && snapshot ? !snapshot.status.clean : false,
        conflictCount: tab.id === activeTabId ? activeConflictCount : 0,
        unavailable: tab.kind !== "start" && !runtime[tab.id],
    }), [activeConflictCount, activeTabId, runtime, snapshot]);
    const ungroupedTabs = useMemo(
        () => persisted.workspace.ungrouped_tabs.map(toTabView),
        [persisted.workspace.ungrouped_tabs, toTabView],
    );
    const tabGroups = useMemo<TabGroupView[]>(() => persisted.workspace.groups.map((group) => ({
        id: group.id,
        name: group.name,
        collapsed: group.collapsed,
        tabs: group.tabs.map(toTabView),
    })), [persisted.workspace.groups, toTabView]);

    const graphMatches = useMemo(() => new Set(searchOids), [searchOids]);
    const iconUrlsByRemote = useMemo(
        () => remoteIconUrls(snapshot?.remotes ?? []),
        [snapshot?.remotes],
    );
    const avatarImages = useAvatars(snapshot, history?.commits ?? [], persisted.settings.avatars);
    const forgeStatus = useForgeStatus(snapshot, persisted.settings.forge);
    // The badge opens the pull request. Where there is no browser to open it in
    // -- the browser build -- the address goes to the clipboard instead, so the
    // badge is never a dead control.
    const openPullRequest = useCallback((pull: PullRequestInfo) => {
        void openExternal(pull.url)
            .then((opened) => {
                if (opened) return;
                return navigator.clipboard.writeText(pull.url)
                    .then(() => addToast({ tone: "success", title: `Link to #${pull.number} copied` }));
            })
            .catch((error) => showError("Could not open the pull request", error));
    }, [addToast, showError]);
    const currentHeadOid = snapshot?.head.kind === "unborn" ? null : snapshot?.head.oid ?? null;
    const wipLane = useMemo(
        () => getWipLane(history?.commits ?? [], currentHeadOid),
        [currentHeadOid, history],
    );
    // Widest the graph column can usefully get: every lane spread out.
    const graphLaneExtent = useMemo(
        () => getCommitGraphWidth(history?.commits ?? []),
        [history],
    );
    const wipLaneColor = getWipLaneColorVariable();
    // The lane position parks against the dragged graph width in CSS, so the
    // WIP row follows a column resize without re-rendering the history.
    const wipLaneX = getCommitLaneXCss(wipLane);
    const wipRowStyle = {
        "--gc-branch-origin": wipLaneX,
        "--gc-branch-interactive-origin": `calc(${wipLaneX} + 11px)`,
        "--gc-branch-row-origin": getCommitRowBranchOrigin(wipLane, graphColumns),
        "--gc-row-branch-color": wipLaneColor,
        "--gc-wip-lane-x": wipLaneX,
    } as React.CSSProperties;
    const activeCommitDraft = activeTabId
        ? commitDrafts[activeTabId] ?? EMPTY_COMMIT_DRAFT
        : EMPTY_COMMIT_DRAFT;
    const updateActiveCommitDraft = useCallback((draft: CommitDraft) => {
        if (!activeTabId) return;
        setCommitDrafts((current) => ({ ...current, [activeTabId]: draft }));
    }, [activeTabId]);
    // The first commit of an empty repository is the one GitCat composes
    // itself. A summary the user already typed on the working-copy row wins
    // over the default, and clears afterwards exactly as the commit form does.
    const createInitialCommit = useCallback(() => {
        const draft = activeCommitDraft;
        const message = draft.message.trim() ? buildCommitMessage(draft) : "Initial commit";
        void runMutation(
            "Initial commit created",
            (repository) => gitcatApi.createInitialCommit(repository.repository_id, message),
        ).then((created) => {
            if (created) updateActiveCommitDraft({ ...draft, message: "", description: "", amend: false });
        });
    }, [activeCommitDraft, runMutation, updateActiveCommitDraft]);
    if (initializing) {
        return (
            <AppShell className="items-center justify-center gap-3 text-muted [&>svg]:animate-orbit">
                <LoaderCircle size={24} />
                <span>Opening GitCat…</span>
            </AppShell>
        );
    }

    return (
        <AppShell>
            <TopTabs
                activeTabId={activeTabId ?? undefined}
                actionsDisabled={busy}
                groups={tabGroups}
                ungroupedTabs={ungroupedTabs}
                onClose={closeTab}
                onCreateGroup={() => setPrompt({ kind: "create_group" })}
                onMoveTab={moveRepositoryTab}
                onOpen={openStartTab}
                onRenameGroup={(groupId) => {
                    const group = persisted.workspace.groups.find((item) => item.id === groupId);
                    if (group) setPrompt({ kind: "rename_group", groupId, current: group.name });
                }}
                onSelect={activateRepositoryTab}
                onTabContextMenu={(request: RepositoryTabContextMenuRequest) => {
                    setCommitMenu(null);
                    setTabMenu({
                        x: request.clientX,
                        y: request.clientY,
                        tab: request.tab,
                        groupId: request.groupId,
                    });
                }}
                onToggleGroup={(groupId) => setPersisted((current) => ({ ...current, workspace: { ...current.workspace, groups: current.workspace.groups.map((group) => group.id === groupId ? { ...group, collapsed: !group.collapsed } : group) } }))}
            />

            {!activeRepository ? (
                activeTab?.kind === "start" ? (
                    <StartPage
                        busy={busy}
                        onClone={() => setStartDialog("clone")}
                        onCreate={() => setStartDialog("create")}
                        onForgetRecent={forgetRecentRepository}
                        onOpen={() => void chooseRepository(activeTab.id)}
                        onSelectRecent={(recent) => void openRepositoryPath(recent.path, activeTab.id)}
                        recents={persisted.recents}
                    />
                ) : activeTab ? (
                    <UnavailableRepositoryView
                        busy={busy}
                        detail={tabErrors[activeTab.id]}
                        loading={openingTabIds.includes(activeTab.id)}
                        name={activeTab.display_name}
                        onClose={() => closeTab(activeTab.id)}
                        onLocate={() => void chooseRepository(activeTab.id)}
                        onRetry={() => void openTabRepository(activeTab.id, activeTab.repository_path)}
                        path={activeTab.repository_path}
                    />
                ) : (
                    <WelcomeView
                        onStart={openStartTab}
                        startKeybind={persisted.settings.keybinds.new_repository_tab}
                    />
                )
            ) : (
                <>
                    {confirmRequest && confirmConfig ? (
                        <ConfirmBar
                            confirmLabel={confirmConfig.confirmLabel}
                            danger={confirmConfig.danger ?? true}
                            message={confirmConfig.message}
                            onCancel={() => setConfirmRequest(null)}
                            onConfirm={submitConfirm}
                        />
                    ) : (
                        <Toolbar
                            branchName={currentBranch(snapshot)}
                            busy={busy}
                            canPop={stashes.length > 0}
                            canStash={(snapshot?.status.entries.length ?? 0) > 0}
                            conflictIndicator={conflictIndicator}
                            conflictTarget={conflictTarget}
                            conflictTargets={conflictTargets}
                            onCreateBranch={createBranchAtHead}
                            onConflictIndicator={showConflictIndicator}
                            onConflictTargetChange={selectConflictTarget}
                            onPull={pullActiveRepository}
                            onPullModeChange={(mode) => setPersisted((current) => ({ ...current, settings: { ...current.settings, default_pull_mode: mode } }))}
                            onPush={pushActiveRepository}
                            onSearch={openSearch}
                            onSettings={() => setSettingsOpen(true)}
                            onStash={stashActiveRepository}
                            onStashPop={popLatestStash}
                            onToggleRightPanel={() => setRightPanelVisible((visible) => !visible)}
                            operation={snapshot?.operation_state ?? "normal"}
                            pullMode={persisted.settings.default_pull_mode}
                            refreshing={overviewLoading}
                            repositoryName={activeRepository.info.name}
                            rightPanelKeybind={persisted.settings.keybinds.toggle_right_panel}
                            rightPanelVisible={rightPanelVisible}
                            searchKeybind={persisted.settings.keybinds.search_commits}
                            settingsKeybind={persisted.settings.keybinds.open_settings}
                        />
                    )}

                    {/* Conflicts are stated on the working-copy row instead,
                        so the banner is left to the operations that are only
                        waiting for a decision. */}
                    {activeConflictCount ? null : (
                        <OperationBanner
                            busy={busy}
                            onAbort={abortActiveOperation}
                            onContinue={continueActiveOperation}
                            onSkip={skipActiveOperation}
                            operation={snapshot?.operation_state ?? "normal"}
                            progress={snapshot?.operation_progress ?? null}
                        />
                    )}

                    <main
                        className="grid min-h-0 flex-auto overflow-hidden bg-background"
                        style={{
                            gridTemplateColumns: `${leftPanelVisible ? sidebarWidth : REF_RAIL_WIDTH}px ${leftPanelVisible ? 5 : 0}px minmax(0, 1fr) ${rightPanelVisible ? 5 : 0}px ${rightPanelVisible ? detailsWidth : 0}px`,
                        }}
                    >
                        <div className="min-h-0 min-w-0 overflow-hidden" style={{ gridColumn: 1 }}>
                            <RefPanel
                                collapsed={!leftPanelVisible}
                                localBranches={snapshot?.local_branches ?? []}
                                onCollapse={() => setLeftPanelVisible(false)}
                                onExpand={() => setLeftPanelVisible(true)}
                                onBranchContextMenu={(request: BranchContextMenuRequest) => {
                                    setCommitMenu(null);
                                    setTabMenu(null);
                                    setBranchMenu({
                                        x: request.clientX,
                                        y: request.clientY,
                                        branch: request.branch,
                                        scope: request.scope,
                                    });
                                }}
                                onCheckout={(branch) => {
                                    if (!branch.is_head) void runMutation(`Checked out ${branch.name}`, (repository) => gitcatApi.checkoutBranch(repository.repository_id, branch.name));
                                }}
                                onCheckoutRemote={checkoutRemoteBranch}
                                onCreateBranch={() => currentHeadOid ? setPrompt({ kind: "create_branch", startOid: currentHeadOid }) : undefined}
                                onOpenPullRequest={openPullRequest}
                                pullRequests={forgeStatus.pullRequests}
                                remoteBranches={snapshot?.remote_branches ?? []}
                                remoteIconUrls={iconUrlsByRemote}
                                tags={snapshot?.tags ?? []}
                                toggleKeybind={persisted.settings.keybinds.toggle_left_panel}
                            />
                        </div>
                        <Resizer hidden={!leftPanelVisible} onPointerDown={(event) => beginResize("left", event)} style={{ gridColumn: 2 }} />

                        <HistoryPane
                            activeConflictCount={activeConflictCount}
                            busy={busy}
                            centerView={centerView}
                            checkoutRemoteBranch={checkoutRemoteBranch}
                            closeConflictEditor={closeConflictEditor}
                            closeDiff={closeDiff}
                            conflictEditor={conflictEditor}
                            columnWidths={graphColumnWidths}
                            columns={graphColumns}
                            copySha={copySha}
                            createInitialCommit={createInitialCommit}
                            currentHeadOid={currentHeadOid}
                            diff={diff}
                            diffLoading={diffLoading}
                            diffMode={diffMode}
                            focusWorktree={focusWorktree}
                            graphLaneExtent={graphLaneExtent}
                            graphMatches={graphMatches}
                            history={history}
                            historyLoading={historyLoading}
                            loadMoreHistory={loadMoreHistory}
                            navigateSearch={navigateSearch}
                            overviewLoading={overviewLoading}
                            remoteIconUrls={iconUrlsByRemote}
                            avatarImages={avatarImages}
                            repositoryName={activeTab?.display_name ?? ""}
                            runMutation={runMutation}
                            saveConflictResult={saveConflictResult}
                            searchBusy={searchBusy}
                            searchFocusToken={searchFocusToken}
                            searchIndex={searchIndex}
                            searchOids={searchOids}
                            searchOpen={searchOpen}
                            searchQuery={searchQuery}
                            selectCommit={selectCommit}
                            selectFirstCommitFromWip={selectFirstCommitFromWip}
                            selectWip={selectWip}
                            selectWipFromGraph={selectWipFromGraph}
                            selectedOid={selectedOid}
                            setColumnWidths={setGraphColumnWidths}
                            setColumns={setGraphColumns}
                            setCommitMenu={setCommitMenu}
                            setDiffMode={setDiffMode}
                            setSearchOpen={setSearchOpen}
                            setSearchQuery={setSearchQuery}
                            setWipDraftMessage={(message) => updateActiveCommitDraft({ ...activeCommitDraft, message })}
                            settings={persisted.settings}
                            snapshot={snapshot}
                            wipDraftMessage={activeCommitDraft.message}
                            wipLane={wipLane}
                            wipRowRef={wipRowRef}
                            wipRowStyle={wipRowStyle}
                            wipSelected={wipSelected}
                            wipStats={wipStats}
                            wipTitleHint={wipTitleHint}
                            worktreeReachable={worktreeReachable}
                        />

                        <Resizer hidden={!rightPanelVisible} onPointerDown={(event) => beginResize("right", event)} style={{ gridColumn: 4 }} />
                        <div className="min-h-0 min-w-0 overflow-hidden" hidden={!rightPanelVisible} style={{ gridColumn: 5 }}>
                            <InspectorPane
                                abortActiveOperation={abortActiveOperation}
                                activeCommitDraft={activeCommitDraft}
                                activeRepository={activeRepository}
                                autoResolveActiveConflicts={autoResolveActiveConflicts}
                                avatarImages={avatarImages}
                                busy={busy}
                                changeFileViewMode={changeFileViewMode}
                                continueActiveOperation={continueActiveOperation}
                                copySha={copySha}
                                createPatchFile={createPatchFile}
                                details={details}
                                jumpToCommit={jumpToCommit}
                                openCommitFile={openCommitFile}
                                openConflictEditor={openConflictEditor}
                                openWorktreeDiff={openWorktreeDiff}
                                overviewLoading={overviewLoading}
                                resolveConflictEntry={resolveConflictEntry}
                                resolveConflictPaths={resolveConflictPaths}
                                rewordCommit={rewordCommit}
                                rewordRequest={rewordRequest}
                                runMutation={runMutation}
                                selectedOid={selectedOid}
                                selectedPath={selectedPath}
                                selectedWorktreeFile={selectedWorktreeFile}
                                settings={persisted.settings}
                                showError={showError}
                                skipActiveOperation={skipActiveOperation}
                                snapshot={snapshot}
                                stageCollapse={stageCollapse}
                                stagePaths={stagePaths}
                                unstagePaths={unstagePaths}
                                updateActiveCommitDraft={updateActiveCommitDraft}
                                wipSelected={wipSelected}
                            />
                        </div>
                    </main>

                    {addRemoteOpen ? (
                        <AddRemoteDialog
                            busy={busy}
                            onClose={() => setAddRemoteOpen(false)}
                            onSubmit={addRemoteAndPush}
                            overrides={forgeOverrides}
                            pushes={snapshot?.head.kind === "branch"}
                            repositoryName={activeRepository.info.name}
                        />
                    ) : null}
                </>
            )}

            <AppStatusBar appMetadata={appMetadata} appUpdate={appUpdate} />

            <AppDialogs
                activeTab={activeTab}
                branchContextActions={branchContextActions}
                branchMenu={branchMenu}
                busy={busy}
                cloneRepository={cloneRepository}
                commitMenu={commitMenu}
                contextActions={contextActions}
                createRepository={createRepository}
                dismissToast={dismissToast}
                executeBranchAction={executeBranchAction}
                executeCommitAction={executeCommitAction}
                executeTabAction={executeTabAction}
                lastDirectory={persisted.last_directory}
                prompt={prompt}
                promptConfig={promptConfig}
                setBranchMenu={setBranchMenu}
                setCommitMenu={setCommitMenu}
                setPersisted={setPersisted}
                setPrompt={setPrompt}
                setSettingsOpen={setSettingsOpen}
                setStartDialog={setStartDialog}
                setTabMenu={setTabMenu}
                settings={persisted.settings}
                settingsOpen={settingsOpen}
                startDialog={startDialog}
                submitPrompt={submitPrompt}
                tabContextActions={tabContextActions}
                tabMenu={tabMenu}
                toasts={toasts}
            />
            {gitHubAccessOpen ? <GitHubAccessDialog onClose={() => setGitHubAccessOpen(false)} /> : null}
        </AppShell>
    );
}

export default App;
