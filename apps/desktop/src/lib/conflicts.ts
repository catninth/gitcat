import type {
  ConflictKind,
  ConflictResolution,
  OperationSource,
  RepositoryOperationState,
} from "./types";

/**
 * Everything the conflict texts are worded from. GitKraken (the visual
 * reference) names the sides by branch where there is one and by commit where
 * there is not, so the labels need the operation, both sides and HEAD.
 */
export interface ConflictContext {
  operation: RepositoryOperationState;
  /** The checked-out branch, or `detached @ <oid>` while detached. */
  branchName: string;
  source: OperationSource | null;
  /** Short HEAD commit id: the current side when no operation names one. */
  headShort?: string | null;
}

export interface ConflictSideLabels {
  ours: string;
  theirs: string;
  oursDescription: string;
  theirsDescription: string;
}

/** What GitKraken calls the stash side of a stash apply or a pull's autostash. */
export const APPLIED_PATCH_LABEL = "Applied patch or stash";

/** The toast body every mutation that stops on conflicts shares. */
export const CONFLICT_TOAST_DETAIL = "There are merge conflicts that need to be resolved";

const OID = /^[0-9a-f]{7,64}$/i;

export function shortOid(value: string): string {
  return OID.test(value) ? value.slice(0, 7) : value;
}

export function conflictOperationLabel(operation: RepositoryOperationState): string {
  switch (operation) {
    case "merge": return "merge";
    case "rebase": return "rebase";
    case "cherry_pick": return "cherry-pick";
    case "revert": return "revert";
    case "bisect": return "Git";
    case "normal": return "Git";
  }
}

export function operationTitle(operation: RepositoryOperationState): string {
  switch (operation) {
    case "merge": return "Merge";
    case "rebase": return "Rebase";
    case "cherry_pick": return "Cherry-pick";
    case "revert": return "Revert";
    case "bisect":
    case "normal": return "Git";
  }
}

/**
 * Headline shown while files are conflicted. Only a rebase reads differently;
 * a cherry-pick, a revert and a stash that stopped on conflicts are all a merge
 * of two sides, which is how GitKraken words them too.
 */
export function conflictHeadline(operation: RepositoryOperationState): string {
  return operation === "rebase" ? "Rebase conflicts detected" : "Merge conflicts detected";
}

export interface ConflictSubtitle {
  verb: "Merging" | "Rebasing";
  incoming: string;
  preposition: "into" | "onto";
  target: string;
}

/**
 * "Merging feature into main" / "Rebasing feature onto main". Nothing else gets
 * a subtitle: a stash or a pull's autostash has no branch to name, and
 * GitKraken's "Merging main into origin/main" after a pull is backwards.
 */
export function conflictSubtitle(context: ConflictContext): ConflictSubtitle | null {
  const { operation, source, branchName } = context;
  if (!source) return null;
  if (operation === "merge") {
    return { verb: "Merging", incoming: source.incoming, preposition: "into", target: branchName };
  }
  if (operation === "rebase" && source.onto) {
    return { verb: "Rebasing", incoming: source.incoming, preposition: "onto", target: shortOid(source.onto) };
  }
  return null;
}

/** The branch the conflicts land on; a rebase works on a detached HEAD. */
export function conflictTarget(operation: RepositoryOperationState, branchName: string): string {
  return operation === "rebase" ? "HEAD" : branchName;
}

/**
 * The working-copy row's notice. Counted, not itemised: the panel lists the
 * files, and a count that changes as they are resolved is what the row is for.
 */
export function conflictNoticeMessage(
  count: number,
  operation: RepositoryOperationState,
  branchName: string,
): string {
  const target = conflictTarget(operation, branchName);
  return count === 1
    ? `A file conflict was found when attempting to merge into ${target}`
    : `${count} file conflicts were found when attempting to merge into ${target}`;
}

