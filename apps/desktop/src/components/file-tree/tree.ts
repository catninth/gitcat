import type { CSSProperties } from "react";

import type { FileViewMode } from "../../lib/types";

export interface FileTreeItem<T> {
  id: string;
  path: string;
  data: T;
  status: string;
  statusLabel: string;
  additions?: number | null;
  deletions?: number | null;
}

export type FolderCollapseTarget = "all" | { path: string };

export interface FolderCollapse {
  target: FolderCollapseTarget;
  token: number;
}

export interface FileChangeCounts {
  added: number;
  deleted: number;
  modified: number;
  renamed: number;
}

export const emptyChangeCounts: FileChangeCounts = { added: 0, deleted: 0, modified: 0, renamed: 0 };

interface MutableFolder<T> {
  name: string;
  path: string;
  folders: Map<string, MutableFolder<T>>;
  files: FileTreeItem<T>[];
}

export interface FolderNode<T> {
  kind: "folder";
  name: string;
  path: string;
  count: number;
  changeCounts: FileChangeCounts;
  children: TreeNode<T>[];
}

export interface FileNode<T> {
  kind: "file";
  name: string;
  item: FileTreeItem<T>;
}

export type TreeNode<T> = FolderNode<T> | FileNode<T>;

export const pathCollator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

export function normalizePath(path: string): string {
  return path.replaceAll("\\", "/").replace(/^\/+|\/+$/g, "");
}

function compareNames(left: { name: string }, right: { name: string }): number {
  return pathCollator.compare(left.name, right.name);
}

interface FolderTotals<T> {
  children: TreeNode<T>[];
  count: number;
  changeCounts: FileChangeCounts;
}

export function fileChangeCounts(status: string): FileChangeCounts {
  if (status === "added" || status === "untracked" || status === "copied") {
    return { ...emptyChangeCounts, added: 1 };
  }
  if (status === "deleted") {
    return { ...emptyChangeCounts, deleted: 1 };
  }
  if (status === "renamed") {
    return { ...emptyChangeCounts, renamed: 1 };
  }
  return { ...emptyChangeCounts, modified: 1 };
}

export function sumChangeCounts(...counts: readonly FileChangeCounts[]): FileChangeCounts {
  return counts.reduce<FileChangeCounts>(
    (total, count) => ({
      added: total.added + count.added,
      deleted: total.deleted + count.deleted,
      modified: total.modified + count.modified,
      renamed: total.renamed + count.renamed,
    }),
    emptyChangeCounts,
  );
}

function finalizeFolder<T>(folder: MutableFolder<T>): FolderTotals<T> {
  const folderNodes = [...folder.folders.values()]
    .map((child): FolderNode<T> => {
      const finalized = finalizeFolder(child);
      return {
        kind: "folder",
        name: child.name,
        path: child.path,
        count: finalized.count,
        changeCounts: finalized.changeCounts,
        children: finalized.children,
      };
    })
    .sort(compareNames);
  const fileNodes = [...folder.files]
    .sort((left, right) => pathCollator.compare(normalizePath(left.path), normalizePath(right.path)))
    .map((item): FileNode<T> => ({
      kind: "file",
      name: normalizePath(item.path).split("/").at(-1) ?? item.path,
      item,
    }));

  return {
    children: [...folderNodes, ...fileNodes],
    count: fileNodes.length + folderNodes.reduce((total, child) => total + child.count, 0),
    changeCounts: sumChangeCounts(
      ...fileNodes.map((node) => fileChangeCounts(node.item.status)),
      ...folderNodes.map((child) => child.changeCounts),
    ),
  };
}

export function buildTree<T>(items: readonly FileTreeItem<T>[]): TreeNode<T>[] {
  const root: MutableFolder<T> = {
    name: "",
    path: "",
    folders: new Map(),
    files: [],
  };

  for (const item of items) {
    const segments = normalizePath(item.path).split("/").filter(Boolean);
    if (!segments.length) continue;

    let current = root;
    const folderPath: string[] = [];
    for (const segment of segments.slice(0, -1)) {
      folderPath.push(segment);
      let child = current.folders.get(segment);
      if (!child) {
        child = {
          name: segment,
          path: folderPath.join("/"),
          folders: new Map(),
          files: [],
        };
        current.folders.set(segment, child);
      }
      current = child;
    }
    current.files.push(item);
  }

  return finalizeFolder(root).children;
}

export function orderedFilePaths(paths: readonly string[], mode: FileViewMode): string[] {
  const items = paths.map((path): FileTreeItem<string> => ({
    id: path,
    path,
    data: path,
    status: "modified",
    statusLabel: "M",
  }));
  if (mode === "tree") return collectFolderItems(buildTree(items));
  return items
    .sort((left, right) => pathCollator.compare(normalizePath(left.path), normalizePath(right.path)))
    .map((item) => item.data);
}

export function collectFolderPaths<T>(nodes: readonly TreeNode<T>[], paths: string[] = []): string[] {
  for (const node of nodes) {
    if (node.kind !== "folder") continue;
    paths.push(node.path);
    collectFolderPaths(node.children, paths);
  }
  return paths;
}

export function collectFolderItems<T>(nodes: readonly TreeNode<T>[], items: T[] = []): T[] {
  for (const node of nodes) {
    if (node.kind === "file") items.push(node.item.data);
    else collectFolderItems(node.children, items);
  }
  return items;
}

// Rows are one flat list; the depth drives padding through this custom property.
export function treeIndent(depth: number): CSSProperties {
  return { "--gc-tree-depth": depth } as CSSProperties;
}
