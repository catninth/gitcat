import assert from "node:assert/strict";
import test from "node:test";

import { remoteBranchUrl, remoteCommitUrl, remoteIconUrls } from "../src/app/branches";
import { avatarEmailsToAsk, avatarLookupFor, remoteSupportsAvatars } from "../src/lib/avatars";
import { normalizeAppSettings } from "../src/app/workspace";
import {
    checksByOid,
    effectiveForge,
    forgeBranchUrl,
    forgeCommitUrl,
    forgeOwnerIconUrl,
    forgeRepoFor,
    forgeRepoKey,
    groupByOwner,
    isForgeKind,
    pullRequestsByBranch,
    withForgeLocations,
    withForgeOverrides,
} from "../src/lib/forge";
import type {
    CheckSummary,
    ForgeKind,
    ForgeRepository,
    PullRequestInfo,
    RemoteInfo,
    RepositorySnapshot,
} from "../src/lib/types";

const OID = "e90df7571bc1195ac454e29873f2bb2276f5dc04";

function remote(overrides: Partial<RemoteInfo> = {}): RemoteInfo {
    return {
        name: "origin",
        fetch_url: "git@github.com:ikoli/gitcat.git",
        push_url: "git@github.com:ikoli/gitcat.git",
        url: {
            scheme: "scp_like",
            host: "github.com",
            path: "ikoli/gitcat",
            owner: "ikoli",
            repo: "gitcat",
        },
        forge: "github",
        web_url: "https://github.com/ikoli/gitcat",
        ...overrides,
    };
}

function snapshot(...remotes: RemoteInfo[]): RepositorySnapshot {
    return {
        generation: "1",
        head: { kind: "branch", name: "main", oid: OID },
        operation_state: "normal",
        status: { clean: true, entries: [] },
        local_branches: [],
        remote_branches: [],
        tags: [],
        remotes,
        capabilities: { shallow: false, partial_clone: false, sparse_checkout: false, worktree: false },
    };
}

test("commit links follow each forge's own layout", () => {
    const base = "https://host.test/team/tool";
    assert.equal(forgeCommitUrl(base, "github", OID), `${base}/commit/${OID}`);
    assert.equal(forgeCommitUrl(base, "gitea", OID), `${base}/commit/${OID}`);
    assert.equal(forgeCommitUrl(base, "gitlab", OID), `${base}/-/commit/${OID}`);
    assert.equal(forgeCommitUrl(base, "bitbucket", OID), `${base}/commits/${OID}`);
    // An unrecognised host follows GitHub, which most self-hosted forges copy.
    assert.equal(forgeCommitUrl(base, "unknown", OID), `${base}/commit/${OID}`);
});

test("branch links follow each forge's own layout", () => {
    const base = "https://host.test/team/tool";
    assert.equal(forgeBranchUrl(base, "github", "main"), `${base}/tree/main`);
    assert.equal(forgeBranchUrl(base, "gitlab", "main"), `${base}/-/tree/main`);
    assert.equal(forgeBranchUrl(base, "bitbucket", "main"), `${base}/src/main/`);
    assert.equal(forgeBranchUrl(base, "gitea", "main"), `${base}/src/branch/main`);
    assert.equal(forgeBranchUrl(base, "azure_devops", "main"), `${base}?version=GBmain`);
});

test("a slash in a branch name stays a path separator", () => {
    const base = "https://host.test/team/tool";
    assert.equal(forgeBranchUrl(base, "github", "feature/diff viewer"), `${base}/tree/feature/diff%20viewer`);
    // Azure passes the branch as one query value, so the slash is escaped.
    assert.equal(forgeBranchUrl(base, "azure_devops", "feature/x"), `${base}?version=GBfeature%2Fx`);
});

test("only GitHub serves an owner avatar under a bare path", () => {
    assert.equal(
        forgeOwnerIconUrl(remote(), "github"),
        "https://github.com/ikoli.png?size=32",
    );
    assert.equal(forgeOwnerIconUrl(remote(), "gitlab"), null);
    assert.equal(forgeOwnerIconUrl(remote({ url: undefined }), "github"), null);
});

test("an override names the forge of a host the backend could not recognise", () => {
    const selfHosted = remote({
        fetch_url: "git@git.example.test:team/tool.git",
        push_url: "git@git.example.test:team/tool.git",
        url: {
            scheme: "scp_like",
            host: "git.example.test",
            path: "team/tool",
            owner: "team",
            repo: "tool",
        },
        forge: "unknown",
        web_url: "https://git.example.test/team/tool",
    });
    const overrides: Record<string, ForgeKind> = { "git.example.test": "gitlab" };

    assert.equal(effectiveForge(selfHosted, overrides), "gitlab");
    assert.equal(effectiveForge(selfHosted, {}), "unknown");
    // A recognised host is not overridden by an entry for a different host.
    assert.equal(effectiveForge(remote(), overrides), "github");

    const overridden = withForgeOverrides(snapshot(selfHosted), overrides);
    assert.equal(
        remoteCommitUrl(overridden, "origin", OID),
        `https://git.example.test/team/tool/-/commit/${OID}`,
    );
});