export function conflictSideLabels(context: ConflictContext): ConflictSideLabels {
  const { operation, branchName, source, headShort } = context;
  const incomingCommit = source ? shortOid(source.incoming_oid ?? source.incoming) : "incoming commit";
  switch (operation) {
    case "merge":
      return {
        ours: branchName,
        theirs: source?.incoming ?? "incoming branch",
        oursDescription: "Git index stage 2: the branch currently checked out.",
        theirsDescription: "Git index stage 3: the branch being merged.",
      };
    case "rebase":
      return {
        ours: source?.onto ? shortOid(source.onto) : "rebase target",
        theirs: source?.incoming ?? "rebased commit",
        oursDescription: "Git index stage 2: the commit onto which commits are being replayed.",
        theirsDescription: "Git index stage 3: the commit currently being replayed.",
      };
    case "cherry_pick":
      return {
        ours: branchName,
        theirs: incomingCommit,
        oursDescription: "Git index stage 2: the current branch before this cherry-pick.",
        theirsDescription: "Git index stage 3: the commit being cherry-picked.",
      };
    case "revert":
      return {
        ours: branchName,
        theirs: incomingCommit,
        oursDescription: "Git index stage 2: the current branch before applying the reverse change.",
        theirsDescription: "Git index stage 3: the reverse of the commit being reverted.",
      };
    case "normal":
    case "bisect":
      return {
        ours: headShort ? shortOid(headShort) : "HEAD",
        theirs: APPLIED_PATCH_LABEL,
        oursDescription: "Git index stage 2: the commit checked out.",
        theirsDescription: "Git index stage 3: the stash or patch being applied.",
      };
  }
}

/**
 * Merge editor pane title: "Commit 8c69e9 on main". The stash side of a stash
 * apply has no commit worth naming, so it keeps GitKraken's fixed wording.
 */
export function mergePaneTitle(
  side: "ours" | "theirs",
  context: ConflictContext,
  /** The text after `>>>>>>>`; a rebase writes the replayed commit there. */
  markerLabel?: string,
): string {
  const { operation, branchName, source, headShort } = context;
  if (side === "theirs" && (operation === "normal" || operation === "bisect")) return APPLIED_PATCH_LABEL;
  if (side === "ours") {
    // During a rebase HEAD is the onto commit plus whatever was replayed.
    const line = operation === "rebase" ? (source?.onto ? shortOid(source.onto) : "HEAD") : branchName;
    return headShort ? `Commit ${shortOid(headShort)} on ${line}` : line;
  }
  if (!source) return "Incoming";
  switch (operation) {
    case "merge":
      return source.incoming_oid ? `Commit ${shortOid(source.incoming_oid)} on ${source.incoming}` : source.incoming;
    case "rebase": {
      const replayed = markerLabel?.match(/^([0-9a-f]{7,64})\b/i)?.[1];
      return replayed ? `Commit ${shortOid(replayed)} on ${source.incoming}` : `Commit on ${source.incoming}`;
    }
    default:
      return `Commit ${shortOid(source.incoming_oid ?? source.incoming)}`;
  }
}

export type PrimaryOperationAction = "commit" | "continue" | "skip";

export interface OperationButtons {
  primary: { action: PrimaryOperationAction; label: string; disabled: boolean };
  /** `null` where the operation has nothing to abort (a stash apply). */
  abortLabel: string | null;
}

/**
 * The buttons under the commit message while an operation or a conflict is in
 * progress. A merge commits; a rebase skips the commit it is stuck on until its
 * conflicts are gone, then continues; a cherry-pick and a revert continue. A
 * stash apply stopped on conflicts is not an operation Git can abort, so it
 * commits like any other change.
 */
export function operationButtons(
  operation: RepositoryOperationState,
  conflictCount: number,
  stagedCount: number,
): OperationButtons {
  const blocked = conflictCount > 0;
  switch (operation) {
    case "merge":
      return {
        primary: {
          action: "continue",
          label: blocked && stagedCount === 0 ? "No Changes to Merge" : "Commit and Merge",
          disabled: blocked,
        },
        abortLabel: "Abort Merge",
      };
    case "rebase":
      return {
        primary: blocked
          ? { action: "skip", label: "Skip Commit", disabled: false }
          : { action: "continue", label: "Continue Rebase", disabled: false },
        abortLabel: "Abort Rebase",
      };
    case "cherry_pick":
      return {
        primary: { action: "continue", label: "Continue Cherry-pick", disabled: blocked },
        abortLabel: "Abort Cherry-pick",
      };
    case "revert":
      return {
        primary: { action: "continue", label: "Continue Revert", disabled: blocked },
        abortLabel: "Abort Revert",
      };
    case "normal":
    case "bisect":
      return {
        primary: {
          action: "commit",
          label: stagedCount > 0 ? `Commit Changes to ${stagedCount} File${stagedCount === 1 ? "" : "s"}` : "Stage Changes to Commit",
          disabled: blocked || stagedCount === 0,
        },
        abortLabel: null,
      };
  }
}

