import { useCallback, type Dispatch, type SetStateAction } from "react";

import type { ToastMessage } from "../components/ToastRegion";
import { gitcatApi } from "../lib/api";
import type { PullMode, PullPhase, RepositorySnapshot, StashEntry } from "../lib/types";
import { conflictToasts, continueConflictMutation, operationTitle } from "../lib/conflicts";
import { currentBranch } from "./branches";
import { continuableOperation } from "./snapshot";
import type { PromptState, RunMutation } from "./state";

export interface RepositoryCommandsParams {
    addToast: (toast: Omit<ToastMessage, "id">) => void;
    autoPrune: boolean;
    defaultPullMode: PullMode;
    /** The step a running pull has reached; `null` once it has finished. */
    onPullPhase: (phase: PullPhase | null) => void;
    /** Asked instead of pushing when the repository has no remote at all. */
    requestRemote: () => void;
    runMutation: RunMutation;
    selectWip: () => void;
    setPrompt: Dispatch<SetStateAction<PromptState>>;
    setRightPanelVisible: Dispatch<SetStateAction<boolean>>;
    snapshot: RepositorySnapshot | null;
    stashes: StashEntry[];
}

export function useRepositoryCommands({
    addToast,
    autoPrune,
    defaultPullMode,
    onPullPhase,
    requestRemote,
    runMutation,
    selectWip,
    setPrompt,
    setRightPanelVisible,
    snapshot,
    stashes,
}: RepositoryCommandsParams) {
    const fetchActiveRepository = useCallback(() => {
        void runMutation("Fetch complete", (repository) => gitcatApi.fetch(repository.repository_id, {
            remote: null,
            prune: autoPrune,
            tags: false,
        }), { queueKey: "fetch" });
    }, [autoPrune, runMutation]);

    const pullActiveRepository = useCallback((mode: PullMode = defaultPullMode) => {
        // A pull that stops while putting the local changes back has two
        // failures to report: the stash that did not apply, and the pull.
        let restoring = false;
        const reportPhase = (phase: PullPhase | null) => {
            if (phase) restoring = phase.phase === "restoring";
            onPullPhase(phase);
        };
        void runMutation("Pull complete", (repository) => gitcatApi.pull(repository.repository_id, {
            remote: null,
            branch: null,
            mode,
            prune: autoPrune,
            autostash: true,
        }, reportPhase).finally(() => onPullPhase(null)), {
            queueKey: "pull",
            conflictToasts: () => conflictToasts(restoring ? "pull_autostash" : "pull"),
        });
    }, [autoPrune, defaultPullMode, onPullPhase, runMutation]);

    // A branch with no upstream is the one case where the push has to say so,
    // which is what the failure's own recovery action asks for. The options
    // object matters: this is wired straight to a click handler elsewhere, and
    // a bare boolean parameter would read the event itself as "set upstream".
    const pushActiveRepository = useCallback((options?: { setUpstream?: boolean }) => {
        // Git would only answer that there is no destination; offering to add
        // one is the step that answer leaves the user to find.
        if (snapshot && snapshot.remotes.length === 0) {
            requestRemote();
            return;
        }
        void runMutation("Push complete", (repository) => gitcatApi.push(repository.repository_id, {
            remote: null,
            branch: null,
            set_upstream: options?.setUpstream === true,
        }), { queueKey: `push:${options?.setUpstream === true}` });
    }, [requestRemote, runMutation, snapshot]);

    const createBranchAtHead = useCallback(() => {
        if (!snapshot) return;
        const oid = snapshot.head.kind === "unborn" ? null : snapshot.head.oid;
        if (oid) setPrompt({ kind: "create_branch", startOid: oid });
        else addToast({ tone: "info", title: "Create the first commit before branching" });
    }, [addToast, snapshot]);

    const stashActiveRepository = useCallback(() => {
        void runMutation("Changes stashed", (repository) => gitcatApi.stashPush(repository.repository_id, null, true), { queueKey: "stash" });
    }, [runMutation]);

    const popLatestStash = useCallback(() => {
        if (!stashes.length) return;
        void runMutation(
            "Stash popped",
            (repository) => gitcatApi.stashApply(repository.repository_id, stashes[0].oid, true),
            {
                queueKey: `stash-pop:${stashes[0].oid}`,
                wipTitleHint: stashes[0].message,
                conflictToasts: conflictToasts("stash_apply"),
            },
        );
    }, [runMutation, stashes]);

    const continueActiveOperation = useCallback((message: string | null = null): Promise<boolean> => {
        const operation = snapshot ? continuableOperation(snapshot.operation_state) : null;
        if (!operation || !snapshot) return Promise.resolve(false);
        const source = snapshot.operation_source;
        const title = operation === "merge" && source
            ? `Merged – ${source.incoming} into ${currentBranch(snapshot)}`
            : `${operationTitle(operation)} continued`;
        return runMutation(
            title,
            (repository) => gitcatApi.continueOperation(repository.repository_id, operation, message),
            { queueKey: `continue:${operation}`, conflictToasts: conflictToasts(continueConflictMutation(operation)) },
        );
    }, [runMutation, snapshot]);

    // GitKraken aborts without asking. That is right while nothing has been
    // resolved yet -- the abort loses nothing -- but once files are resolved
    // the abort throws that work away, so it is confirmed then.
    const abortActiveOperation = useCallback(() => {
        const operation = snapshot ? continuableOperation(snapshot.operation_state) : null;
        if (!operation || !snapshot) return;
        const resolved = snapshot.status.entries.filter((entry) => entry.index && !entry.conflicted).length;
        const label = operationTitle(operation).toLowerCase();
        if (resolved > 0 && !window.confirm(
            `Abort the ${label}? ${resolved} resolved file${resolved === 1 ? "" : "s"} will be reset.`,
        )) return;
        void runMutation(`${operationTitle(operation)} aborted`, (repository) => gitcatApi.abortOperation(repository.repository_id, operation), { queueKey: `abort:${operation}` });
    }, [runMutation, snapshot]);

    const skipActiveOperation = useCallback(() => {
        const operation = snapshot ? continuableOperation(snapshot.operation_state) : null;
        if (!operation || operation === "merge") return;
        const label = operation.replace("_", "-");
        if (!window.confirm(`Skip the commit currently being applied by this ${label} and drop its changes?`)) return;
        void runMutation("Commit skipped", (repository) => gitcatApi.skipOperation(repository.repository_id, operation), {
            queueKey: `skip:${operation}`,
            conflictToasts: conflictToasts(continueConflictMutation(operation)),
        });
    }, [runMutation, snapshot]);

    const autoResolveActiveConflicts = useCallback(() => {
        if (!snapshot?.status.entries.some((entry) => entry.conflicted)) return;
        void runMutation("Recorded conflict resolutions applied", (repository) => gitcatApi.autoResolveConflicts(repository.repository_id), { queueKey: "auto-resolve" });
    }, [runMutation, snapshot]);

    const focusCommitMessage = useCallback(() => {
        if (!snapshot) return;
        selectWip();
        setRightPanelVisible(true);
        requestAnimationFrame(() => document.getElementById("commit-message")?.focus());
    }, [selectWip, snapshot]);

    return {
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
    };
}
