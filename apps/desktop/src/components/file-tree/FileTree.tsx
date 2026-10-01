import { ChevronDown, ChevronRight } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent, ReactNode } from "react";

import { cx } from "../../lib";
import type { FileViewMode } from "../../lib/types";
import { ChangeCountSummary } from "./ChangeCounts";
import { EntryName, RowAction, TreeEntry, TreeRow, fileStatusClass, fileStatusIcon } from "./TreeRow";
import {
  buildTree,
  collectFolderItems,
  collectFolderPaths,
  normalizePath,
  pathCollator,
  treeIndent,
} from "./tree";
import type { FileTreeItem, FolderCollapse, TreeNode } from "./tree";

function collapseTargetPath(target: FolderCollapse["target"]): string {
  return target === "all" ? "" : normalizePath(target.path);
}

function isForcedCollapsed(forced: string | null, path: string): boolean {
  if (forced === null) return false;
  return !forced || path === forced || path.startsWith(`${forced}/`);
}

interface FileTreeProps<T> {
  ariaLabel: string;
  className?: string;
  collapse?: FolderCollapse;
  emptyClassName?: string;
  emptyState: ReactNode;
  items: readonly FileTreeItem<T>[];
  mode: FileViewMode;
  onSelect: (item: T) => void;
  onItemContextMenu?: (item: T, event: ReactMouseEvent) => void;
  onFolderContextMenu?: (folder: { path: string; items: T[] }, event: ReactMouseEvent) => void;
  renderAction?: (item: T) => ReactNode;
  selectedId?: string;
}

