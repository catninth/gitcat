import type {
    CheckSummary,
    ForgeKind,
    ForgeRepo,
    ForgeRepository,
    PullRequestInfo,
    RemoteInfo,
    RepositorySnapshot,
} from "./types";

// Human-readable names for the settings override list.
export const FORGE_LABELS: Record<ForgeKind, string> = {
    unknown: "Detect from host",
    github: "GitHub",
    gitlab: "GitLab",
    bitbucket: "Bitbucket",
    gitea: "Gitea / Forgejo",
    azure_devops: "Azure DevOps",
};

// The order the settings dropdown offers, detection first.
export const FORGE_KINDS: readonly ForgeKind[] = [
    "unknown",
    "github",
    "gitlab",
    "bitbucket",
    "gitea",
    "azure_devops",
];

export function isForgeKind(value: string): value is ForgeKind {
    return value in FORGE_LABELS;
}

// The backend recognises public hosts only, because a self-hosted install
// cannot be told apart from any other domain without asking it over the
// network. Settings map such a host to its forge by hand.
export function effectiveForge(
    remote: RemoteInfo | null | undefined,
    overrides: Readonly<Record<string, ForgeKind>> = {},
): ForgeKind {
    if (!remote) return "unknown";
    const host = remote.url?.host;
    const override = host ? overrides[host] : undefined;
    if (override && override !== "unknown") return override;
    return remote.forge;
}

// Re-labels every remote whose host is named in settings, so the rest of the
// app can keep reading `remote.forge` without knowing overrides exist. The
// home page the backend built stays correct: overriding only changes the path
// layout below it.
export function withForgeOverrides(
    snapshot: RepositorySnapshot | null,
    overrides: Readonly<Record<string, ForgeKind>>,
): RepositorySnapshot | null {
    if (!snapshot || Object.keys(overrides).length === 0) return snapshot;
    let changed = false;
    const remotes = snapshot.remotes.map((remote) => {
        const forge = effectiveForge(remote, overrides);
        if (forge === remote.forge) return remote;
        changed = true;
        return { ...remote, forge };
    });
    return changed ? { ...snapshot, remotes } : snapshot;
}

export function forgeRepoKey(repo: ForgeRepo): string {
    return `${repo.host}/${repo.owner}/${repo.repo}/${repo.forge}`;
}

// A transferred or renamed repository keeps answering at the path its remote
// URL was cloned with, so the owner parsed from that URL can name an account
// the repository has left. `locations` holds where the service says each one
// lives now, keyed by `forgeRepoKey` of the URL's own reading. Only the parsed
// parts and the home page move: the fetch and push URLs are what Git uses, and
// they keep working through the service's redirect.
export function withForgeLocations(
    snapshot: RepositorySnapshot | null,
    locations: ReadonlyMap<string, ForgeRepo>,
): RepositorySnapshot | null {
    if (!snapshot || locations.size === 0) return snapshot;
    let changed = false;
    const remotes = snapshot.remotes.map((remote) => {
        const repo = forgeRepoFor(remote);
        const location = repo ? locations.get(forgeRepoKey(repo)) : undefined;
        if (!repo || !remote.url || !location) return remote;
        if (location.owner === repo.owner && location.repo === repo.repo) return remote;
        changed = true;
        const oldPath = `/${repo.owner}/${repo.repo}`;
        const webUrl = remote.web_url?.replace(/\/+$/, "");
        return {
            ...remote,
            url: {
                ...remote.url,
                owner: location.owner,
                repo: location.repo,
                path: `${location.owner}/${location.repo}`,
            },
            web_url: webUrl?.endsWith(oldPath)
                ? `${webUrl.slice(0, -oldPath.length)}/${location.owner}/${location.repo}`
                : remote.web_url,
        };
    });
    return changed ? { ...snapshot, remotes } : snapshot;
}

// Web layouts diverge below the repository home page: GitLab nests everything
// under `/-/`, Bitbucket pluralises commits, Gitea names the ref kind, and
// Azure passes the branch as a query parameter. An unrecognised host follows
// the GitHub layout, which is what most self-hosted forges imitate.
export function forgeCommitUrl(webUrl: string, forge: ForgeKind, oid: string): string {
    const base = webUrl.replace(/\/+$/, "");
    switch (forge) {
        case "gitlab":
            return `${base}/-/commit/${oid}`;
        case "bitbucket":
            return `${base}/commits/${oid}`;
        default:
            return `${base}/commit/${oid}`;
    }
}

