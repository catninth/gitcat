import assert from "node:assert/strict";
import test from "node:test";

import {
  APPLIED_PATCH_LABEL,
  CONFLICT_TOAST_DETAIL,
  conflictDecision,
  conflictHeadline,
  conflictNoticeMessage,
  conflictSideLabels,
  conflictSubtitle,
  conflictToasts,
  continueConflictMutation,
  looksLikeHelperCopy,
  mergePaneTitle,
  operationButtons,
  type ConflictContext,
} from "../src/lib/conflicts";

const MERGE_OID = "ffcec0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PICK_OID = "abcdef1234567890abcdef1234567890abcdef12";

const merge: ConflictContext = {
  operation: "merge",
  branchName: "main",
  source: { incoming: "feature", incoming_oid: MERGE_OID },
  headShort: "8c69e9a",
};
const rebase: ConflictContext = {
  operation: "rebase",
  branchName: "detached @ 63b5591",
  source: { incoming: "feature", onto: "main" },
  headShort: "63b5591",
};
const cherryPick: ConflictContext = {
  operation: "cherry_pick",
  branchName: "main",
  source: { incoming: "hotfix", incoming_oid: PICK_OID },
  headShort: "1111111",
};
const stash: ConflictContext = { operation: "normal", branchName: "main", source: null, headShort: "0850191" };

test("headline is a rebase or a merge, whatever stopped", () => {
  assert.equal(conflictHeadline("rebase"), "Rebase conflicts detected");
  for (const operation of ["merge", "cherry_pick", "revert", "normal"] as const) {
    assert.equal(conflictHeadline(operation), "Merge conflicts detected");
  }
});

test("only a merge and a rebase get a subtitle", () => {
  assert.deepEqual(conflictSubtitle(merge), { verb: "Merging", incoming: "feature", preposition: "into", target: "main" });
  assert.deepEqual(
    conflictSubtitle({ ...rebase, source: { incoming: "main", onto: "d60f65aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" } }),
    { verb: "Rebasing", incoming: "main", preposition: "onto", target: "d60f65a" },
  );
  assert.equal(conflictSubtitle(cherryPick), null);
  assert.equal(conflictSubtitle(stash), null);
});

test("working-copy notice counts files and names the target", () => {
  assert.equal(conflictNoticeMessage(1, "normal", "main"), "A file conflict was found when attempting to merge into main");
  assert.equal(conflictNoticeMessage(8, "merge", "main"), "8 file conflicts were found when attempting to merge into main");
  assert.equal(conflictNoticeMessage(1, "rebase", "detached @ 63b5591"), "A file conflict was found when attempting to merge into HEAD");
});

test("side labels follow the operation", () => {
  assert.deepEqual(
    [conflictSideLabels(merge).ours, conflictSideLabels(merge).theirs],
    ["main", "feature"],
  );
  assert.deepEqual([conflictSideLabels(rebase).ours, conflictSideLabels(rebase).theirs], ["main", "feature"]);
  assert.deepEqual([conflictSideLabels(cherryPick).ours, conflictSideLabels(cherryPick).theirs], ["main", "abcdef1"]);
  assert.deepEqual([conflictSideLabels(stash).ours, conflictSideLabels(stash).theirs], ["0850191", APPLIED_PATCH_LABEL]);
});

test("merge editor panes are named by commit and branch", () => {
  assert.equal(mergePaneTitle("ours", merge), "Commit 8c69e9a on main");
  assert.equal(mergePaneTitle("theirs", merge), "Commit ffcec0a on feature");
  assert.equal(mergePaneTitle("ours", rebase), "Commit 63b5591 on main");
  assert.equal(mergePaneTitle("theirs", rebase, "5603541 (f1: edit line 2)"), "Commit 5603541 on feature");
  assert.equal(mergePaneTitle("theirs", cherryPick), "Commit abcdef1");
  assert.equal(mergePaneTitle("ours", stash), "Commit 0850191 on main");
  assert.equal(mergePaneTitle("theirs", stash), APPLIED_PATCH_LABEL);
});

test("operation buttons", () => {
  assert.deepEqual(operationButtons("merge", 1, 0), {
    primary: { action: "continue", label: "No Changes to Merge", disabled: true },
    abortLabel: "Abort Merge",
  });
  assert.deepEqual(operationButtons("merge", 1, 3).primary, { action: "continue", label: "Commit and Merge", disabled: true });
  assert.deepEqual(operationButtons("merge", 0, 1).primary, { action: "continue", label: "Commit and Merge", disabled: false });
  assert.deepEqual(operationButtons("rebase", 2, 0).primary, { action: "skip", label: "Skip Commit", disabled: false });
  assert.deepEqual(operationButtons("rebase", 0, 1).primary, { action: "continue", label: "Continue Rebase", disabled: false });
  assert.equal(operationButtons("cherry_pick", 0, 1).abortLabel, "Abort Cherry-pick");
  assert.equal(operationButtons("revert", 1, 1).primary.disabled, true);
  const stashApply = operationButtons("normal", 1, 1);
  assert.equal(stashApply.abortLabel, null);
  assert.deepEqual(stashApply.primary, { action: "commit", label: "Commit Changes to 1 File", disabled: true });
  assert.equal(operationButtons("normal", 0, 2).primary.disabled, false);
});

test("non-text conflicts ask a question instead of opening the editor", () => {
  const labels = { ours: "main", theirs: "theirs" };
  const modDel = conflictDecision("deleted_by_them", "dir/moddel.txt", labels);
  assert.equal(modDel?.message, "moddel.txt has been modified on main, but deleted on theirs.");
  assert.deepEqual(modDel?.choices.map((choice) => [choice.label, choice.resolution]), [
    ["Keep Modified Version", "ours"],
    ["Delete The File", "delete"],
    ["Keep Base Version", "base"],
  ]);
  const delMod = conflictDecision("deleted_by_us", "delmod.txt", labels);
  assert.equal(delMod?.message, "delmod.txt has been modified on theirs, but deleted on main.");
  assert.equal(delMod?.choices[0].resolution, "theirs");
  assert.equal(conflictDecision("added_by_them", "ren-theirs.txt", labels)?.choices[0].resolution, "theirs");
  assert.equal(conflictDecision("added_by_us", "fd~HEAD", labels, true)?.helperFile, true);
  assert.deepEqual(conflictDecision("both_deleted", "ren.txt", labels)?.choices.map((choice) => choice.resolution), ["delete", "base"]);
  assert.equal(conflictDecision("both_modified", "text.txt", labels), null);
  assert.equal(conflictDecision("both_added", "addadd.txt", labels), null);
});

test("helper copies are recognised beside their directory", () => {
  assert.equal(looksLikeHelperCopy("fd~HEAD", ["fd/inner.txt", "fd~HEAD"]), true);
  assert.equal(looksLikeHelperCopy("notes~draft", ["notes~draft"]), false);
  assert.equal(looksLikeHelperCopy("~HEAD", ["/x"]), false);
});

test("conflict toasts are titled by the command that stopped", () => {
  assert.deepEqual(conflictToasts("merge"), [{ title: "Merge Failed", detail: CONFLICT_TOAST_DETAIL }]);
  assert.deepEqual(conflictToasts("pull_autostash").map((toast) => toast.title), ["Error Applying Stash", "Pull Failed"]);
  assert.equal(conflictToasts(continueConflictMutation("rebase"))[0].title, "Rebase Skip/Continue Failed");
  assert.equal(conflictToasts(continueConflictMutation("cherry_pick"))[0].title, "Cherry pick Failed");
});
