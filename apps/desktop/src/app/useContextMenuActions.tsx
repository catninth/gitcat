import { Copy, Download, ExternalLink, GitBranchPlus, GitCommitHorizontal, GitMerge, GitPullRequestArrow, Link, PackageCheck, PackageOpen, RotateCcw, Tag, Trash2, Upload, X, } from "lucide-react";
import { PencilFilled } from "../components/ui/PencilFilled";
import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type RefObject, type SetStateAction, } from "react";

import { primaryBranchDecoration } from "../components/CommitGraph";
import type { ContextAction } from "../components/ContextMenu";
import { branchNameWithoutRemote, remoteNameOf, type BranchScope } from "../components/ref-sidebar";
import type { ToastMessage } from "../components/ToastRegion";
import { PULL_LABELS } from "../components/toolbar";
import { gitcatApi } from "../lib/api";
import { openExternal } from "../lib/platform";
import type {
    BranchInfo,
    CommitActionAvailability,
    CommitSummary,
    PersistedState,
    PullMode,
    RefLabel,
    RepositorySnapshot,
    ResetMode,
} from "../lib/types";
import {
    branchAcceptsPull,
    branchPushTarget,
    linkRemoteName,
    remoteBranchUrl,
    remoteCommitUrl,
} from "./branches";
import { expectedState } from "./snapshot";
import type {
    BranchMenuState,
    CommitMenuState,
    ConfirmState,
    PromptState,
    RunMutation,
    RuntimeRepository,
    TabMenuState,
} from "./state";
import { workspaceTabs } from "./workspace";

// The ref a commit-row context menu acts on, resolved against the snapshot.
// A click anywhere in the row uses the row's primary ref, so the label and the
// rest of the row open the same menu. Both the menu builder and the action
// dispatcher read this, so it lives outside the hook.
interface CommitRefContext {
    branchRef: RefLabel | null;
    displayName: string | null;
    headName: string | null;
    isHeadRef: boolean;
    isRemoteRef: boolean;
    localName: string | null;
    refInfo: BranchInfo | undefined;
    remoteName: string | null;
    scope: BranchScope;
    tracksHead: boolean;
}

function commitRefContext(
    commit: CommitSummary,
    decoration: RefLabel | null | undefined,
    snapshot: RepositorySnapshot | null,
): CommitRefContext {
    const headName = snapshot?.head.kind === "branch" ? snapshot.head.name : null;
    const clicked = decoration ?? primaryBranchDecoration(commit);
    const branchRef = clicked
        && (clicked.kind === "local_branch" || clicked.kind === "remote_branch")
        ? clicked
        : null;
    const isRemoteRef = branchRef?.kind === "remote_branch";
    const localName = branchRef
        ? (isRemoteRef ? branchNameWithoutRemote(branchRef.name) : branchRef.name)
        : null;
    const refInfo = branchRef
        ? (isRemoteRef
            ? snapshot?.remote_branches.find((candidate) => candidate.name === branchRef.name)
            : snapshot?.local_branches.find((candidate) => candidate.name === branchRef.name))
        : undefined;
    return {
        branchRef,
        displayName: branchRef?.name ?? null,
        headName,
        isHeadRef: Boolean(branchRef && !isRemoteRef && localName === headName),
        isRemoteRef,
        localName,
        refInfo,
        remoteName: linkRemoteName(
            snapshot,
            isRemoteRef && branchRef ? remoteNameOf(branchRef.name) : null,
        ),
        scope: isRemoteRef ? "remote" : "local",
        tracksHead: Boolean(isRemoteRef && headName && localName === headName),
    };
}

// Git refuses to delete the branch HEAD points at. The menu item stays live
// and answers with the reason instead of greying out, because a disabled row
// explains nothing about why the branch cannot go.
const CHECKED_OUT_DELETE_DETAIL = "Cannot delete currently checked out branch. Switch branches first.";

