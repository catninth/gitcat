import { useCallback, useEffect, useState, type RefObject } from "react";

import { gitcatApi } from "../lib/api";
import type { RepositorySnapshot } from "../lib/types";
import type { RuntimeRepository } from "./state";

export interface AutoRefreshParams {
    activeRepository: RuntimeRepository | undefined;
    activeRepositoryIdRef: RefObject<string | null>;
    autoFetchIntervalMinutes: number;
    autoFetchRef: RefObject<() => void>;
    autoPrune: boolean;
    autoRefreshRef: RefObject<() => void>;
    autoReloadDiffRef: RefObject<() => void>;
    busy: boolean;
    lastAutoFetchRef: RefObject<Map<string, number>>;
    loadOverview: (repository: RuntimeRepository, preserveSelection?: boolean, showLoading?: boolean) => Promise<void>;
    overviewLoading: boolean;
    overviewRepositoryId: string | null;
    reloadOpenWorktreeDiff: () => void;
    showError: (title: string, error: unknown) => void;
    snapshot: RepositorySnapshot | null;
}

export function useAutoRefresh({
    activeRepository,
    activeRepositoryIdRef,
    autoFetchIntervalMinutes,
    autoFetchRef,
    autoPrune,
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
}: AutoRefreshParams) {
    const refreshActiveRepository = useCallback(() => {
        if (!activeRepository || busy || overviewLoading) return;
        reloadOpenWorktreeDiff();
        void loadOverview(activeRepository, true)
            .catch((error) => showError("Refresh failed", error));
    }, [activeRepository, busy, loadOverview, overviewLoading, reloadOpenWorktreeDiff, showError]);

    const backgroundRefreshActiveRepository = useCallback(() => {
        if (!activeRepository || busy || overviewLoading) return;
        void loadOverview(activeRepository, true, false)
            .catch((error) => showError("Refresh failed", error));
    }, [activeRepository, busy, loadOverview, overviewLoading, showError]);

    // Keep the latest refresh behind a ref so the long-lived filesystem-change
    // listener below always calls the current one without re-subscribing.
    useEffect(() => {
        autoRefreshRef.current = backgroundRefreshActiveRepository;
    }, [backgroundRefreshActiveRepository]);

    // The repository whose background fetch is still out. An empty repository
    // reads this: until the remote has answered, "no commits" is only what
    // this clone knows, not what the remote has.
    const [fetchingRepositoryId, setFetchingRepositoryId] = useState<string | null>(null);

    const autoFetchActiveRepository = useCallback(() => {
        if (!activeRepository || busy || overviewLoading) return;
        if (!snapshot?.remotes.length) return;
        const repository = activeRepository;
        lastAutoFetchRef.current.set(repository.repository_id, Date.now());
        setFetchingRepositoryId(repository.repository_id);
        void gitcatApi.fetch(repository.repository_id, {
            remote: null,
            prune: autoPrune,
            tags: false,
        })
            .then(() => {
                if (activeRepositoryIdRef.current !== repository.repository_id) return;
                return loadOverview(repository, true, false);
            })
            .catch(() => undefined)
            .finally(() => setFetchingRepositoryId((current) => (
                current === repository.repository_id ? null : current
            )));
    }, [
        activeRepository,
        busy,
        loadOverview,
        overviewLoading,
        autoPrune,
        snapshot?.remotes.length,
    ]);

    useEffect(() => {
        autoFetchRef.current = autoFetchActiveRepository;
    }, [autoFetchActiveRepository]);

    useEffect(() => {
        const minutes = autoFetchIntervalMinutes;
        if (!minutes) return;
        const timer = window.setInterval(() => autoFetchRef.current(), minutes * 60_000);
        return () => window.clearInterval(timer);
    }, [autoFetchIntervalMinutes]);

    useEffect(() => {
        const minutes = autoFetchIntervalMinutes;
        if (!minutes || !overviewRepositoryId) return;
        const lastFetchedAt = lastAutoFetchRef.current.get(overviewRepositoryId) ?? 0;
        if (Date.now() - lastFetchedAt < minutes * 60_000) return;
        autoFetchRef.current();
    }, [overviewRepositoryId, autoFetchIntervalMinutes]);

    useEffect(() => {
        if (gitcatApi.runtime !== "tauri") return;
        const repositoryId = activeRepository?.repository_id;
        if (repositoryId) void gitcatApi.watchRepository(repositoryId).catch(() => undefined);
        else void gitcatApi.unwatchRepository().catch(() => undefined);
    }, [activeRepository?.repository_id]);

    useEffect(() => {
        if (gitcatApi.runtime !== "tauri") return;
        let unlisten: (() => void) | undefined;
        let disposed = false;
        void (async () => {
            const { listen } = await import("@tauri-apps/api/event");
            const stop = await listen<{ repository_id: string }>(
                "repository:changed",
                (event) => {
                    if (event.payload.repository_id === activeRepositoryIdRef.current) {
                        autoRefreshRef.current();
                        autoReloadDiffRef.current();
                    }
                },
            );
            if (disposed) stop();
            else unlisten = stop;
        })();
        return () => {
            disposed = true;
            unlisten?.();
        };
    }, []);

    return { fetchingRepositoryId, refreshActiveRepository };
}