export function forgeBranchUrl(webUrl: string, forge: ForgeKind, branch: string): string {
    const base = webUrl.replace(/\/+$/, "");
    // Path segments keep their slashes; `feature/x` is two segments, not one
    // escaped name.
    const path = branch.split("/").map(encodeURIComponent).join("/");
    switch (forge) {
        case "gitlab":
            return `${base}/-/tree/${path}`;
        case "bitbucket":
            return `${base}/src/${path}/`;
        case "gitea":
            return `${base}/src/branch/${path}`;
        case "azure_devops":
            return `${base}?version=GB${encodeURIComponent(branch)}`;
        default:
            return `${base}/tree/${path}`;
    }
}

// Only GitHub serves an owner avatar under a plain `<owner>.png` path, so the
// remote icon stays GitHub-only until an avatar layer lands.
export function forgeOwnerIconUrl(
    remote: RemoteInfo,
    forge: ForgeKind,
    size = 32,
): string | null {
    if (forge !== "github") return null;
    const owner = remote.url?.owner;
    return owner ? `https://github.com/${encodeURIComponent(owner)}.png?size=${size}` : null;
}

// A hosting-service request needs the path as well as the host, so a remote
// that parsed to neither -- a local clone, an unresolved SSH alias -- has
// nothing to ask about.
export function forgeRepoFor(remote: RemoteInfo | null | undefined): ForgeRepo | null {
    const { host, owner, repo } = remote?.url ?? {};
    if (!remote || !host || !owner || !repo) return null;
    return { host, owner, repo, forge: remote.forge };
}

// Branch rows look their pull request up by branch name, so a fork's pull
// request is dropped rather than matched: a fork's `main` is not this
// repository's `main`. When two pull requests share a head branch the first
// wins, and the service already sorted them by most recently updated.
export function pullRequestsByBranch(
    pulls: readonly PullRequestInfo[],
    owner: string,
): Map<string, PullRequestInfo> {
    const byBranch = new Map<string, PullRequestInfo>();
    const target = owner.toLocaleLowerCase();
    for (const pull of pulls) {
        if (pull.head_owner && pull.head_owner.toLocaleLowerCase() !== target) continue;
        if (!byBranch.has(pull.head_ref)) byBranch.set(pull.head_ref, pull);
    }
    return byBranch;
}

// Rows without a reported check are absent rather than present-and-empty, so
// a caller can tell "nothing ran" from "not asked yet".
export function checksByOid(summaries: readonly CheckSummary[]): Map<string, CheckSummary> {
    const byOid = new Map<string, CheckSummary>();
    for (const summary of summaries) {
        if (summary.state !== "none") byOid.set(summary.oid, summary);
    }
    return byOid;
}

/** One owner's repositories, in the order a listing draws them. */
export interface OwnerGroup {
    owner: string;
    repositories: ForgeRepository[];
}

// An account reaches its own repositories and every user or organisation that
// granted it access, and those are separate places rather than one flat list:
// a repository name only says which repository it is once its owner is on
// screen. The signed-in account leads because it is the one owner the user
// always has; the rest are alphabetical, since nothing else orders them.
export function groupByOwner(
    repositories: readonly ForgeRepository[],
    account?: string,
): OwnerGroup[] {
    const groups = new Map<string, OwnerGroup>();
    for (const repository of repositories) {
        const owner = repository.owner || repository.full_name.split("/")[0] || repository.full_name;
        const key = owner.toLocaleLowerCase();
        const group = groups.get(key);
        if (group) group.repositories.push(repository);
        else groups.set(key, { owner, repositories: [repository] });
    }

    const own = account?.trim().toLocaleLowerCase();
    for (const group of groups.values()) {
        group.repositories.sort((left, right) => left.name.localeCompare(right.name));
    }
    return [...groups.values()].sort((left, right) => {
        const leftOwn = left.owner.toLocaleLowerCase() === own;
        const rightOwn = right.owner.toLocaleLowerCase() === own;
        if (leftOwn !== rightOwn) return leftOwn ? -1 : 1;
        return left.owner.localeCompare(right.owner);
    });
}
