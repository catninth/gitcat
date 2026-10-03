import { invokeTauri, isTauriEnvironment } from "./platform";
import type { CheckSummary, ForgeRepo, PullRequestInfo } from "./types";

/**
 * Pull requests and check state come from the backend, which owns the network
 * access, the credential and the cache. Outside Tauri there is no backend to
 * ask, so the rows simply stay undecorated.
 */
export async function fetchPullRequests(
    repo: ForgeRepo,
    refresh = false,
): Promise<PullRequestInfo[]> {
    if (!isTauriEnvironment()) return [];
    return invokeTauri<PullRequestInfo[]>("forge_pull_requests", { repo, refresh });
}

/** Where a remote's repository lives now; itself when it has not moved. */
export async function fetchRepositoryLocation(repo: ForgeRepo): Promise<ForgeRepo> {
    if (!isTauriEnvironment()) return repo;
    return invokeTauri<ForgeRepo>("forge_repository_location", { repo });
}

/** Rolled-up check state for a handful of tips. The backend caps the batch. */
export async function fetchChecks(
    repo: ForgeRepo,
    oids: readonly string[],
    refresh = false,
): Promise<CheckSummary[]> {
    if (!isTauriEnvironment() || oids.length === 0) return [];
    return invokeTauri<CheckSummary[]>("forge_checks", { repo, oids: [...oids], refresh });
}
