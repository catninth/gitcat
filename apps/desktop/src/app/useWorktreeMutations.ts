import { useCallback, useRef, type Dispatch, type RefObject, type SetStateAction } from "react";

import type { FolderCollapseTarget } from "../components/file-tree";
import type { ToastMessage } from "../components/ToastRegion";
import { gitcatApi } from "../lib/api";
import type { ConflictToast } from "../lib/conflicts";
import type { FileViewMode, HistoryPage, RepositorySnapshot } from "../lib/types";
import { createMutationQueue } from "./mutationQueue";
import { expectedState, isMutationResult } from "./snapshot";
import type { CenterView, RuntimeRepository } from "./state";
import { nextWorktreeSelection, optimisticWorktreeSelection, optimisticWorktreeSnapshot, type WorktreeStageAction } from "./worktree";

export interface WorktreeMutationsParams {
    activeRepository: RuntimeRepository | undefined;
    activeRepositoryIdRef: RefObject<string | null>;
    addToast: (toast: Omit<ToastMessage, "id">) => void;
    busy: boolean;
    centerView: CenterView;
    fileViewMode: FileViewMode;
    history: HistoryPage | null;
    loadOverview: (repository: RuntimeRepository, preserveSelection?: boolean, showLoading?: boolean) => Promise<void>;
    openWorktreeDiffRef: RefObject<(path: string, staged: boolean) => void>;
    overviewLoadSequence: RefObject<number>;
    overviewLoading: boolean;
    pendingSelectionRef: RefObject<{ index: number; subject: string } | null>;
    selectedWorktreeFile: { path: string; staged: boolean } | null;
    setBusy: Dispatch<SetStateAction<boolean>>;
    setSelectedWorktreeFile: Dispatch<SetStateAction<{ path: string; staged: boolean } | null>>;
    setSnapshot: Dispatch<SetStateAction<RepositorySnapshot | null>>;
    setStageCollapse: Dispatch<SetStateAction<{ target: FolderCollapseTarget; staged: boolean; token: number } | null>>;
    setWipTitleHint: Dispatch<SetStateAction<string | null>>;
    showError: (title: string, error: unknown) => void;
    snapshot: RepositorySnapshot | null;
    swapWorktreeDiffSideRef: RefObject<(path: string, staged: boolean) => void>;
}