test("an unchanged snapshot keeps its identity so memoised consumers do not rerun", () => {
    const unchanged = snapshot(remote());
    assert.equal(withForgeOverrides(unchanged, {}), unchanged);
    assert.equal(withForgeOverrides(unchanged, { "other.test": "gitea" }), unchanged);
});

test("a remote without a web url offers no links", () => {
    const local = snapshot(remote({
        fetch_url: "/srv/git/tool.git",
        push_url: "/srv/git/tool.git",
        url: undefined,
        forge: "unknown",
        web_url: undefined,
    }));
    assert.equal(remoteCommitUrl(local, "origin", OID), null);
    assert.equal(remoteBranchUrl(local, "origin", "main"), null);
    assert.equal(remoteCommitUrl(snapshot(remote()), "upstream", OID), null);
    assert.equal(remoteIconUrls(local.remotes).size, 0);
});

test("remote icons are keyed by remote name", () => {
    const icons = remoteIconUrls(snapshot(remote()).remotes);
    assert.equal(icons.get("origin"), "https://github.com/ikoli.png?size=32");
});

test("stored overrides are sanitised to bare lower-case hosts", () => {
    const settings = normalizeAppSettings({
        forge_overrides: {
            "Git.Example.Test": "gitlab",
            "https://git.other.test": "gitea",
            "git.third.test/team": "gitea",
            "git.fourth.test": "not-a-forge",
            "git.fifth.test": "unknown",
        },
    });
    assert.deepEqual(settings.forge_overrides, { "git.example.test": "gitlab" });
});

test("only a GitHub remote with an owner and repository can resolve avatars", () => {
    assert.ok(remoteSupportsAvatars(remote()));
    assert.ok(!remoteSupportsAvatars(remote({ forge: "gitlab" })));
    assert.ok(!remoteSupportsAvatars(remote({ url: undefined })));
    assert.ok(!remoteSupportsAvatars(null));
});

test("a lookup carries the repository, the forge and the page tip", () => {
    assert.deepEqual(avatarLookupFor(remote(), ["a@b.test"], OID), {
        host: "github.com",
        owner: "ikoli",
        repo: "gitcat",
        forge: "github",
        tip_oid: OID,
        emails: ["a@b.test"],
    });
    // Without a tip the service walks from the default branch instead.
    assert.deepEqual(avatarLookupFor(remote(), [], null)?.tip_oid, undefined);
    assert.equal(avatarLookupFor(remote({ url: undefined }), ["a@b.test"], OID), null);
});

test("an unresolved author is asked about again once the tip has moved", () => {
    const emails = ["known@b.test", "new@b.test", "unknown@b.test"];
    const asked = new Set(["known@b.test", "unknown@b.test"]);
    const unresolved = new Set(["unknown@b.test"]);

    // The same tip only pays for the addresses nobody has asked about yet.
    assert.deepEqual(avatarEmailsToAsk(emails, asked, unresolved, false), ["new@b.test"]);
    // A tip that moved may name the author the last round could not.
    assert.deepEqual(
        avatarEmailsToAsk(emails, asked, unresolved, true),
        ["new@b.test", "unknown@b.test"],
    );
    // A resolved author is never sent again.
    assert.deepEqual(avatarEmailsToAsk(["known@b.test"], asked, unresolved, true), []);
});

test("avatar settings fall back to the defaults, not to enabled Gravatar", () => {
    assert.deepEqual(normalizeAppSettings({}).avatars, {
        enabled: true,
        gravatar_fallback: false,
    });
    assert.deepEqual(
        normalizeAppSettings({ avatars: { enabled: false, gravatar_fallback: "yes" } }).avatars,
        { enabled: false, gravatar_fallback: false },
    );
});

test("forge kinds are recognised by name", () => {
    assert.ok(isForgeKind("github"));
    assert.ok(!isForgeKind("sourcehut"));
});


function pull(overrides: Partial<PullRequestInfo> = {}): PullRequestInfo {
    return {
        number: 7,
        title: "Add lanes",
        state: "open",
        head_ref: "feat/lanes",
        head_oid: OID,
        head_owner: "ikoli",
        base_ref: "main",
        url: "https://github.com/ikoli/gitcat/pull/7",
        ...overrides,
    };
}

function summary(overrides: Partial<CheckSummary> = {}): CheckSummary {
    return { oid: OID, state: "success", total: 3, failed: 0, pending: 0, ...overrides };
}

