import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type RefObject, type SetStateAction, } from "react";

import type { ToastMessage } from "../components/ToastRegion";
import type { ConflictIndicator } from "../components/toolbar";
import { getApiError, gitcatApi } from "../lib/api";
import { conflictOperationLabel } from "../lib/conflicts";
import type { ConflictPreflightResult, PersistedState, RepositorySnapshot, RepositoryTab } from "../lib/types";
import { defaultConflictPreflightTarget } from "./snapshot";
import type { CenterView, RuntimeRepository } from "./state";

export interface ConflictPreflightParams {
    activeConflictCount: number;
    activeRepository: RuntimeRepository | undefined;
    activeRepositoryIdRef: RefObject<string | null>;
    activeTab: RepositoryTab | undefined;
    activeTabId: string | null;
    addToast: (toast: Omit<ToastMessage, "id">) => void;
    setCenterView: Dispatch<SetStateAction<CenterView>>;
    setPersisted: Dispatch<SetStateAction<PersistedState>>;
    setRightPanelVisible: Dispatch<SetStateAction<boolean>>;
    setSelectedOid: Dispatch<SetStateAction<string | null>>;
    setWipSelected: Dispatch<SetStateAction<boolean>>;
    snapshot: RepositorySnapshot | null;
}

export function useConflictPreflight({
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
}: ConflictPreflightParams) {
    const [conflictPreflight, setConflictPreflight] = useState<ConflictPreflightResult | null>(null);
    const [conflictPreflightLoading, setConflictPreflightLoading] = useState(false);
    const conflictPreflightSequence = useRef(0);

    const conflictTarget = activeTab?.conflict_target_disabled
        ? null
        : activeTab?.conflict_target ?? defaultConflictPreflightTarget(snapshot);
    const conflictTargets = useMemo(() => {
        if (!snapshot) return conflictTarget ? [conflictTarget] : [];
        const candidates = [
            ...snapshot.local_branches.filter((branch) => !branch.is_head).map((branch) => branch.name),
            ...snapshot.remote_branches.map((branch) => branch.name),
        ];
        if (conflictTarget) candidates.push(conflictTarget);
        return [...new Set(candidates)].sort((left, right) => left.localeCompare(right));
    }, [conflictTarget, snapshot]);
    const conflictHeadOid = snapshot?.head.kind === "unborn" ? null : snapshot?.head.oid ?? null;
    const conflictTargetOid = snapshot
        ? [...snapshot.local_branches, ...snapshot.remote_branches].find((branch) => branch.name === conflictTarget)?.oid ?? null
        : null;

    const selectConflictTarget = useCallback((target: string | null) => {
        if (!activeTabId) return;
        setPersisted((current) => ({
            ...current,
            workspace: {
                ...current.workspace,
                ungrouped_tabs: current.workspace.ungrouped_tabs.map((tab) => (
                    tab.id === activeTabId
                        ? { ...tab, conflict_target: target, conflict_target_disabled: target === null }
                        : tab
                )),
            },
        }));
    }, [activeTabId]);

    useEffect(() => {
        const sequence = ++conflictPreflightSequence.current;
        if (!activeRepository || activeConflictCount || !conflictTarget) {
            setConflictPreflight(null);
            setConflictPreflightLoading(false);
            return;
        }

        const repositoryId = activeRepository.repository_id;
        setConflictPreflight(null);
        setConflictPreflightLoading(true);
        void gitcatApi.conflictPreflight(repositoryId, conflictTarget)
            .then((result) => {
                if (
                    sequence !== conflictPreflightSequence.current
                    || activeRepositoryIdRef.current !== repositoryId
                ) return;
                setConflictPreflight(result);
            })
            .catch((error) => {
                if (
                    sequence !== conflictPreflightSequence.current
                    || activeRepositoryIdRef.current !== repositoryId
                ) return;
                const apiError = getApiError(error);
                setConflictPreflight({
                    target: conflictTarget,
                    target_oid: "",
                    state: "unavailable",
                    conflicting_paths: [],
                    unavailable_reason: apiError.details ?? apiError.message,
                });
            })
            .finally(() => {
                if (sequence === conflictPreflightSequence.current) setConflictPreflightLoading(false);
            });
    }, [activeConflictCount, activeRepository, conflictHeadOid, conflictTarget, conflictTargetOid]);

    const conflictIndicator: ConflictIndicator = activeConflictCount
        ? {
            state: "active",
            count: activeConflictCount,
            label: `${activeConflictCount} unresolved ${conflictOperationLabel(snapshot?.operation_state ?? "normal")} conflict${activeConflictCount === 1 ? "" : "s"}`,
        }
        : conflictPreflightLoading
            ? { state: "checking", label: `Checking conflicts against ${conflictTarget ?? "upstream"}` }
            : conflictPreflight?.state === "clean"
                ? { state: "clean", label: `No conflicts detected against ${conflictPreflight.target}` }
                : conflictPreflight?.state === "conflicting"
                    ? {
                        state: "conflicting",
                        count: conflictPreflight.conflicting_paths.length,
                        label: `${conflictPreflight.conflicting_paths.length} potential conflict${conflictPreflight.conflicting_paths.length === 1 ? "" : "s"} against ${conflictPreflight.target}`,
                    }
                    : {
                        state: "unavailable",
                        label: conflictPreflight?.unavailable_reason
                            ?? (conflictTarget ? `Conflict check unavailable for ${conflictTarget}` : "Choose a comparison target to enable conflict checks"),
                    };

    const showConflictIndicator = useCallback(() => {
        if (activeConflictCount) {
            setSelectedOid(null);
            setWipSelected(true);
            setRightPanelVisible(true);
            setCenterView("graph");
            addToast({ tone: "info", title: conflictIndicator.label, detail: "Resolve each file in the Working tree panel." });
            return;
        }
        if (conflictPreflight?.state === "conflicting") {
            const preview = conflictPreflight.conflicting_paths.slice(0, 4).join(", ");
            const remainder = conflictPreflight.conflicting_paths.length - 4;
            addToast({
                tone: "info",
                title: conflictIndicator.label,
                detail: `${preview}${remainder > 0 ? `, and ${remainder} more` : ""}`,
            });
            return;
        }
        addToast({ tone: conflictPreflight?.state === "clean" ? "success" : "info", title: conflictIndicator.label });
    }, [activeConflictCount, addToast, conflictIndicator.label, conflictPreflight]);

    return {
        conflictIndicator,
        conflictTarget,
        conflictTargets,
        selectConflictTarget,
        showConflictIndicator,
    };
}