export function useWorktreeMutations({
    activeRepository,
    activeRepositoryIdRef,
    addToast,
    busy,
    centerView,
    fileViewMode,
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
}: WorktreeMutationsParams) {
    const applyOptimisticWorktreeMutation = useCallback((
        action: WorktreeStageAction,
        paths: string[],
        selection?: { path: string; staged: boolean } | null,
    ) => {
        if (!activeRepository || !snapshot || !paths.length) return undefined;
        const repositoryId = activeRepository.repository_id;
        const previousSnapshot = snapshot;
        const previousSelectedWorktreeFile = selectedWorktreeFile;

        setSnapshot((current) => {
            if (!current || activeRepositoryIdRef.current !== repositoryId) return current;
            return optimisticWorktreeSnapshot(current, action, paths);
        });
        setSelectedWorktreeFile((current) => {
            if (activeRepositoryIdRef.current !== repositoryId) return current;
            return selection ?? optimisticWorktreeSelection(current, action, paths);
        });

        return () => {
            if (activeRepositoryIdRef.current !== repositoryId) return;
            setSnapshot(previousSnapshot);
            setSelectedWorktreeFile(previousSelectedWorktreeFile);
        };
    }, [activeRepository, selectedWorktreeFile, snapshot]);

    // The queue reads the hold through a ref because a task can be started long
    // after the render that enqueued it, and the `busy` it captured then says
    // nothing about the repository now.
    const blockedRef = useRef(false);
    blockedRef.current = busy || overviewLoading;
    const queueRef = useRef<ReturnType<typeof createMutationQueue> | null>(null);
    if (!queueRef.current) queueRef.current = createMutationQueue({ blocked: () => blockedRef.current });

    const runMutation = useCallback(async (
        title: string,
        operation: (repository: RuntimeRepository) => Promise<unknown>,
        options?: {
            silent?: boolean;
            optimistic?: () => (() => void) | undefined;
            onError?: (error: unknown) => boolean;
            queueKey?: string;
            wipTitleHint?: string;
            conflictToasts?: ConflictToast[] | (() => ConflictToast[]);
        },
    ): Promise<boolean> => {
        if (!activeRepository) return false;
        // The repository is fixed when the operation is asked for, not when it
        // runs: a queued push belongs to the tab it was pressed in.
        const repository = activeRepository;
        return queueRef.current!.run(async () => {
            let rollbackOptimistic: (() => void) | undefined;
            let operationCompleted = false;
            ++overviewLoadSequence.current;
            setBusy(true);
            try {
                rollbackOptimistic = options?.optimistic?.();
                const result = await operation(repository);
                operationCompleted = true;
                if (activeRepositoryIdRef.current === repository.repository_id) {
                    setWipTitleHint(options?.wipTitleHint ?? null);
                    await loadOverview(repository, true)
                        .catch((error) => showError("Refresh failed", error));
                }
                if (isMutationResult(result) && result.conflicts.length) {
                    const named = typeof options?.conflictToasts === "function"
                        ? options.conflictToasts()
                        : options?.conflictToasts;
                    if (named?.length) {
                        for (const toast of named) addToast({ tone: "error", ...toast });
                    } else {
                        addToast({
                            tone: "info",
                            title: `${title}: attention required`,
                            detail: `${result.conflicts.length} conflict${result.conflicts.length === 1 ? " remains" : "s remain"}. Resolve them in the Working tree panel.`,
                        });
                    }
                } else if (isMutationResult(result) && result.notice) {
                    // The command reported that it had nothing to do; saying
                    // only "complete" would claim work that never happened.
                    addToast({ tone: "info", title, detail: result.notice });
                } else if (!options?.silent) {
                    addToast({ tone: "success", title });
                }
                return true;
            } catch (error) {
                if (!operationCompleted) rollbackOptimistic?.();
                if (!options?.onError?.(error)) showError(`${title} failed`, error);
                return false;
            } finally {
                setBusy(false);
            }
        }, {
            key: options?.queueKey,
            // Staging is silent and frequent; a queue notice belongs to the
            // commands the user pressed a button or a shortcut for.
            onQueued: options?.silent
                ? undefined
                : () => addToast({ tone: "info", title: "Queued", detail: "Waiting for the running operation to finish." }),
        });
    }, [activeRepository, addToast, loadOverview, setWipTitleHint, showError]);

    const rewordCommit = useCallback((oid: string, message: string) => {
        if (!snapshot) return Promise.resolve(false);
        const index = history?.commits.findIndex((commit) => commit.oid === oid) ?? -1;
        const expected = expectedState(snapshot);
        return runMutation("Commit message updated", (repository) => gitcatApi
            .rewordCommit(repository.repository_id, oid, message, expected)
            .then((result) => {
                if (index >= 0) pendingSelectionRef.current = { index, subject: message.split("\n")[0].trim() };
                return result;
            }));
    }, [history, runMutation, snapshot]);

    const worktreeDiffFollowUp = useCallback((action: WorktreeStageAction, paths: string[]) => {
        if (!activeRepository || busy || overviewLoading) return null;
        if (!snapshot || centerView !== "diff" || !selectedWorktreeFile) return null;
        if (selectedWorktreeFile.staged !== (action === "unstage")) return null;
        if (!paths.includes(selectedWorktreeFile.path)) return null;
        return nextWorktreeSelection(
            snapshot.status.entries,
            selectedWorktreeFile,
            action,
            paths,
            fileViewMode,
        );
    }, [
        activeRepository,
        busy,
        centerView,
        overviewLoading,
        fileViewMode,
        selectedWorktreeFile,
        snapshot,
    ]);

    const stagePaths = useCallback((paths: string[], collapse?: FolderCollapseTarget) => {
        if (collapse) setStageCollapse((current) => ({ target: collapse, staged: true, token: (current?.token ?? 0) + 1 }));
        const followUp = worktreeDiffFollowUp("stage", paths);
        const crossed = followUp?.staged === true;
        if (followUp && !crossed) openWorktreeDiffRef.current(followUp.path, followUp.staged);
        void runMutation(
            "Files staged",
            (repository) => gitcatApi.stagePaths(repository.repository_id, paths),
            {
                silent: true,
                optimistic: () => applyOptimisticWorktreeMutation("stage", paths, followUp),
            },
        ).then((done) => {
            if (done && followUp && crossed) swapWorktreeDiffSideRef.current(followUp.path, followUp.staged);
        });
    }, [applyOptimisticWorktreeMutation, runMutation, worktreeDiffFollowUp]);

    const unstagePaths = useCallback((paths: string[], collapse?: FolderCollapseTarget) => {
        if (collapse) setStageCollapse((current) => ({ target: collapse, staged: false, token: (current?.token ?? 0) + 1 }));
        const followUp = worktreeDiffFollowUp("unstage", paths);
        const crossed = followUp?.staged === false;
        if (followUp && !crossed) openWorktreeDiffRef.current(followUp.path, followUp.staged);
        void runMutation(
            "Files unstaged",
            (repository) => gitcatApi.unstagePaths(repository.repository_id, paths),
            {
                silent: true,
                optimistic: () => applyOptimisticWorktreeMutation("unstage", paths, followUp),
            },
        ).then((done) => {
            if (done && followUp && crossed) swapWorktreeDiffSideRef.current(followUp.path, followUp.staged);
        });
    }, [applyOptimisticWorktreeMutation, runMutation, worktreeDiffFollowUp]);

    const createPatchFile = useCallback(async (paths: string[], staged: boolean) => {
        if (!activeRepository) return;
        const repositoryId = activeRepository.repository_id;
        try {
            let destination = `${paths[0]?.split(/[\\/]/).at(-1) ?? "changes"}.patch`;
            if (gitcatApi.runtime === "tauri") {
                const { save } = await import("@tauri-apps/plugin-dialog");
                const chosen = await save({
                    title: "Save patch",
                    defaultPath: destination,
                    filters: [{ name: "Patch", extensions: ["patch", "diff"] }],
                });
                if (!chosen) return;
                destination = chosen;
            }
            await gitcatApi.savePatch(repositoryId, paths, staged, destination);
            addToast({ tone: "success", title: "Patch created", detail: destination });
        } catch (error) {
            showError("Create patch failed", error);
        }
    }, [activeRepository, addToast, showError]);

    return { createPatchFile, rewordCommit, runMutation, stagePaths, unstagePaths };
}
