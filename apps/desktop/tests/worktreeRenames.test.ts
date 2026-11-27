import assert from "node:assert/strict";
import test from "node:test";

import {
  optimisticWorktreeSnapshot,
  renameSource,
  withAllRenameSources,
  withRenameSources,
} from "../src/app/worktree";
import type { RepositorySnapshot, StatusEntry } from "../src/lib/types";

const unstagedRename: StatusEntry = {
  path: "src/new.cs",
  old_path: "src/old.cs",
  worktree: "renamed",
  conflicted: false,
  submodule: false,
};

const stagedRenameWithEdit: StatusEntry = {
  path: "src/moved.cs",
  old_path: "src/origin.cs",
  index: "renamed",
  worktree: "modified",
  conflicted: false,
  submodule: false,
};

const plain: StatusEntry = { path: "README.md", worktree: "modified", conflicted: false, submodule: false };

const entries = [unstagedRename, stagedRenameWithEdit, plain];

test("a rename source belongs only to the side that reports the rename", () => {
  assert.equal(renameSource(unstagedRename, false), "src/old.cs");
  assert.equal(renameSource(unstagedRename, true), undefined);
  assert.equal(renameSource(stagedRenameWithEdit, true), "src/origin.cs");
  // The unstaged half of a staged rename is an ordinary edit of the new path.
  assert.equal(renameSource(stagedRenameWithEdit, false), undefined);
  assert.equal(renameSource(plain, false), undefined);
});

test("staging an unstaged rename sends both halves", () => {
  assert.deepEqual(withRenameSources(entries, ["src/new.cs", "README.md"], false), [
    "src/new.cs",
    "README.md",
    "src/old.cs",
  ]);
  // Staging the edit on top of a staged rename must not name the source,
  // which no longer exists anywhere Git could add it from.
  assert.deepEqual(withRenameSources(entries, ["src/moved.cs"], false), ["src/moved.cs"]);
});

test("unstaging a staged rename sends both halves", () => {
  assert.deepEqual(withRenameSources(entries, ["src/moved.cs"], true), ["src/moved.cs", "src/origin.cs"]);
});

test("discard and stash take a rename whole from either side", () => {
  assert.deepEqual(withAllRenameSources(entries, ["src/new.cs", "src/moved.cs"]), [
    "src/new.cs",
    "src/moved.cs",
    "src/old.cs",
    "src/origin.cs",
  ]);
});

test("optimistic stage and unstage keep the rename row and its source", () => {
  const snapshot = { status: { clean: false, ahead: 0, behind: 0, stash_count: 0, entries } } as unknown as RepositorySnapshot;

  const staged = optimisticWorktreeSnapshot(snapshot, "stage", ["src/new.cs"]);
  const stagedEntry = staged.status.entries.find((entry) => entry.path === "src/new.cs");
  assert.equal(stagedEntry?.index, "renamed");
  assert.equal(stagedEntry?.worktree, undefined);
  assert.equal(stagedEntry?.old_path, "src/old.cs");

  const unstaged = optimisticWorktreeSnapshot(snapshot, "unstage", ["src/moved.cs"]);
  const unstagedEntry = unstaged.status.entries.find((entry) => entry.path === "src/moved.cs");
  assert.equal(unstagedEntry?.index, undefined);
  assert.equal(unstagedEntry?.worktree, "renamed");
  assert.equal(unstagedEntry?.old_path, "src/origin.cs");
});
