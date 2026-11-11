import { useCallback, useEffect, useState, type Dispatch, type RefObject, type SetStateAction } from "react";

import { absoluteFilePath } from "../components/worktree/WorktreePanel";
import { gitcatApi } from "../lib/api";
import {
    conflictDecision,
    conflictSideLabels,
    looksLikeHelperCopy,
    type ConflictContext,
    type ConflictDecision,
} from "../lib/conflicts";
import type {
    ConflictFileDetails,
    ConflictLineEndingPolicy,
    ConflictResolution,
    ConflictResolutionWarning,
    RepositorySnapshot,
    StatusEntry,
    SubmoduleConflictDetails,
} from "../lib/types";
import { currentBranch } from "./branches";
import type { CenterView, RunMutation, RuntimeRepository } from "./state";

/**
 * What a click on a conflicted file opens when it is not a text merge: a
 * question for a deleted/added/renamed path, or the submodule pointer view.
 */
export type ConflictPrompt =
    | { kind: "decision"; entry: StatusEntry; decision: ConflictDecision }
    | { kind: "submodule"; entry: StatusEntry; details: SubmoduleConflictDetails };

export interface ConflictActionsParams {
    activeRepository: RuntimeRepository | undefined;
    activeRepositoryIdRef: RefObject<string | null>;
    busy: boolean;
    conflictEditor: ConflictFileDetails | null;
    openRepositoryPath: (path: string) => Promise<void>;
    runMutation: RunMutation;
    setBusy: Dispatch<SetStateAction<boolean>>;
    setCenterView: Dispatch<SetStateAction<CenterView>>;
    setConflictEditor: Dispatch<SetStateAction<ConflictFileDetails | null>>;
    setSelectedWorktreeFile: Dispatch<SetStateAction<{ path: string; staged: boolean } | null>>;
    showError: (title: string, error: unknown) => void;
    snapshot: RepositorySnapshot | null;
}

export function conflictContextOf(snapshot: RepositorySnapshot | null): ConflictContext {
    return {
        operation: snapshot?.operation_state ?? "normal",
        branchName: currentBranch(snapshot),
        source: snapshot?.operation_source ?? null,
        headShort: snapshot && snapshot.head.kind !== "unborn" ? snapshot.head.oid.slice(0, 7) : null,
    };
}

function describeWarning(warning: ConflictResolutionWarning): string {
    return warning.reason === "markers"
        ? `${warning.path} still contains conflict markers`
        : `${warning.path} is a copy Git set aside for a file/folder conflict`;
}

/**
 * GitKraken marks a file resolved without looking at it, and so commits
 * leftover `<<<<<<<` blocks and `fd~HEAD` copies without a word. Here the
 * paths are checked first and the user is asked only when something is wrong.
 */
async function confirmMarkResolved(repositoryId: string, paths: string[]): Promise<boolean> {
    const warnings = await gitcatApi.conflictResolutionWarnings(repositoryId, paths);
    if (!warnings.length) return true;
    const shown = warnings.slice(0, 12).map((warning) => `• ${describeWarning(warning)}`);
    if (warnings.length > shown.length) shown.push(`• …and ${warnings.length - shown.length} more`);
    return window.confirm(
        `Mark resolved anyway?\n\n${shown.join("\n")}\n\nThe files will be staged exactly as they are now.`,
    );
}