export interface ConflictDecisionChoice {
  label: string;
  resolution: ConflictResolution;
  danger?: boolean;
}

export interface ConflictDecision {
  message: string;
  choices: ConflictDecisionChoice[];
  /** Git wrote this path aside for a file/directory conflict. */
  helperFile: boolean;
}

/**
 * The question a conflict that is not two versions of one text file asks
 * instead of opening the merge editor. "Us" is index stage 2, "them" stage 3.
 */
export function conflictDecision(
  kind: ConflictKind | undefined,
  path: string,
  labels: Pick<ConflictSideLabels, "ours" | "theirs">,
  helperFile = false,
): ConflictDecision | null {
  const name = path.split("/").at(-1) ?? path;
  const remove: ConflictDecisionChoice = { label: "Delete The File", resolution: "delete", danger: true };
  const base: ConflictDecisionChoice = { label: "Keep Base Version", resolution: "base" };
  switch (kind) {
    case "deleted_by_them":
      return {
        message: `${name} has been modified on ${labels.ours}, but deleted on ${labels.theirs}.`,
        choices: [{ label: "Keep Modified Version", resolution: "ours" }, remove, base],
        helperFile,
      };
    case "deleted_by_us":
      return {
        message: `${name} has been modified on ${labels.theirs}, but deleted on ${labels.ours}.`,
        choices: [{ label: "Keep Modified Version", resolution: "theirs" }, remove, base],
        helperFile,
      };
    case "added_by_us":
    case "added_by_them":
      return {
        message: `GitCat was unable to determine whether to keep ${name}, would you like to keep it?`,
        choices: [
          { label: "Keep New Version", resolution: kind === "added_by_us" ? "ours" : "theirs" },
          remove,
        ],
        helperFile,
      };
    case "both_deleted":
      return {
        message: `${name} was deleted on both sides.`,
        choices: [remove, base],
        helperFile,
      };
    case "both_modified":
    case "both_added":
    case undefined:
      return null;
  }
}

/** `fd~HEAD`: the name Git gives a file it moved aside for a same-named directory. */
export function looksLikeHelperCopy(path: string, allPaths: readonly string[]): boolean {
  const cut = path.lastIndexOf("~");
  if (cut <= 0 || cut === path.length - 1 || path.slice(cut + 1).includes("/")) return false;
  const prefix = `${path.slice(0, cut)}/`;
  return allPaths.some((candidate) => candidate.startsWith(prefix));
}

export type ConflictMutation =
  | "merge"
  | "rebase"
  | "rebase_step"
  | "cherry_pick"
  | "revert"
  | "stash_apply"
  | "pull"
  | "pull_autostash";

export interface ConflictToast {
  title: string;
  detail: string;
}

/** Toasts for a command that stopped on conflicts, as GitKraken titles them. */
export function conflictToasts(kind: ConflictMutation): ConflictToast[] {
  const toast = (title: string) => ({ title, detail: CONFLICT_TOAST_DETAIL });
  switch (kind) {
    case "merge": return [toast("Merge Failed")];
    case "rebase": return [toast("Rebase Failed")];
    case "rebase_step": return [toast("Rebase Skip/Continue Failed")];
    case "cherry_pick": return [toast("Cherry pick Failed")];
    case "revert": return [toast("Revert Failed")];
    case "stash_apply": return [toast("Error Applying Stash")];
    case "pull": return [{ title: "Pull Failed", detail: "Has conflicts" }];
    case "pull_autostash": return [toast("Error Applying Stash"), { title: "Pull Failed", detail: "Has conflicts" }];
  }
}

/** Which toast a continue or skip stopping on new conflicts gets. */
export function continueConflictMutation(operation: RepositoryOperationState): ConflictMutation {
  switch (operation) {
    case "rebase": return "rebase_step";
    case "cherry_pick": return "cherry_pick";
    case "revert": return "revert";
    default: return "merge";
  }
}
