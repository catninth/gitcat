import { useCallback, useEffect, type Dispatch, type RefObject, type SetStateAction } from "react";

import { isWholeFileMode, type DiffViewMode } from "../components/diff";
import { gitcatApi } from "../lib/api";
import { sameFileDiff } from "../lib/diffs";
import type { ChangedFile, DiffRequest, FileDiff, RepositorySnapshot, StatusEntry } from "../lib/types";
import type { CenterView, RuntimeRepository } from "./state";

export interface DiffPaneParams {
    activeRepository: RuntimeRepository | undefined;
    activeRepositoryIdRef: RefObject<string | null>;
    autoReloadDiffRef: RefObject<() => void>;
    diffContextLines: number;
    diffLoadSequence: RefObject<number>;
    diffLoading: boolean;
    diffMaxBytes: number;
    diffMode: DiffViewMode;
    diffWholeFileRef: RefObject<boolean>;
    openDiffRequestRef: RefObject<{ sequence: number; request: DiffRequest } | null>;
    openWorktreeDiffRef: RefObject<(path: string, staged: boolean) => void>;
    selectedOid: string | null;
    selectedWorktreeFile: { path: string; staged: boolean } | null;
    setCenterView: Dispatch<SetStateAction<CenterView>>;
    setDiff: Dispatch<SetStateAction<FileDiff | null>>;
    setDiffLoading: Dispatch<SetStateAction<boolean>>;
    setSelectedPath: Dispatch<SetStateAction<string | undefined>>;
    setSelectedWorktreeFile: Dispatch<SetStateAction<{ path: string; staged: boolean } | null>>;
    showError: (title: string, error: unknown) => void;
    snapshot: RepositorySnapshot | null;
    swapWorktreeDiffSideRef: RefObject<(path: string, staged: boolean) => void>;
}

export function useDiffPane({
    activeRepository,
    activeRepositoryIdRef,
    autoReloadDiffRef,
    diffContextLines,
    diffLoadSequence,
    diffLoading,
    diffMaxBytes,
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
}: DiffPaneParams) {
    const closeDiff = useCallback(() => {
        ++diffLoadSequence.current;
        setDiff(null);
        setDiffLoading(false);
        setSelectedPath(undefined);
        setSelectedWorktreeFile(null);
        setCenterView("graph");
    }, []);

    useEffect(() => {
        if (!selectedWorktreeFile || !snapshot) return;
        const selectedEntry = snapshot.status.entries.find((entry) => entry.path === selectedWorktreeFile.path);
        const stillVisible = selectedWorktreeFile.staged
            ? Boolean(selectedEntry?.index && !selectedEntry.conflicted)
            : Boolean(selectedEntry?.worktree || selectedEntry?.conflicted);
        if (!stillVisible) closeDiff();
    }, [closeDiff, selectedWorktreeFile, snapshot]);

    const loadDiff = useCallback(async (request: DiffRequest, silent = false) => {
        if (!activeRepository) return;
        const repositoryId = activeRepository.repository_id;
        const sequence = ++diffLoadSequence.current;
        openDiffRequestRef.current = { sequence, request };
        if (!silent) {
            setSelectedPath(request.path);
            setCenterView("diff");
            setDiff(null);
            setDiffLoading(true);
        }
        try {
            const nextDiff = await gitcatApi.diff(repositoryId, request);
            if (
                sequence !== diffLoadSequence.current
                || activeRepositoryIdRef.current !== repositoryId
            ) return;
            setDiff((current) => (current && sameFileDiff(current, nextDiff) ? current : nextDiff));
        } catch (error) {
            if (
                !silent
                && sequence === diffLoadSequence.current
                && activeRepositoryIdRef.current === repositoryId
            ) showError("Diff could not be loaded", error);
        } finally {
            if (!silent && sequence === diffLoadSequence.current) setDiffLoading(false);
        }
    }, [activeRepository, showError]);

    const reloadOpenWorktreeDiff = useCallback(() => {
        const open = openDiffRequestRef.current;
        if (!activeRepository || !open || diffLoading) return;
        if (open.sequence !== diffLoadSequence.current) return;
        const target = open.request.target.kind;
        if (target !== "worktree" && target !== "staged") return;
        return loadDiff(open.request, true);
    }, [activeRepository, diffLoading, loadDiff]);

    useEffect(() => {
        autoReloadDiffRef.current = reloadOpenWorktreeDiff;
    }, [reloadOpenWorktreeDiff]);

    useEffect(() => {
        const wholeFile = isWholeFileMode(diffMode);
        diffWholeFileRef.current = wholeFile;
        const open = openDiffRequestRef.current;
        if (!open || open.sequence !== diffLoadSequence.current) return;
        if (open.request.whole_file === wholeFile) return;
        void loadDiff({ ...open.request, whole_file: wholeFile }, true);
    }, [diffMode, loadDiff]);

    const openCommitFile = useCallback((file: ChangedFile) => {
        if (!selectedOid) return;
        setSelectedWorktreeFile(null);
        void loadDiff({
            target: { kind: "commit", oid: selectedOid, parent_index: 0 },
            path: file.new_path,
            context_lines: diffContextLines,
            ignore_whitespace: false,
            max_bytes: diffMaxBytes,
            whole_file: diffWholeFileRef.current,
        });
    }, [loadDiff, diffContextLines, diffMaxBytes, selectedOid]);

    const openWorktreeFileDiff = useCallback((path: string, staged: boolean) => {
        setSelectedWorktreeFile({ path, staged });
        void loadDiff({
            target: { kind: staged ? "staged" : "worktree" },
            path,
            context_lines: diffContextLines,
            ignore_whitespace: false,
            max_bytes: diffMaxBytes,
            whole_file: diffWholeFileRef.current,
        });
    }, [loadDiff, diffContextLines, diffMaxBytes]);

    const openWorktreeDiff = useCallback((entry: StatusEntry, staged: boolean) => {
        openWorktreeFileDiff(entry.path, staged);
    }, [openWorktreeFileDiff]);

    const swapWorktreeDiffSide = useCallback((path: string, staged: boolean) => {
        const open = openDiffRequestRef.current;
        if (!open || open.sequence !== diffLoadSequence.current || open.request.path !== path) return;
        const kind = open.request.target.kind;
        if (kind !== "worktree" && kind !== "staged") return;
        setSelectedWorktreeFile({ path, staged });
        void loadDiff({ ...open.request, target: { kind: staged ? "staged" : "worktree" } }, true);
    }, [loadDiff]);

    useEffect(() => {
        openWorktreeDiffRef.current = openWorktreeFileDiff;
        swapWorktreeDiffSideRef.current = swapWorktreeDiffSide;
    }, [openWorktreeFileDiff, swapWorktreeDiffSide]);

    return {
        closeDiff,
        loadDiff,
        openCommitFile,
        openWorktreeDiff,
        openWorktreeFileDiff,
        reloadOpenWorktreeDiff,
        swapWorktreeDiffSide,
    };
}
