import { useCallback, useMemo, useState } from "react";

import type { DiffLineActions } from "../components/diff";
import { gitcatApi } from "../lib/api";
import type { DiffLine, FileDiff, LinePatchAction, RepositorySnapshot } from "../lib/types";
import type { RunMutation } from "./state";

const TITLES: Record<LinePatchAction, string> = {
    stage: "Line staged",
    unstage: "Line unstaged",
    discard: "Line discarded",
};

export interface DiffLineActionsParams {
    diff: FileDiff | null;
    reloadOpenWorktreeDiff: () => Promise<void> | undefined;
    runMutation: RunMutation;
    selectedWorktreeFile: { path: string; staged: boolean } | null;
    snapshot: RepositorySnapshot | null;
}

/**
 * Line actions for the open worktree diff, or null where there are none: a
 * commit diff, or a conflicted file whose diff is drawn against "ours".
 *
 * Staging a line moves the line numbers of everything after it, so the
 * actions stay busy until the diff has been read again; a second click on
 * the stale view would otherwise name lines by numbers that have moved.
 */
export function useDiffLineActions({
    diff,
    reloadOpenWorktreeDiff,
    runMutation,
    selectedWorktreeFile,
    snapshot,
}: DiffLineActionsParams): DiffLineActions | null {
    const [busy, setBusy] = useState(false);
    const path = selectedWorktreeFile?.path;

    const onApply = useCallback((action: LinePatchAction, lines: DiffLine[]) => {
        if (!path || !lines.length) return;
        setBusy(true);
        void runMutation(
            TITLES[action],
            (repository) => gitcatApi.applyDiffLines(repository.repository_id, { path, action, lines }),
            { silent: true },
        )
            .then((done) => (done ? reloadOpenWorktreeDiff() : undefined))
            .finally(() => setBusy(false));
    }, [path, reloadOpenWorktreeDiff, runMutation]);

    const conflicted = Boolean(
        path && snapshot?.status.entries.find((entry) => entry.path === path)?.conflicted,
    );
    const side = selectedWorktreeFile?.staged ? "staged" : "unstaged";
    const available = Boolean(path && diff?.new_path === path && !conflicted);

    return useMemo(
        () => (available ? { side, busy, onApply } : null),
        [available, busy, onApply, side],
    );
}
