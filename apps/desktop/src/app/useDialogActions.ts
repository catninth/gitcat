import { useCallback, useMemo, useRef, type Dispatch, type SetStateAction } from "react";

import { gitcatApi } from "../lib/api";
import type { PersistedState, RepositorySnapshot } from "../lib/types";
import { expectedState, isNotFullyMerged } from "./snapshot";
import type { ConfirmState, PromptState, RunMutation } from "./state";
import { makeId } from "./workspace";

export interface DialogActionsParams {
    confirmRequest: ConfirmState;
    openAddRemote: () => void;
    prompt: PromptState;
    runMutation: RunMutation;
    setConfirmRequest: Dispatch<SetStateAction<ConfirmState>>;
    setPersisted: Dispatch<SetStateAction<PersistedState>>;
    setPrompt: Dispatch<SetStateAction<PromptState>>;
    snapshot: RepositorySnapshot | null;
}

export function useDialogActions({
    confirmRequest,
    openAddRemote,
    prompt,
    runMutation,
    setConfirmRequest,
    setPersisted,
    setPrompt,
    snapshot,
}: DialogActionsParams) {
    // What the last force push aimed at, so the failure's own "overwrite"
    // action knows what it is overwriting.
    const lastForcePush = useRef<{ remote: string; branch: string } | null>(null);

    const submitPrompt = useCallback((value: string, secondaryValue?: string) => {
        if (!prompt) return;
        const currentPrompt = prompt;
        setPrompt(null);
        switch (currentPrompt.kind) {
            case "alias_tab":
                setPersisted((current) => ({
                    ...current,
                    workspace: {
                        ...current.workspace,
                        ungrouped_tabs: current.workspace.ungrouped_tabs.map((tab) => tab.id === currentPrompt.tabId ? { ...tab, display_name: value } : tab),
                        groups: current.workspace.groups.map((group) => ({
                            ...group,
                            tabs: group.tabs.map((tab) => tab.id === currentPrompt.tabId ? { ...tab, display_name: value } : tab),
                        })),
                    },
                }));
                break;
            case "create_branch":
                void runMutation("Branch created", (repository) => gitcatApi.createBranch(repository.repository_id, value, currentPrompt.startOid, true));
                break;
            case "rename_branch":
                void runMutation("Branch renamed", (repository) => gitcatApi.renameBranch(repository.repository_id, currentPrompt.branch.name, value));
                break;
            case "create_tag": {
                const message = currentPrompt.annotated ? (secondaryValue?.trim() || value) : null;
                void runMutation("Tag created", (repository) => gitcatApi.createTag(repository.repository_id, value, currentPrompt.oid, message));
                break;
            }
            case "set_upstream":
                void runMutation("Upstream updated", (repository) => gitcatApi.setUpstream(repository.repository_id, currentPrompt.branch.name, value));
                break;
        }
    }, [prompt, runMutation]);

    const promptConfig = useMemo(() => {
        if (!prompt) return null;
        switch (prompt.kind) {
            case "alias_tab": return { title: "Rename repository tab", label: "Tab name", initialValue: prompt.current, confirmLabel: "Rename" };
            case "create_branch": return { title: "Create branch", label: "Branch name", placeholder: "feature/short-name", confirmLabel: "Create and checkout" };
            case "rename_branch": return { title: "Rename branch", label: "New branch name", initialValue: prompt.branch.name, confirmLabel: "Rename" };
            case "create_tag": return prompt.annotated
                ? {
                    title: "Create annotated tag",
                    label: "Tag name",
                    placeholder: "v1.0.0",
                    secondaryLabel: "Tag message",
                    secondaryPlaceholder: "Release 1.0.0",
                    secondaryRequired: true,
                    confirmLabel: "Create tag",
                }
                : { title: "Create tag", label: "Tag name", placeholder: "v1.0.0", confirmLabel: "Create tag" };
            case "set_upstream": return {
                title: "Set upstream",
                label: "Upstream branch",
                placeholder: "origin/main",
                initialValue: prompt.branch.upstream ?? "",
                confirmLabel: "Set upstream",
            };
        }
    }, [prompt]);

    const submitConfirm = useCallback(() => {
        if (!confirmRequest || !snapshot) return;
        const request = confirmRequest;
        setConfirmRequest(null);
        switch (request.kind) {
            case "delete_branch":
                void runMutation("Branch deleted", (repository) => gitcatApi.deleteBranch(
                    repository.repository_id,
                    request.name,
                    request.force,
                    true,
                    expectedState(snapshot),
                ), {
                    onError: (error) => {
                        if (request.force || !isNotFullyMerged(error)) return false;
                        setConfirmRequest({ kind: "delete_branch", name: request.name, force: true });
                        return true;
                    },
                });
                break;
            case "delete_stash":
                void runMutation("Stash deleted", (repository) => gitcatApi.stashDrop(
                    repository.repository_id,
                    request.oid,
                    true,
                    expectedState(snapshot),
                ));
                break;
            case "force_push":
                lastForcePush.current = request;
                void runMutation("Force push complete", (repository) => gitcatApi.push(repository.repository_id, {
                    remote: request.remote,
                    branch: request.branch,
                    set_upstream: false,
                    force: request.ignoreRemote ? "force" : "with_lease",
                }), { queueKey: `push:${request.remote}:${request.branch}` });
                break;
            case "add_remote_for_push":
                openAddRemote();
                break;
        }
    }, [confirmRequest, openAddRemote, runMutation, snapshot]);

    /**
     * The second half of a force push whose lease was refused.
     *
     * The failure itself offers this, so it starts from the push that was just
     * attempted rather than from a menu. It asks again before overwriting: the
     * first confirmation was for a lease that would have stopped exactly this.
     */
    const escalateForcePush = useCallback(() => {
        const previous = lastForcePush.current;
        if (previous) setConfirmRequest({ kind: "force_push", ...previous, ignoreRemote: true });
    }, [setConfirmRequest]);

    const confirmConfig = useMemo(() => {
        if (!confirmRequest) return null;
        switch (confirmRequest.kind) {
            case "delete_branch": return confirmRequest.force
                ? {
                    message: `"${confirmRequest.name}" is not fully merged. Force deleting it discards commits that exist only on this branch.`,
                    confirmLabel: "Force delete",
                }
                : {
                    message: `This is a destructive operation, are you sure you want to delete "${confirmRequest.name}"?`,
                    confirmLabel: "Delete",
                };
            case "delete_stash": {
                const label = confirmRequest.message.trim();
                return {
                    message: label
                        ? `This is a destructive operation, are you sure you want to delete stash with message "${label}"?`
                        : `This is a destructive operation, are you sure you want to delete ${confirmRequest.selector}?`,
                    confirmLabel: "Delete",
                };
            }
            case "force_push": {
                const target = `${confirmRequest.remote}/${confirmRequest.branch}`;
                return confirmRequest.ignoreRemote
                    ? {
                        message: `${target} has commits this branch has never seen. Pushing anyway discards them for everyone.`,
                        confirmLabel: "Overwrite the remote",
                    }
                    : {
                        message: `Force pushing replaces ${target} with "${confirmRequest.branch}", discarding commits on the remote that are not in this branch. Work pushed by someone else since the last fetch stops the push instead.`,
                        confirmLabel: "Force push",
                    };
            }
            case "add_remote_for_push": return {
                message: "There are no remotes to push to, would you like to add one?",
                confirmLabel: "Yes",
                danger: false,
            };
        }
    }, [confirmRequest]);

    return { confirmConfig, escalateForcePush, promptConfig, submitConfirm, submitPrompt };
}