export function useConflictActions({
    activeRepository,
    activeRepositoryIdRef,
    busy,
    conflictEditor,
    openRepositoryPath,
    runMutation,
    setBusy,
    setCenterView,
    setConflictEditor,
    setSelectedWorktreeFile,
    showError,
    snapshot,
}: ConflictActionsParams) {
    const [conflictPrompt, setConflictPrompt] = useState<ConflictPrompt | null>(null);

    const openConflictEditor = useCallback(async (entry: StatusEntry) => {
        if (!activeRepository || busy) return;
        const repositoryId = activeRepository.repository_id;
        const labels = conflictSideLabels(conflictContextOf(snapshot));
        const allPaths = snapshot?.status.entries.map((item) => item.path) ?? [];
        const decision = entry.submodule
            ? null
            : conflictDecision(entry.conflict_kind, entry.path, labels, looksLikeHelperCopy(entry.path, allPaths));
        if (decision) {
            setConflictEditor(null);
            setConflictPrompt({ kind: "decision", entry, decision });
            setSelectedWorktreeFile({ path: entry.path, staged: false });
            setCenterView("graph");
            return;
        }
        setBusy(true);
        try {
            if (entry.submodule) {
                const details = await gitcatApi.submoduleConflictDetails(repositoryId, entry.path);
                if (activeRepositoryIdRef.current !== repositoryId) return;
                setConflictEditor(null);
                setConflictPrompt({ kind: "submodule", entry, details });
            } else {
                const next = await gitcatApi.conflictDetails(repositoryId, entry.path);
                if (activeRepositoryIdRef.current !== repositoryId) return;
                setConflictPrompt(null);
                setConflictEditor(next);
            }
            setSelectedWorktreeFile({ path: entry.path, staged: false });
            setCenterView("merge");
        } catch (error) {
            if (activeRepositoryIdRef.current === repositoryId) showError("Conflict editor could not be opened", error);
        } finally {
            setBusy(false);
        }
    }, [activeRepository, busy, showError, snapshot]);

    const closeConflictEditor = useCallback(() => {
        setConflictEditor(null);
        setConflictPrompt(null);
        setSelectedWorktreeFile(null);
        setCenterView("graph");
    }, []);

    // The backend refuses the write when the conflict or the working copy moved
    // under the editor, so a stale composition never silently overwrites one.
    const saveConflictResult = useCallback((text: string, lineEnding: ConflictLineEndingPolicy) => {
        const current = conflictEditor;
        if (!current) return;
        void runMutation("Conflict result saved", (repository) => gitcatApi.saveConflictResult(
            repository.repository_id,
            current.path,
            text,
            lineEnding,
            current.expected_state,
        )).then((success) => { if (success) closeConflictEditor(); });
    }, [closeConflictEditor, conflictEditor, runMutation]);

    // Resolving the file from anywhere else -- the sidebar menu, an external
    // tool, `git add` in a terminal -- leaves the editor showing a conflict that
    // no longer exists.
    useEffect(() => {
        if (!snapshot) return;
        const stillConflicted = (path: string) => snapshot.status.entries.find((item) => item.path === path)?.conflicted;
        if (conflictEditor && !stillConflicted(conflictEditor.path)) closeConflictEditor();
        else if (conflictPrompt && !stillConflicted(conflictPrompt.entry.path)) closeConflictEditor();
    }, [closeConflictEditor, conflictEditor, conflictPrompt, snapshot]);

    const resolveConflictEntry = useCallback((entry: StatusEntry, resolution: ConflictResolution) => {
        if (!activeRepository) return;
        if (resolution === "delete" && !window.confirm(`Delete '${entry.path}' as the conflict resolution?`)) return;
        const repositoryId = activeRepository.repository_id;
        void (async () => {
            if (resolution === "mark_resolved") {
                try {
                    if (!await confirmMarkResolved(repositoryId, [entry.path])) return;
                } catch (error) {
                    showError("Conflict could not be checked", error);
                    return;
                }
            }
            await runMutation("Conflict resolved", async (repository) => {
                const conflict = await gitcatApi.conflictDetails(repository.repository_id, entry.path);
                return gitcatApi.resolveConflict(
                    repository.repository_id,
                    entry.path,
                    resolution,
                    conflict.expected_state,
                );
            }, { silent: true });
        })();
    }, [activeRepository, runMutation, showError]);

    // One file from a menu is a direct choice, as in GitKraken; a folder or
    // "for all" takes a side for files the user has not looked at, so that is
    // still confirmed.
    const resolveConflictPaths = useCallback((paths: string[], resolution: ConflictResolution) => {
        if (!paths.length || !activeRepository) return;
        const count = `${paths.length} conflicted file${paths.length === 1 ? "" : "s"}`;
        if (resolution === "delete" && !window.confirm(`Delete ${count} as the conflict resolution?`)) return;
        if ((resolution === "ours" || resolution === "theirs") && paths.length > 1
            && !window.confirm(`Take the ${resolution === "ours" ? "current" : "incoming"} version for ${count}?`)) return;
        const repositoryId = activeRepository.repository_id;
        void (async () => {
            if (resolution === "mark_resolved") {
                try {
                    if (!await confirmMarkResolved(repositoryId, paths)) return;
                } catch (error) {
                    showError("Conflicts could not be checked", error);
                    return;
                }
            }
            await runMutation(
                `${paths.length} conflict${paths.length === 1 ? "" : "s"} resolved`,
                (repository) => gitcatApi.resolveConflicts(repository.repository_id, paths, resolution),
                { silent: true },
            );
        })();
    }, [activeRepository, runMutation, showError]);

    const chooseConflictPrompt = useCallback((resolution: ConflictResolution) => {
        if (!conflictPrompt) return;
        resolveConflictEntry(conflictPrompt.entry, resolution);
    }, [conflictPrompt, resolveConflictEntry]);

    const openConflictSubmodule = useCallback(() => {
        if (conflictPrompt?.kind !== "submodule" || !activeRepository) return;
        void openRepositoryPath(absoluteFilePath(activeRepository.info.root, conflictPrompt.entry.path));
    }, [activeRepository, conflictPrompt, openRepositoryPath]);

    return {
        chooseConflictPrompt,
        closeConflictEditor,
        conflictPrompt,
        openConflictEditor,
        openConflictSubmodule,
        resolveConflictEntry,
        resolveConflictPaths,
        saveConflictResult,
    };
}
