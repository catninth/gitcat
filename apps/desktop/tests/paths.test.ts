import assert from "node:assert/strict";
import test from "node:test";

import { findRepositoryTab, normalizePersistedState } from "../src/app/workspace";
import { comparablePath, joinPath, parentDirectory, samePath } from "../src/lib/paths";
import type { PersistedState, RepositoryGroup, RepositoryTab, WorkspaceState } from "../src/lib/types";

function tab(
  id: string,
  repository_path: string,
  kind: RepositoryTab["kind"] = "repository",
): RepositoryTab {
  return { id, repository_path, display_name: id, order: 0, kind };
}

function workspace(ungrouped: RepositoryTab[], groups: RepositoryGroup[] = []): WorkspaceState {
  return { version: 2, ungrouped_tabs: ungrouped, groups, active_tab_id: null };
}

test("a Windows path is the same folder however it is spelled", () => {
  assert.equal(samePath("C:\\Users\\dev\\repo", "c:/users/dev/repo"), true);
  assert.equal(samePath("C:\\Users\\dev\\repo\\", "C:\\Users\\dev\\repo"), true);
  assert.equal(samePath("\\\\build\\Share\\repo", "\\\\BUILD\\share\\repo"), true);
  assert.equal(samePath("C:\\Users\\dev\\repo", "C:\\Users\\dev\\repo\\src"), false);
});

test("case is part of the name outside Windows", () => {
  assert.equal(samePath("/home/dev/repo", "/home/dev/repo/"), true);
  assert.equal(samePath("/home/dev/repo", "/home/dev/Repo"), false);
});

test("an empty path names no folder", () => {
  // Start tabs carry an empty path; two of them are not the same repository.
  assert.equal(samePath("", ""), false);
  assert.equal(samePath("   ", "C:\\repo"), false);
});

test("a drive root keeps its separator", () => {
  assert.equal(comparablePath("C:\\"), "c:/");
  assert.equal(comparablePath("/"), "/");
});

test("the tab holding a repository is found however the folder was spelled", () => {
  const state = workspace([
    tab("start", "", "start"),
    tab("a", "C:\\Users\\dev\\alpha"),
    tab("b", "C:\\Users\\dev\\beta"),
  ]);
  assert.equal(findRepositoryTab(state, "c:/users/dev/alpha")?.id, "a");
  assert.equal(findRepositoryTab(state, "C:\\Users\\dev\\beta\\")?.id, "b");
  assert.equal(findRepositoryTab(state, "C:\\Users\\dev\\gamma"), undefined);
  assert.equal(findRepositoryTab(state, ""), undefined);
});

test("tabs saved in a repository folder rejoin the single row after the unfiled ones", () => {
  const group: RepositoryGroup = {
    id: "group",
    name: "Work",
    collapsed: true,
    tabs: [tab("c", "C:\\c"), tab("d", "C:\\d")],
  };
  const saved = { workspace: workspace([tab("a", "C:\\a"), tab("b", "C:\\b")], [group]) } as PersistedState;
  const { workspace: restored } = normalizePersistedState(saved);
  assert.deepEqual(restored.groups, []);
  assert.deepEqual(
    restored.ungrouped_tabs.map((item) => [item.id, item.order]),
    [["a", 0], ["b", 1], ["c", 2], ["d", 3]],
  );
});

test("path helpers keep their separator", () => {
  assert.equal(joinPath("C:\\Users\\dev", "repo"), "C:\\Users\\dev\\repo");
  assert.equal(parentDirectory("C:\\Users\\dev\\repo"), "C:\\Users\\dev");
});
