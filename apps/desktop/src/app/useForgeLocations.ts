import { useEffect, useMemo, useRef, useState } from "react";

import { forgeRepoFor, forgeRepoKey } from "../lib/forge";
import { fetchRepositoryLocation } from "../lib/pullRequests";
import type { ForgeRepo, RepositorySnapshot } from "../lib/types";

const NO_LOCATIONS: ReadonlyMap<string, ForgeRepo> = new Map();
const RETRY_AFTER_MS = 5 * 60 * 1000;

/**
 * Where each GitHub remote's repository lives now, keyed by `forgeRepoKey` of
 * what its URL says. Feed the result to `withForgeLocations`.
 *
 * Each repository is asked about once per session; the backend caches the
 * answer as well. Until an answer arrives the remote keeps the owner its URL
 * names, which is still a working remote through the service's redirect.
 */
export function useForgeLocations(
    snapshot: RepositorySnapshot | null,
): ReadonlyMap<string, ForgeRepo> {
    const [locations, setLocations] = useState(NO_LOCATIONS);
    // Key to when the lookup was sent; a failed one becomes askable again once
    // `RETRY_AFTER_MS` has passed, so an offline session does not send one per
    // snapshot.
    const asked = useRef(new Map<string, number>());

    const repos = useMemo(() => {
        const byKey = new Map<string, ForgeRepo>();
        for (const remote of snapshot?.remotes ?? []) {
            const repo = forgeRepoFor(remote);
            if (repo?.forge === "github") byKey.set(forgeRepoKey(repo), repo);
        }
        return byKey;
    }, [snapshot?.remotes]);

    // An answer is a fact about the repository rather than about the tab that
    // asked, so it is kept even when the tab has moved on by the time it lands.
    useEffect(() => {
        const now = Date.now();
        for (const [key, repo] of repos) {
            const sent = asked.current.get(key);
            if (sent !== undefined && now - sent < RETRY_AFTER_MS) continue;
            asked.current.set(key, now);
            void fetchRepositoryLocation(repo).then(
                (location) => {
                    asked.current.set(key, Number.POSITIVE_INFINITY);
                    setLocations((current) => new Map(current).set(key, location));
                },
                () => {
                    // Left to expire, so a later snapshot asks again.
                },
            );
        }
    }, [repos]);

    return locations;
}