function checkedOutDeleteToast(name: string): Omit<ToastMessage, "id"> {
    return {
        tone: "error",
        title: `Delete Failed: refs/heads/${name}`,
        detail: CHECKED_OUT_DELETE_DETAIL,
    };
}

export interface ContextMenuActionsParams {
    activateRepositoryTab: (nextId: string | undefined) => void;
    activeRepository: RuntimeRepository | undefined;
    activeRepositoryIdRef: RefObject<string | null>;
    addToast: (toast: Omit<ToastMessage, "id">) => void;
    branchMenu: BranchMenuState | null;
    closeTab: (tabId: string) => void;
    commitActions: CommitActionAvailability[];
    commitMenu: CommitMenuState | null;
    copySha: (oid: string) => Promise<void>;
    defaultPullMode: PullMode;
    detailsOid: string | undefined;
    pullActiveRepository: (mode?: PullMode) => void;
    runMutation: RunMutation;
    setBranchMenu: Dispatch<SetStateAction<BranchMenuState | null>>;
    setCommitMenu: Dispatch<SetStateAction<CommitMenuState | null>>;
    setConfirmRequest: Dispatch<SetStateAction<ConfirmState>>;
    setPersisted: Dispatch<SetStateAction<PersistedState>>;
    setPrompt: Dispatch<SetStateAction<PromptState>>;
    setTabMenu: Dispatch<SetStateAction<TabMenuState | null>>;
    showError: (title: string, error: unknown) => void;
    snapshot: RepositorySnapshot | null;
    startCommitReword: (oid: string) => void;
    tabMenu: TabMenuState | null;
    workspace: PersistedState["workspace"];
}

export interface ContextMenuActions {
    branchContextActions: ContextAction[];
    checkoutRemoteBranch: (branch: BranchInfo) => void;
    contextActions: ContextAction[];
    executeBranchAction: (action: string) => void;
    executeCommitAction: (action: string) => void;
    executeTabAction: (action: string) => void;
    tabContextActions: ContextAction[];
}

