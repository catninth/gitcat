import { useCallback, useEffect, useRef, useState } from "react";

import { gitcatApi } from "../lib/api";
import type { RepositorySnapshot, SubmoduleInfo, SubmoduleSyncAction } from "../lib/types";
import type { RunMutation, RuntimeRepository } from "./state";
import { runSubmoduleBatch, submoduleErrorMessage, type SubmoduleBatchItem } from "./submoduleBatch";

type SubmoduleDialogState = { kind: "add" } | { kind: "edit" | "delete"; submodule: SubmoduleInfo } | { kind: "batch" } | null;

export function useSubmodules({
    repository, snapshot, busy, runMutation, openRepositoryPath,
}: {
    repository: RuntimeRepository | undefined;
    snapshot: RepositorySnapshot | null;
    busy: boolean;
    runMutation: RunMutation;
    openRepositoryPath: (path: string) => Promise<void>;
}) {
    const repositoryId = repository?.repository_id;
    const activeId = useRef(repositoryId);
    activeId.current = repositoryId;
    const request = useRef(0);
    const operation = useRef(false);
    const [data, setData] = useState<{ repositoryId: string; modules: SubmoduleInfo[] } | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [dialog, setDialog] = useState<SubmoduleDialogState>(null);
    const [dialogError, setDialogError] = useState<string | null>(null);
    const [working, setWorking] = useState(false);
    const [batchAction, setBatchAction] = useState<SubmoduleSyncAction | null>(null);
    const [batchResults, setBatchResults] = useState<SubmoduleBatchItem[]>([]);
    const [batchRunning, setBatchRunning] = useState(false);
    const loaded = data !== null && data.repositoryId === repositoryId;
    const modules = loaded ? data.modules : [];

    const refresh = useCallback(async () => {
        if (!repositoryId) return;
        const sequence = ++request.current;
        setLoading(true);
        try {
            const list = await gitcatApi.listSubmodules(repositoryId);
            if (activeId.current !== repositoryId || request.current !== sequence) return;
            setData({ repositoryId, modules: list });
            setError(null);
        } catch (cause) {
            if (activeId.current === repositoryId && request.current === sequence) setError(submoduleErrorMessage(cause));
        } finally {
            if (activeId.current === repositoryId && request.current === sequence) setLoading(false);
        }
    }, [repositoryId]);

    useEffect(() => {
        ++request.current;
        setError(null);
        setDialog(null);
        setDialogError(null);
        setBatchAction(null);
        setBatchResults([]);
        setLoading(false);
    }, [repositoryId]);

    // Every overview refresh includes external filesystem changes and returning
    // from a child tab. Reads stay independent of the commit graph's load path.
    useEffect(() => {
        if (repositoryId && !busy) void refresh();
    }, [repositoryId, snapshot, busy, refresh]);

    const showDialog = (next: SubmoduleDialogState) => {
        if (operation.current) return;
        setDialogError(null);
        setDialog(next);
        if (next?.kind === "batch") { setBatchAction(null); setBatchResults([]); }
    };
    const closeDialog = () => { if (!operation.current) { setDialog(null); setDialogError(null); } };

    const mutate = async (title: string, action: (id: string) => Promise<unknown>, closeOnSuccess = false) => {
        if (!repositoryId || operation.current) return;
        const id = repositoryId;
        operation.current = true;
        setWorking(true);
        setDialogError(null);
        try {
            const success = await runMutation(title, async (target) => {
                try { return await action(target.repository_id); }
                finally { if (activeId.current === id) await refresh(); }
            }, { queueKey: `submodule:${id}`, onError: (cause) => {
                if (activeId.current === id) setDialogError(submoduleErrorMessage(cause));
                // A context-menu operation has no dialog to display its error.
                return dialog !== null && activeId.current === id;
            } });
            if (success && closeOnSuccess && activeId.current === id) setDialog(null);
        } finally { operation.current = false; setWorking(false); }
    };

    const open = (module: SubmoduleInfo) => {
        if (busy || working || !module.initialized) return;
        closeDialog();
        void openRepositoryPath(module.absolute_path);
    };
    const update = (module: SubmoduleInfo) => void mutate(
        module.initialized ? `Updated ${module.path}` : `Initialized ${module.path}`,
        (id) => gitcatApi.updateSubmodule(id, module.path),
    );
    const submit = ({ url, path }: { url: string; path: string }) => {
        if (dialog?.kind === "edit") {
            void mutate("Submodule URL updated", (id) => gitcatApi.setSubmoduleUrl(id, dialog.submodule.path, url), true);
        } else if (dialog?.kind === "add") {
            void mutate(`Added ${path}`, (id) => gitcatApi.addSubmodule(id, url, path), true);
        }
    };
    const remove = () => {
        if (dialog?.kind !== "delete") return;
        void mutate(`Deleted ${dialog.submodule.path}`, (id) => gitcatApi.removeSubmodule(id, dialog.submodule.path, true), true);
    };
    const runBatch = async (action: SubmoduleSyncAction, includeParent: boolean) => {
        if (!repository || operation.current) return;
        const id = repository.repository_id;
        operation.current = true;
        setWorking(true);
        setBatchRunning(true);
        setBatchAction(action);
        setBatchResults([]);
        setDialogError(null);
        try {
            await runMutation("Repository sync finished", async () => {
                try {
                    await runSubmoduleBatch({
                        api: gitcatApi, repositoryId: id, repositoryName: repository.info.name,
                        action, includeParent,
                        onProgress: (items) => { if (activeId.current === id) setBatchResults(items); },
                    });
                } finally { if (activeId.current === id) await refresh(); }
            }, { silent: true, queueKey: `submodule-batch:${id}`, onError: (cause) => {
                if (activeId.current === id) setDialogError(submoduleErrorMessage(cause));
                return activeId.current === id;
            } });
        } finally { operation.current = false; setWorking(false); setBatchRunning(false); }
    };

    const selected = dialog && "submodule" in dialog
        ? modules.find((module) => module.path === dialog.submodule.path) ?? dialog.submodule
        : null;
    return {
        modules, error,
        // Every overview refresh reloads the list; only the first load for a
        // repository is worth announcing, or an empty list flickers each time.
        loading: loading && !loaded, dialog, dialogError, working, selected,
        batchAction, batchResults, batchRunning, refresh, closeDialog, submit, remove, runBatch, open, update,
        add: () => showDialog({ kind: "add" }),
        edit: (submodule: SubmoduleInfo) => showDialog({ kind: "edit", submodule }),
        requestDelete: (submodule: SubmoduleInfo) => showDialog({ kind: "delete", submodule }),
        manage: () => showDialog({ kind: "batch" }),
    };
}
