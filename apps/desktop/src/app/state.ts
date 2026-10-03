import type { gitcatApi } from "../lib/api";
import type { BranchScope } from "../components/ref-sidebar";
import type { TabView } from "../components/top-tabs";
import type { BranchInfo, CommitSummary, RefLabel, RepositoryInfo } from "../lib/types";

// The center pane shows one of three things: the graph, a file diff, or the
// three-way editor for a conflicted file.
export type CenterView = "graph" | "diff" | "merge";

export interface RuntimeRepository {
    repository_id: string;
    info: RepositoryInfo;
}

export type PromptState =
    | { kind: "alias_tab"; tabId: string; current: string }
    | { kind: "create_branch"; startOid: string }
    | { kind: "rename_branch"; branch: BranchInfo }
    | { kind: "create_tag"; oid: string; annotated: boolean }
    | { kind: "set_upstream"; branch: BranchInfo }
    | null;

export type ConfirmState =
    | { kind: "delete_branch"; name: string; force: boolean }
    | { kind: "delete_stash"; oid: string; selector: string; message: string }
    /**
     * A push that overwrites the remote. `ignoreRemote` is the second step:
     * the lease was refused because the remote moved, and the user asked to
     * overwrite it anyway. It is never the state a menu opens with.
     */
    | { kind: "force_push"; remote: string; branch: string; ignoreRemote: boolean }
    /** A push asked for in a repository with nowhere to push to. */
    | { kind: "add_remote_for_push" }
    | null;

export interface CommitMenuState {
    x: number;
    y: number;
    commit: CommitSummary;
    // The ref label the click landed on, when it landed on one.
    decoration?: RefLabel | null;
}

export interface TabMenuState {
    x: number;
    y: number;
    tab: TabView;
}

export interface BranchMenuState {
    x: number;
    y: number;
    branch: BranchInfo;
    scope: BranchScope;
}

export type RunMutation = (
    title: string,
    operation: (repository: RuntimeRepository) => Promise<unknown>,
    options?: {
        silent?: boolean;
        optimistic?: () => (() => void) | undefined;
        onError?: (error: unknown) => boolean;
        /**
         * Names the kind of operation for the mutation queue. A command asked
         * for again while one of the same kind is already running or waiting
         * is dropped rather than queued twice; a mutation with no key never
         * collides with another.
         */
        queueKey?: string;
        /**
         * On success, shows this as the WIP row's title until the next
         * mutation (any mutation not itself passing this option clears it).
         * Mirrors GitKraken, which labels the working-tree row with the
         * message of the stash that was just popped into it.
         */
        wipTitleHint?: string;
    },
) => Promise<boolean>;

export type CommitDetailsPanel = Awaited<ReturnType<typeof gitcatApi.commitDetails>>;