export function useContextMenuActions({
    activateRepositoryTab,
    activeRepository,
    activeRepositoryIdRef,
    addToast,
    branchMenu,
    closeTab,
    commitActions,
    commitMenu,
    copySha,
    defaultPullMode,
    detailsOid,
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
    workspace,
}: ContextMenuActionsParams): ContextMenuActions {
    const [commitMenuActions, setCommitMenuActions] = useState<{
        oid: string;
        actions: CommitActionAvailability[];
    } | null>(null);
    const commitMenuActionSequence = useRef(0);

    useEffect(() => {
        if (!commitMenu || !activeRepository) {
            ++commitMenuActionSequence.current;
            setCommitMenuActions(null);
            return;
        }

        if (detailsOid === commitMenu.commit.oid) {
            ++commitMenuActionSequence.current;
            setCommitMenuActions({ oid: commitMenu.commit.oid, actions: commitActions });
            return;
        }

        const repositoryId = activeRepository.repository_id;
        const oid = commitMenu.commit.oid;
        const sequence = ++commitMenuActionSequence.current;
        setCommitMenuActions({ oid, actions: [] });
        void gitcatApi.commitActionAvailability(repositoryId, oid)
            .then((actions) => {
                if (
                    sequence !== commitMenuActionSequence.current
                    || activeRepositoryIdRef.current !== repositoryId
                ) return;
                setCommitMenuActions({ oid, actions });
            })
            .catch((error) => {
                if (
                    sequence === commitMenuActionSequence.current
                    && activeRepositoryIdRef.current === repositoryId
                ) showError("Commit actions could not be loaded", error);
            });
    }, [activeRepository, commitActions, commitMenu, detailsOid, showError]);

    const commitMenuActionMap = useMemo(() => {
        if (!commitMenuActions || commitMenuActions.oid !== commitMenu?.commit.oid) return new Map();
        return new Map(commitMenuActions.actions.map((action) => [action.kind, action]));
    }, [commitMenu?.commit.oid, commitMenuActions]);

    const contextActions = useMemo<ContextAction[]>(() => {
        if (!commitMenu) return [];
        if (commitMenu.commit.stash) {
            return [
                { id: "stash_apply", label: "Apply Stash", icon: <PackageOpen size={15} /> },
                { id: "stash_pop", label: "Pop Stash", icon: <PackageCheck size={15} /> },
                { id: "stash_drop", label: "Delete Stash", icon: <Trash2 size={15} />, danger: true, separatorBefore: true },
                { id: "copy", label: "Copy full commit SHA", icon: <Copy size={15} />, separatorBefore: true },
            ];
        }
        const enabled = (kind: CommitActionAvailability["kind"]) => (
            commitMenuActionMap.get(kind)?.enabled ?? false
        );
        const reference = commitRefContext(commitMenu.commit, commitMenu.decoration, snapshot);
        const { branchRef, displayName, headName, isHeadRef, isRemoteRef, localName, refInfo, remoteName } = reference;
        const resetTarget = headName ?? "branch";

        // Blocks shared by every variant, in the order GitKraken lists them.
        const resetItem: ContextAction = {
            id: "reset",
            label: `Reset ${resetTarget} to this commit`,
            icon: <RotateCcw size={15} />,
            disabled: !enabled("reset"),
            submenu: [
                { id: "reset:soft", label: "Soft - keep all changes" },
                { id: "reset:mixed", label: "Mixed - keep working copy but reset index" },
                { id: "reset:hard", label: "Hard - discard all changes" },
            ],
        };
        const historyItems = (withCherryPick: boolean): ContextAction[] => [
            { id: "branch", label: "Create branch here", icon: <GitBranchPlus size={15} />, disabled: !enabled("create_branch"), separatorBefore: true },
            ...(withCherryPick
                ? [{ id: "cherry_pick", label: "Cherry pick commit", icon: <GitPullRequestArrow size={15} />, disabled: !enabled("cherry_pick") }]
                : []),
            resetItem,
            { id: "reword", label: "Edit commit message", icon: <PencilFilled size={15} />, disabled: !enabled("reword") },
            { id: "revert", label: "Revert commit", icon: <RotateCcw size={15} />, disabled: !enabled("revert") },
        ];
        const copyItems = (): ContextAction[] => [
            ...(branchRef
                ? [{ id: "copy_branch", label: "Copy branch name", icon: <Copy size={15} />, separatorBefore: true }]
                : []),
            { id: "copy", label: "Copy commit sha", icon: <Copy size={15} />, separatorBefore: !branchRef },
            ...(branchRef && remoteName && remoteBranchUrl(snapshot, remoteName, localName ?? "")
                ? [
                    { id: "open_link_branch", label: `Open branch on ${remoteName}`, icon: <ExternalLink size={15} /> },
                    { id: "copy_link_branch", label: `Copy link to branch: ${displayName}`, icon: <Link size={15} /> },
                ]
                : []),
            ...(remoteName && remoteCommitUrl(snapshot, remoteName, commitMenu.commit.oid)
                ? [
                    { id: "open_link_commit", label: `Open this commit on ${remoteName}`, icon: <ExternalLink size={15} /> },
                    { id: "copy_link_commit", label: `Copy link to this commit on remote: ${remoteName}`, icon: <Link size={15} /> },
                ]
                : []),
        ];
        const tagItems = (): ContextAction[] => [
            { id: "tag", label: "Create tag here", icon: <Tag size={15} />, disabled: !enabled("create_tag"), separatorBefore: true },
            { id: "tag_annotated", label: "Create annotated tag here", icon: <Tag size={15} />, disabled: !enabled("create_tag") },
        ];

        // The checked-out branch: no checkout of itself, no merge into itself,
        // but it owns the remote-tracking actions.
        if (isHeadRef && refInfo) {
            return [
                { id: "pull", label: PULL_LABELS[defaultPullMode], icon: <Download size={15} />, disabled: !branchAcceptsPull(snapshot, refInfo, "local") },
                { id: "push", label: "Push", icon: <Upload size={15} />, disabled: !branchPushTarget(snapshot, refInfo, "local") },
                { id: "force_push", label: "Force push", icon: <Upload size={15} />, danger: true, disabled: !branchPushTarget(snapshot, refInfo, "local") },
                { id: "set_upstream", label: "Set upstream", icon: <Link size={15} /> },
                { id: "checkout", label: "Checkout this commit", icon: <GitCommitHorizontal size={15} />, disabled: !enabled("checkout"), separatorBefore: true },
                ...historyItems(false),
                { id: "rename_ref", label: `Rename ${displayName}`, icon: <PencilFilled size={15} />, separatorBefore: true },
                { id: "delete_ref", label: `Delete ${displayName}`, icon: <Trash2 size={15} />, danger: true },
                ...copyItems(),
                ...tagItems(),
            ];
        }

        // Any other branch ref: a remote-tracking ref of the checked-out branch
        // keeps pull/push, everything else only integrates into HEAD.
        if (branchRef) {
            const canDelete = !isRemoteRef && Boolean(refInfo);
            return [
                ...(reference.tracksHead && refInfo
                    ? [
                        { id: "pull", label: PULL_LABELS[defaultPullMode], icon: <Download size={15} />, disabled: !branchAcceptsPull(snapshot, refInfo, reference.scope) },
                        { id: "push", label: "Push", icon: <Upload size={15} />, disabled: !branchPushTarget(snapshot, refInfo, reference.scope) },
                        { id: "force_push", label: "Force push", icon: <Upload size={15} />, danger: true, disabled: !branchPushTarget(snapshot, refInfo, reference.scope) },
                    ]
                    : []),
                {
                    id: "merge_ref",
                    label: `Merge ${displayName} into ${headName ?? "HEAD"}`,
                    icon: <GitMerge size={15} />,
                    disabled: !headName,
                },
                {
                    id: "checkout_menu",
                    label: "Checkout",
                    icon: <GitCommitHorizontal size={15} />,
                    separatorBefore: true,
                    submenu: [
                        { id: "checkout_ref", label: displayName ?? "branch" },
                        { id: "checkout", label: "this commit", disabled: !enabled("checkout") },
                    ],
                },
                ...historyItems(true),
                ...(isRemoteRef
                    ? []
                    : [
                        { id: "rename_ref", label: `Rename ${displayName}`, icon: <PencilFilled size={15} />, disabled: !refInfo, separatorBefore: true },
                        { id: "delete_ref", label: `Delete ${displayName}`, icon: <Trash2 size={15} />, danger: true, disabled: !canDelete },
                    ]),
                ...copyItems(),
                ...tagItems(),
            ];
        }

        // A plain commit row.
        return [
            { id: "checkout", label: "Checkout this commit", icon: <GitCommitHorizontal size={15} />, disabled: !enabled("checkout") },
            ...historyItems(true),
            ...copyItems(),
            ...tagItems(),
        ];
    }, [commitMenu, commitMenuActionMap, defaultPullMode, snapshot]);

    const executeCommitAction = useCallback((action: string) => {
        if (!commitMenu) return;
        const commit = commitMenu.commit;
        const reference = commitRefContext(commit, commitMenu.decoration, snapshot);
        setCommitMenu(null);
        const copyText = (text: string, title: string) => {
            void navigator.clipboard.writeText(text)
                .then(() => addToast({ tone: "success", title }))
                .catch((error) => showError(`Could not copy ${title.toLowerCase()}`, error));
        };
        const openOrCopy = (url: string, copyTitle: string) => {
            void openExternal(url)
                .then((opened) => { if (!opened) copyText(url, copyTitle); })
                .catch((error) => showError("Could not open the link", error));
        };
        switch (action) {
            case "copy":
                void copySha(commit.oid);
                break;
            case "copy_branch":
                if (reference.displayName) copyText(reference.displayName, "Branch name copied");
                break;
            // Opening falls back to the clipboard: outside the desktop
            // application there is no browser to hand the link to.
            case "open_link_branch": {
                const url = reference.remoteName && reference.localName
                    ? remoteBranchUrl(snapshot, reference.remoteName, reference.localName)
                    : null;
                if (url) openOrCopy(url, "Branch link copied");
                break;
            }
            case "open_link_commit": {
                const url = reference.remoteName
                    ? remoteCommitUrl(snapshot, reference.remoteName, commit.oid)
                    : null;
                if (url) openOrCopy(url, "Commit link copied");
                break;
            }
            case "copy_link_branch": {
                const url = reference.remoteName && reference.localName
                    ? remoteBranchUrl(snapshot, reference.remoteName, reference.localName)
                    : null;
                if (url) copyText(url, "Branch link copied");
                break;
            }
            case "copy_link_commit": {
                const url = reference.remoteName
                    ? remoteCommitUrl(snapshot, reference.remoteName, commit.oid)
                    : null;
                if (url) copyText(url, "Commit link copied");
                break;
            }
            case "checkout_ref":
                if (reference.localName) {
                    const name = reference.localName;
                    void runMutation(`Checked out ${name}`, (repository) => gitcatApi.checkoutBranch(repository.repository_id, name));
                }
                break;
            case "merge_ref":
                if (reference.displayName) {
                    const name = reference.displayName;
                    void runMutation(`Merged ${name}`, (repository) => gitcatApi.mergeBranch(repository.repository_id, name));
                }
                break;
            case "pull":
                pullActiveRepository();
                break;
            case "push": {
                const target = reference.refInfo
                    ? branchPushTarget(snapshot, reference.refInfo, reference.scope)
                    : null;
                if (!target) break;
                void runMutation("Push complete", (repository) => gitcatApi.push(repository.repository_id, {
                    remote: target.remote,
                    branch: target.branch,
                    set_upstream: target.setUpstream,
                }));
                break;
            }
            case "force_push": {
                const target = reference.refInfo
                    ? branchPushTarget(snapshot, reference.refInfo, reference.scope)
                    : null;
                // Overwriting the remote is asked for here and carried out by
                // the confirmation, which is also where the lease is chosen.
                if (target) setConfirmRequest({ kind: "force_push", remote: target.remote, branch: target.branch, ignoreRemote: false });
                break;
            }
            case "set_upstream":
                if (reference.refInfo) setPrompt({ kind: "set_upstream", branch: reference.refInfo });
                break;
            case "rename_ref":
                if (reference.refInfo && !reference.isRemoteRef) setPrompt({ kind: "rename_branch", branch: reference.refInfo });
                break;
            case "delete_ref":
                if (reference.refInfo && !reference.isRemoteRef) {
                    if (reference.refInfo.is_head) addToast(checkedOutDeleteToast(reference.refInfo.name));
                    else setConfirmRequest({ kind: "delete_branch", name: reference.refInfo.name, force: false });
                }
                break;
            case "reword":
                startCommitReword(commit.oid);
                break;
            case "stash_apply":
                if (commit.stash) {
                    const { oid } = commit.stash;
                    void runMutation("Stash applied", (repository) => gitcatApi.stashApply(repository.repository_id, oid, false));
                }
                break;
            case "stash_pop":
                if (commit.stash) {
                    const { oid } = commit.stash;
                    void runMutation(
                        "Stash popped",
                        (repository) => gitcatApi.stashApply(repository.repository_id, oid, true),
                        { wipTitleHint: commit.subject },
                    );
                }
                break;
            case "stash_drop":
                if (commit.stash) setConfirmRequest({
                    kind: "delete_stash",
                    oid: commit.stash.oid,
                    selector: commit.stash.selector,
                    message: commit.subject,
                });
                break;
            case "branch":
                setPrompt({ kind: "create_branch", startOid: commit.oid });
                break;
            case "tag":
                setPrompt({ kind: "create_tag", oid: commit.oid, annotated: false });
                break;
            case "tag_annotated":
                setPrompt({ kind: "create_tag", oid: commit.oid, annotated: true });
                break;
            case "checkout":
                if (window.confirm(`Check out ${commit.short_oid} in detached HEAD state?`)) {
                    void runMutation("Commit checked out", (repository) => gitcatApi.checkoutCommit(repository.repository_id, commit.oid));
                }
                break;
            case "cherry_pick": {
                const mainline = commit.parent_oids.length > 1 ? Number(window.prompt("Mainline parent number", "1")) : null;
                if (commit.parent_oids.length > 1 && (!mainline || mainline < 1)) break;
                void runMutation("Commit cherry-picked", (repository) => gitcatApi.cherryPick(repository.repository_id, commit.oid, mainline));
                break;
            }
            case "revert": {
                const mainline = commit.parent_oids.length > 1 ? Number(window.prompt("Mainline parent number", "1")) : null;
                if (commit.parent_oids.length > 1 && (!mainline || mainline < 1)) break;
                void runMutation("Commit reverted", (repository) => gitcatApi.revertCommit(repository.repository_id, commit.oid, mainline));
                break;
            }
            case "reset:soft":
            case "reset:mixed":
            case "reset:hard": {
                if (!snapshot) break;
                const mode = action.slice("reset:".length) as ResetMode;
                const branchName = snapshot.head.kind === "branch" ? snapshot.head.name : "branch";
                if (
                    mode === "hard"
                    && !window.confirm(`Hard reset ${branchName} to ${commit.short_oid}? This will discard uncommitted changes and cannot be undone.`)
                ) break;
                void runMutation("Branch reset", (repository) => gitcatApi.resetCommit(
                    repository.repository_id,
                    commit.oid,
                    mode,
                    mode === "hard",
                    expectedState(snapshot),
                ));
                break;
            }
        }
    }, [
        addToast,
        commitMenu,
        copySha,
        pullActiveRepository,
        runMutation,
        setConfirmRequest,
        setPrompt,
        showError,
        snapshot,
        startCommitReword,
    ]);

    const tabContextActions = useMemo<ContextAction[]>(() => {
        if (!tabMenu) return [];
        const orderedTabs = workspaceTabs(workspace);
        const tabIndex = orderedTabs.findIndex((tab) => tab.id === tabMenu.tab.id);
        return [
            { id: "activate", label: "Activate repository", icon: <GitCommitHorizontal size={15} /> },
            { id: "alias", label: "Rename tab", icon: <Tag size={15} />, separatorBefore: true },
            { id: "copy_path", label: "Copy repository path", icon: <Copy size={15} /> },
            { id: "close_others", label: "Close other repositories", icon: <X size={15} />, disabled: orderedTabs.length <= 1, separatorBefore: true },
            { id: "close_right", label: "Close repositories to the right", icon: <X size={15} />, disabled: tabIndex < 0 || tabIndex === orderedTabs.length - 1 },
            { id: "close", label: "Close repository", icon: <X size={15} /> },
        ];
    }, [workspace, tabMenu]);

    const executeTabAction = useCallback((action: string) => {
        if (!tabMenu) return;
        const selectedTab = tabMenu.tab;
        setTabMenu(null);
        if (action === "activate") {
            setPersisted((current) => ({ ...current, workspace: { ...current.workspace, active_tab_id: selectedTab.id } }));
        } else if (action === "alias") {
            setPrompt({ kind: "alias_tab", tabId: selectedTab.id, current: selectedTab.label });
        } else if (action === "copy_path") {
            void navigator.clipboard.writeText(selectedTab.path)
                .then(() => addToast({ tone: "success", title: "Repository path copied" }))
                .catch((error) => showError("Could not copy repository path", error));
        } else if (action === "close_others") {
            workspaceTabs(workspace)
                .filter((tab) => tab.id !== selectedTab.id)
                .forEach((tab) => closeTab(tab.id));
            activateRepositoryTab(selectedTab.id);
        } else if (action === "close_right") {
            const tabs = workspaceTabs(workspace);
            const index = tabs.findIndex((tab) => tab.id === selectedTab.id);
            tabs.slice(index + 1).forEach((tab) => closeTab(tab.id));
        } else if (action === "close") {
            closeTab(selectedTab.id);
        }
    }, [activateRepositoryTab, addToast, closeTab, workspace, showError, tabMenu]);

    const checkoutRemoteBranch = useCallback((branch: BranchInfo) => {
        const localName = branchNameWithoutRemote(branch.name);
        void runMutation(`Checked out ${localName}`, (repository) => gitcatApi.checkoutBranch(repository.repository_id, localName));
    }, [runMutation]);

    const branchContextActions = useMemo<ContextAction[]>(() => {
        if (!branchMenu) return [];
        const { branch, scope } = branchMenu;
        const isLocal = scope === "local";
        const displayName = isLocal ? branch.name : branchNameWithoutRemote(branch.name);
        return [
            {
                id: "pull",
                label: PULL_LABELS[defaultPullMode],
                icon: <Download size={15} />,
                disabled: !branchAcceptsPull(snapshot, branch, scope),
            },
            {
                id: "push",
                label: "Push",
                icon: <Upload size={15} />,
                disabled: !branchPushTarget(snapshot, branch, scope),
            },
            {
                id: "force_push",
                label: "Force push",
                icon: <Upload size={15} />,
                danger: true,
                disabled: !branchPushTarget(snapshot, branch, scope),
            },
            { id: "create_branch", label: "Create branch here", icon: <GitBranchPlus size={15} />, separatorBefore: true },
            { id: "rename", label: `Rename ${displayName}`, icon: <PencilFilled size={15} />, disabled: !isLocal, separatorBefore: true },
            {
                id: "delete",
                label: `Delete ${displayName}`,
                icon: <Trash2 size={15} />,
                danger: true,
                disabled: !isLocal,
            },
            { id: "copy", label: "Copy branch name", icon: <Copy size={15} />, separatorBefore: true },
        ];
    }, [branchMenu, defaultPullMode, snapshot]);

    const executeBranchAction = useCallback((action: string) => {
        if (!branchMenu) return;
        const { branch, scope } = branchMenu;
        const isLocal = scope === "local";
        const displayName = isLocal ? branch.name : branchNameWithoutRemote(branch.name);
        setBranchMenu(null);
        switch (action) {
            case "pull":
                pullActiveRepository();
                break;
            case "push": {
                const target = branchPushTarget(snapshot, branch, scope);
                if (!target) break;
                void runMutation("Push complete", (repository) => gitcatApi.push(repository.repository_id, {
                    remote: target.remote,
                    branch: target.branch,
                    set_upstream: target.setUpstream,
                }));
                break;
            }
            case "force_push": {
                const target = branchPushTarget(snapshot, branch, scope);
                if (target) setConfirmRequest({ kind: "force_push", remote: target.remote, branch: target.branch, ignoreRemote: false });
                break;
            }
            case "create_branch":
                setPrompt({ kind: "create_branch", startOid: branch.oid });
                break;
            case "rename":
                if (isLocal) setPrompt({ kind: "rename_branch", branch });
                break;
            case "delete":
                if (!isLocal) break;
                if (branch.is_head) addToast(checkedOutDeleteToast(branch.name));
                else setConfirmRequest({ kind: "delete_branch", name: branch.name, force: false });
                break;
            case "copy":
                void navigator.clipboard.writeText(displayName)
                    .then(() => addToast({ tone: "success", title: "Branch name copied" }))
                    .catch((error) => showError("Could not copy branch name", error));
                break;
        }
    }, [addToast, branchMenu, pullActiveRepository, runMutation, showError, snapshot]);

    return {
        branchContextActions,
        checkoutRemoteBranch,
        contextActions,
        executeBranchAction,
        executeCommitAction,
        executeTabAction,
        tabContextActions,
    };
}
