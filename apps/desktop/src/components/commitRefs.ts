// Which ref labels a commit row shows, and in what order.

import type { CommitSummary, RefLabel } from "../lib/types";

export type GraphRefLabel = RefLabel & {
  synthetic?: boolean;
};
export function isBranchDecoration(decoration: RefLabel): boolean {
  return decoration.kind === "local_branch" || decoration.kind === "remote_branch";
}

export function refPriority(decoration: RefLabel): number {
  if (decoration.is_head) return 0;
  if (decoration.kind === "local_branch") return 1;
  if (decoration.kind === "remote_branch") return 2;
  return 3;
}

export function sortedDecorations(decorations: readonly GraphRefLabel[]): GraphRefLabel[] {
  return [...decorations].sort((left, right) => (
    refPriority(left) - refPriority(right)
      || left.name.localeCompare(right.name)
      || left.full_name.localeCompare(right.full_name)
  ));
}

// The ref a row acts on when a click did not land on a specific label: the one
// the ref stack draws first, so the row menu and the label menu agree.
export function primaryBranchDecoration(commit: CommitSummary): RefLabel | null {
  const branches = commit.decorations.filter(isBranchDecoration);
  return sortedDecorations(branches)[0] ?? null;
}

export function visibleDecorations(
  commit: CommitSummary,
  hideHeadDecoration: boolean,
  detachedHeadOid?: string | null,
): GraphRefLabel[] {
  const decorations: GraphRefLabel[] = commit.decorations
    .filter((decoration) => !hideHeadDecoration || !decoration.is_head);

  if (!hideHeadDecoration && detachedHeadOid === commit.oid && !decorations.some((decoration) => decoration.is_head)) {
    decorations.push({
      name: "HEAD",
      full_name: "HEAD",
      kind: "local_branch",
      is_head: true,
      synthetic: true,
    });
  }

  return sortedDecorations(decorations);
}

export function remoteBranchNameWithoutRemote(name: string): string {
  const slashIndex = name.indexOf("/");
  return slashIndex >= 0 ? name.slice(slashIndex + 1) : name;
}

export function remoteNameFromBranchName(name: string): string | null {
  const slashIndex = name.indexOf("/");
  return slashIndex > 0 ? name.slice(0, slashIndex) : null;
}
