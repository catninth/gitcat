import { useCallback, type Dispatch, type RefObject, type SetStateAction } from "react";

import { revealCommitRow } from "../components/CommitGraph";
import type { CommitActionAvailability, CommitSummary, FileDiff, HistoryPage } from "../lib/types";
import type { CenterView, CommitDetailsPanel } from "./state";

export interface CommitSelectionParams {
    diffLoadSequence: RefObject<number>;
    history: HistoryPage | null;
    selectedOidRef: RefObject<string | null>;
    setCenterView: Dispatch<SetStateAction<CenterView>>;
    setCommitActions: Dispatch<SetStateAction<CommitActionAvailability[]>>;
    setDetails: Dispatch<SetStateAction<CommitDetailsPanel | null>>;
    setDiff: Dispatch<SetStateAction<FileDiff | null>>;
    setDiffLoading: Dispatch<SetStateAction<boolean>>;
    setRightPanelVisible: Dispatch<SetStateAction<boolean>>;
    setSelectedOid: Dispatch<SetStateAction<string | null>>;
    setSelectedPath: Dispatch<SetStateAction<string | undefined>>;
    setSelectedWorktreeFile: Dispatch<SetStateAction<{ path: string; staged: boolean } | null>>;
    setWipSelected: Dispatch<SetStateAction<boolean>>;
    wipRowRef: RefObject<HTMLDivElement | null>;
    wipSelected: boolean;
}

export function useCommitSelection({
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
}: CommitSelectionParams) {
    const selectCommitOid = useCallback((oid: string) => {
        if (!wipSelected && selectedOidRef.current === oid) return;

        ++diffLoadSequence.current;
        setSelectedOid(oid);
        setWipSelected(false);
        setDetails(null);
        setCommitActions([]);
        setSelectedPath(undefined);
        setSelectedWorktreeFile(null);
        setDiff(null);
        setDiffLoading(false);
        setCenterView("graph");
    }, [wipSelected]);

    const selectCommit = useCallback((commit: CommitSummary) => {
        selectCommitOid(commit.oid);
    }, [selectCommitOid]);

    const jumpToCommit = useCallback((oid: string) => {
        selectCommitOid(oid);
        requestAnimationFrame(() => {
            revealCommitRow(oid);
        });
    }, [selectCommitOid]);

    const selectWip = useCallback(() => {
        ++diffLoadSequence.current;
        setWipSelected(true);
        setSelectedOid(null);
        setDetails(null);
        setCommitActions([]);
        setSelectedPath(undefined);
        setSelectedWorktreeFile(null);
        setDiff(null);
        setDiffLoading(false);
        setCenterView("graph");
    }, []);

    const focusWorktree = useCallback(() => {
        selectWip();
        setRightPanelVisible(true);
    }, [selectWip]);

    const selectFirstCommitFromWip = useCallback(() => {
        const firstCommit = history?.commits[0];
        if (!firstCommit) return;

        document.querySelector<HTMLElement>("[data-commit-list]")?.focus({ preventScroll: true });
        selectCommit(firstCommit);
    }, [history, selectCommit]);

    const selectWipFromGraph = useCallback(() => {
        selectWip();
        wipRowRef.current?.focus();
    }, [selectWip]);

    return {
        focusWorktree,
        jumpToCommit,
        selectCommit,
        selectCommitOid,
        selectFirstCommitFromWip,
        selectWip,
        selectWipFromGraph,
    };
}