test("a request target needs the path, not just the host", () => {
    assert.deepEqual(forgeRepoFor(remote()), {
        host: "github.com",
        owner: "ikoli",
        repo: "gitcat",
        forge: "github",
    });
    assert.equal(forgeRepoFor(remote({ url: undefined })), null);
    assert.equal(forgeRepoFor(null), null);
    // An override reaches the request, so a self-hosted install is asked as
    // the forge the user named.
    assert.equal(forgeRepoFor(remote({ forge: "gitlab" }))?.forge, "gitlab");
});

test("a fork's pull request does not decorate the local branch of that name", () => {
    const byBranch = pullRequestsByBranch(
        [
            pull(),
            pull({ number: 8, head_ref: "main", head_owner: "someone-else" }),
            pull({ number: 9, head_ref: "main", head_owner: "IKOLI" }),
        ],
        "ikoli",
    );

    assert.deepEqual([...byBranch.keys()], ["feat/lanes", "main"]);
    // The owner comparison is case-insensitive; GitHub logins are.
    assert.equal(byBranch.get("main")?.number, 9);
});

test("the first pull request on a branch wins, and the service sorted them", () => {
    const byBranch = pullRequestsByBranch(
        [pull({ number: 4 }), pull({ number: 5 })],
        "ikoli",
    );
    assert.equal(byBranch.get("feat/lanes")?.number, 4);
});

test("a commit nothing reported on is absent rather than present and empty", () => {
    const byOid = checksByOid([
        summary(),
        summary({ oid: "abc", state: "none", total: 0 }),
    ]);

    assert.deepEqual([...byOid.keys()], [OID]);
    assert.equal(byOid.get(OID)?.state, "success");
});

test("forge settings fall back to the defaults", () => {
    assert.deepEqual(normalizeAppSettings({}).forge, { pull_requests: true, checks: true });
    assert.deepEqual(
        normalizeAppSettings({ forge: { pull_requests: false, checks: "yes" } }).forge,
        { pull_requests: false, checks: true },
    );
});

function listed(full_name: string, overrides: Partial<ForgeRepository> = {}): ForgeRepository {
    const [owner, name] = full_name.split("/");
    return {
        full_name,
        owner,
        name,
        private: false,
        fork: false,
        clone_url: `https://github.com/${full_name}.git`,
        ...overrides,
    };
}

test("a listing is grouped by owner, with the signed-in account leading", () => {
    const groups = groupByOwner([
        listed("riftmarch/servers"),
        listed("fantasydream-hu/mono"),
        listed("RisDN/gitcat"),
        listed("fantasydream-hu/discord-bot"),
    ], "risdn");

    assert.deepEqual(groups.map((group) => group.owner), [
        "RisDN",
        "fantasydream-hu",
        "riftmarch",
    ]);
    assert.deepEqual(groups[1].repositories.map((repository) => repository.name), [
        "discord-bot",
        "mono",
    ]);
});

test("owners group case-insensitively, and without an account nothing leads", () => {
    const groups = groupByOwner([
        listed("Riftmarch/servers"),
        listed("riftmarch/wiki"),
        listed("acme/tool"),
    ]);

    assert.deepEqual(groups.map((group) => group.owner), ["acme", "Riftmarch"]);
    assert.equal(groups[1].repositories.length, 2);
});

test("a transferred repository takes the owner the service names, not its cloned url", () => {
    const cloned = remote({
        fetch_url: "https://github.com/RisDN/gitcat.git",
        push_url: "https://github.com/RisDN/gitcat.git",
        url: { scheme: "https", host: "github.com", path: "RisDN/gitcat", owner: "RisDN", repo: "gitcat" },
        web_url: "https://github.com/RisDN/gitcat",
    });
    const repo = forgeRepoFor(cloned)!;
    const locations = new Map([[forgeRepoKey(repo), { ...repo, owner: "catninth" }]]);

    const moved = withForgeLocations(snapshot(cloned, remote({ name: "upstream" })), locations)!;
    const origin = moved.remotes[0];
    assert.equal(origin.url?.owner, "catninth");
    assert.equal(origin.url?.path, "catninth/gitcat");
    assert.equal(origin.web_url, "https://github.com/catninth/gitcat");
    // Git keeps using what it was cloned with; the redirect still serves it.
    assert.equal(origin.fetch_url, "https://github.com/RisDN/gitcat.git");
    assert.equal(remoteIconUrls(moved.remotes).get("origin"), "https://github.com/catninth.png?size=32");
    // A remote nobody asked about is left as it was.
    assert.equal(moved.remotes[1].url?.owner, "ikoli");
});

test("a repository that has not moved keeps the snapshot it came in", () => {
    const origin = remote();
    const repo = forgeRepoFor(origin)!;
    const unchanged = snapshot(origin);
    assert.equal(withForgeLocations(unchanged, new Map([[forgeRepoKey(repo), repo]])), unchanged);
    assert.equal(withForgeLocations(unchanged, new Map()), unchanged);
});