export function FileTree<T>({
  ariaLabel,
  className = "",
  collapse,
  emptyClassName = "",
  emptyState,
  items,
  mode,
  onSelect,
  onItemContextMenu,
  onFolderContextMenu,
  renderAction,
  selectedId,
}: FileTreeProps<T>) {
  const sortedItems = useMemo(
    () => [...items].sort((left, right) => pathCollator.compare(normalizePath(left.path), normalizePath(right.path))),
    [items],
  );
  const tree = useMemo(() => buildTree(items), [items]);
  const folderPaths = useMemo(() => collectFolderPaths(tree), [tree]);
  const initialForcedCollapse = () => (collapse ? collapseTargetPath(collapse.target) : null);
  const [folderExpansion, setFolderExpansion] = useState<Map<string, boolean>>(() => new Map());
  const [defaultExpanded, setDefaultExpanded] = useState<boolean | null>(null);
  const [forcedCollapse, setForcedCollapse] = useState<string | null>(initialForcedCollapse);
  const isFolderExpanded = (path: string) => {
    const normalized = normalizePath(path);
    if (isForcedCollapsed(forcedCollapse, normalized)) return false;
    return folderExpansion.get(path) ?? defaultExpanded ?? normalized.split("/").length <= 3;
  };
  const allExpanded = folderPaths.every(isFolderExpanded);
  const collapseTokenRef = useRef(collapse?.token ?? 0);
  const skipSelectionExpandRef = useRef<string | undefined>(undefined);
  const forcedCollapseRef = useRef<string | null>(initialForcedCollapse());

  const forceCollapse = (target: string) => {
    const previous = forcedCollapseRef.current;
    forcedCollapseRef.current = target;
    setForcedCollapse(target);
    setFolderExpansion((current) => {
      const next = new Map(current);
      if (previous) next.set(previous, false);
      if (target) next.set(target, false);
      return next;
    });
  };

  const releaseForcedCollapse = () => {
    forcedCollapseRef.current = null;
    setForcedCollapse(null);
  };

  useEffect(() => {
    if (!collapse || collapseTokenRef.current === collapse.token) return;
    collapseTokenRef.current = collapse.token;
    forceCollapse(collapseTargetPath(collapse.target));
    if (collapse.target === "all") {
      skipSelectionExpandRef.current = selectedId;
      setFolderExpansion(new Map());
      setDefaultExpanded(false);
    }
  }, [collapse, selectedId]);

  useEffect(() => {
    if (mode !== "tree" || !selectedId) return;
    if (skipSelectionExpandRef.current === selectedId) return;
    const selected = items.find((item) => item.id === selectedId);
    if (!selected) return;
    if (isForcedCollapsed(forcedCollapseRef.current, normalizePath(selected.path))) {
      skipSelectionExpandRef.current = selectedId;
      return;
    }
    const segments = normalizePath(selected.path).split("/").slice(0, -1);
    const ancestors = segments.map((_, index) => segments.slice(0, index + 1).join("/"));
    setFolderExpansion((current) => {
      if (ancestors.every((path) => current.get(path) === true)) return current;
      const next = new Map(current);
      ancestors.forEach((path) => next.set(path, true));
      return next;
    });
    requestAnimationFrame(() => {
      document.querySelector<HTMLElement>(`[data-selected="true"][data-file-tree-id="${CSS.escape(selectedId)}"]`)?.scrollIntoView({ block: "nearest" });
    });
  }, [items, mode, selectedId]);

  const toggleFolder = (path: string) => {
    const expanded = isFolderExpanded(path);
    const prefix = `${path}/`;
    releaseForcedCollapse();
    setFolderExpansion((current) => {
      const next = new Map(current);
      next.set(path, !expanded);
      folderPaths.filter((candidate) => candidate.startsWith(prefix)).forEach((candidate) => next.set(candidate, !expanded));
      return next;
    });
  };

  const toggleAll = () => {
    forcedCollapseRef.current = null;
    setForcedCollapse(null);
    setFolderExpansion(new Map());
    setDefaultExpanded(!allExpanded);
  };

  const renderFile = (item: FileTreeItem<T>, label: string, depth: number) => {
    const StatusIcon = fileStatusIcon(item.status);
    const selected = selectedId === item.id;
    const unmerged = item.status === "unmerged";
    return (
      <TreeRow
        data-file-tree-id={item.id}
        data-selected={selected ? "true" : undefined}
        key={item.id}
        onContextMenu={onItemContextMenu ? (event) => { event.preventDefault(); onItemContextMenu(item.data, event); } : undefined}
        role="listitem"
        selected={selected}
        unmerged={unmerged}
      >
        <TreeEntry
          aria-current={selected ? "true" : undefined}
          className={cx("flex-1", renderAction && unmerged && "pr-25.5")}
          depth={depth}
          onClick={() => onSelect(item.data)}
          title={item.path}
        >
          <b aria-label={item.statusLabel} className={fileStatusClass(item.status)} title={item.statusLabel}>
            <StatusIcon aria-hidden="true" size={15} strokeWidth={2.6} />
          </b>
          <EntryName>{label}</EntryName>
          {item.binary ? <small className="text-[11px] text-warning">binary</small> : null}
        </TreeEntry>
        {renderAction ? <RowAction pinned={unmerged}>{renderAction(item.data)}</RowAction> : null}
      </TreeRow>
    );
  };

  const renderNodes = (nodes: readonly TreeNode<T>[], depth: number): ReactNode => nodes.map((node) => {
    if (node.kind === "file") return renderFile(node.item, node.name, depth);

    const expanded = isFolderExpanded(node.path);
    return (
      <div key={node.path} role="listitem">
        <TreeEntry
          aria-expanded={expanded}
          className="w-full rounded-[3px] hover:bg-row-hover text-[color-mix(in_srgb,var(--gc-muted)_88%,var(--gc-text))] [&>svg:first-child]:-mr-1 [&>svg]:shrink-0"
          depth={depth}
          onClick={() => toggleFolder(node.path)}
          onContextMenu={onFolderContextMenu ? (event) => {
            event.preventDefault();
            onFolderContextMenu({ path: node.path, items: collectFolderItems(node.children) }, event);
          } : undefined}
          title={node.path}
        >
          {expanded ? <ChevronDown aria-hidden="true" size={16} /> : <ChevronRight aria-hidden="true" size={16} />}
          <EntryName grow={false}>{node.name}</EntryName>
          {!expanded ? <ChangeCountSummary counts={node.changeCounts} /> : null}
        </TreeEntry>
        {expanded ? (
          <div className="flex min-w-0 flex-col gap-0.75" role="list" style={treeIndent(depth)}>
            {renderNodes(node.children, depth + 1)}
          </div>
        ) : null}
      </div>
    );
  });

  if (!items.length) {
    return (
      <div className={cx("flex min-h-14.5 items-center justify-center gap-1.5 text-[12px] text-muted", emptyClassName)}>
        {emptyState}
      </div>
    );
  }

  return (
    <div className={cx("min-h-0 min-w-0 overflow-auto", className)}>
      {mode === "tree" && folderPaths.length ? (
        <button
          className="flex min-h-7.25 cursor-pointer items-center bg-transparent px-2 text-[13px] text-[color-mix(in_srgb,var(--gc-text)_83%,var(--gc-muted))] hover:text-accent"
          onClick={toggleAll}
          type="button"
        >
          {allExpanded ? "Collapse all" : "Expand all"}
        </button>
      ) : null}
      <div aria-label={ariaLabel} className="flex min-w-0 flex-col gap-0.75" role="list">
        {mode === "tree"
          ? renderNodes(tree, 0)
          : sortedItems.map((item) => renderFile(item, normalizePath(item.path), 0))}
      </div>
    </div>
  );
}
