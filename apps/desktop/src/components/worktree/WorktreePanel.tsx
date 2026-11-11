import { useEffect, useMemo, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { createPortal } from "react-dom";

import { conflictSideLabels, type ConflictContext } from "../../lib/conflicts";
import type { ConflictResolution, OperationProgress, OperationSource, RepositoryOperationState, StatusEntry, WorktreeStatus } from "../../lib/types";
import { ContextMenu, type ContextAction } from "../ContextMenu";
import { FileTreeControls } from "../file-tree";
import type { FileTreeItem, FileViewMode, FolderCollapse, FolderCollapseTarget } from "../file-tree";
import { SidePanel } from "../ui";
import { CommitForm } from "./CommitForm";
import type { CommitDraft } from "./CommitForm";
import { ConflictBulkMenu } from "./ConflictBulkMenu";
import { AutostashNotice, ConflictHeader } from "./ConflictHeader";
import { OperationActions } from "./OperationActions";
import { StatusSection } from "./StatusSection";
import { WorktreeHeader } from "./WorktreeHeader";

export type { CommitDraft };

const STATUS_LABEL: Record<string, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  type_changed: "T",
  unmerged: "U",
  untracked: "?",
  ignored: "!",
};

interface WorktreePanelProps {
  status: WorktreeStatus;
  busy: boolean;
  collapse?: FolderCollapse & { staged: boolean };
  fileViewMode: FileViewMode;
  onFileViewModeChange: (mode: FileViewMode) => void;
  onStage: (paths: string[], collapse?: FolderCollapseTarget) => void;
  onUnstage: (paths: string[], collapse?: FolderCollapseTarget) => void;
  onDiscard: (paths: string[]) => void;
  onStashFile: (paths: string[]) => void;
  onIgnore: (patterns: string[]) => void;
  onCreatePatch: (paths: string[], staged: boolean) => void;
  onOpenFolder: (path: string) => void;
  onRevealFile: (path: string) => void;
  onOpenDiff: (entry: StatusEntry, staged: boolean) => void;
  onCommit: (message: string, amend: boolean, signoff: boolean) => Promise<boolean>;
  onResolveConflict: (entry: StatusEntry, resolution: ConflictResolution) => void;
  onResolveConflicts: (paths: string[], resolution: ConflictResolution) => void;
  onOpenConflict: (entry: StatusEntry) => void;
  onAutoResolveConflicts: () => void;
  /** `message` replaces the one Git prepared; `null` keeps Git's. */
  onContinueOperation: (message: string | null) => Promise<boolean>;
  onSkipOperation: () => void;
  onAbortOperation: () => void;
  commitKeybind?: string;
  repositoryRoot: string;
  selectedFile?: { path: string; staged: boolean } | null;
  operation: RepositoryOperationState;
  operationProgress: OperationProgress | null;
  operationSource: OperationSource | null;
  operationMessage?: string | null;
  operationAutostash?: boolean;
  /** Short HEAD commit id, for naming the current side of a stash apply. */
  headShort?: string | null;
  branchName: string;
  draft: CommitDraft;
  onDraftChange: (draft: CommitDraft) => void;
}

const REVEAL_LABEL = typeof navigator === "undefined"
  ? "Show in file manager"
  : /Windows/i.test(navigator.userAgent)
    ? "Show in Explorer"
    : /Mac/i.test(navigator.userAgent) ? "Reveal in Finder" : "Show in file manager";

// Joins a repository-relative path onto the root with the root's own separator,
// so the copied path pastes straight into the platform's file manager or shell.
export function absoluteFilePath(root: string, path: string): string {
  const separator = root.includes("\\") ? "\\" : "/";
  const trimmed = root.replace(/[\\/]+$/, "");
  return `${trimmed}${separator}${path.replace(/[\\/]/g, separator)}`;
}

export function buildCommitMessage(draft: CommitDraft): string {
  const summary = draft.message.trim();
  const description = draft.description.trim();
  return description ? `${summary}\n\n${description}` : summary;
}

function toTreeItems(entries: StatusEntry[], side: "index" | "worktree"): FileTreeItem<StatusEntry>[] {
  return entries.map((entry) => {
    const change = entry.conflicted ? "unmerged" : (entry[side] ?? "modified");
    const stats = side === "index" ? entry.index_stats : entry.worktree_stats;
    return {
      id: entry.path,
      path: entry.path,
      data: entry,
      status: change,
      statusLabel: STATUS_LABEL[change] ?? "M",
      additions: stats?.additions ?? null,
      deletions: stats?.deletions ?? null,
    };
  });
}

export function WorktreePanel({
  status,
  busy,
  collapse,
  fileViewMode,
  onFileViewModeChange,
  onStage,
  onUnstage,
  onDiscard,
  onStashFile,
  onIgnore,
  onCreatePatch,
  onOpenFolder,
  onRevealFile,
  onOpenDiff,
  onCommit,
  onResolveConflict,
  onResolveConflicts,
  onOpenConflict,
  onAutoResolveConflicts,
  onContinueOperation,
  onSkipOperation,
  onAbortOperation,
  commitKeybind = "Ctrl+Enter",
  repositoryRoot,
  selectedFile,
  operation,
  operationProgress,
  operationSource,
  operationMessage = null,
  operationAutostash = false,
  headShort = null,
  branchName,
  draft,
  onDraftChange,
}: WorktreePanelProps) {
  const [unstagedOpen, setUnstagedOpen] = useState(true);
  const [stagedOpen, setStagedOpen] = useState(true);
  const [conflictsOpen, setConflictsOpen] = useState(true);
  const staged = useMemo(() => status.entries.filter((entry) => entry.index && !entry.conflicted), [status.entries]);
  const unstaged = useMemo(() => status.entries.filter((entry) => entry.worktree && !entry.conflicted), [status.entries]);
  const stageable = unstaged;
  const conflicts = useMemo(() => status.entries.filter((entry) => entry.conflicted), [status.entries]);
  const operationPending = operation !== "normal" && operation !== "bisect";
  // GitKraken's rule: while anything is conflicted the panel is about the
  // conflicts and what is already resolved, nothing else. Unstaged and
  // untracked files come back the moment the last conflict is resolved.
  const inConflict = conflicts.length > 0;
  const conflictContext = useMemo<ConflictContext>(
    () => ({ operation, branchName, source: operationSource, headShort }),
    [branchName, headShort, operation, operationSource],
  );
  const labels = useMemo(() => conflictSideLabels(conflictContext), [conflictContext]);
  const canCommit = operation === "normal"
    && !busy
    && Boolean(draft.message.trim())
    && Boolean(staged.length || draft.amend)
    && !conflicts.length;

  // The message Git prepared for the operation fills an empty draft once. A
  // draft the user wrote is never overwritten, and one they emptied stays
  // empty; a prefilled message nobody touched leaves with the operation.
  useEffect(() => {
    const empty = !draft.message.trim() && !draft.description.trim();
    // A rebase prepares a new message for every commit it replays; one left
    // over from the previous step must not be committed with the next.
    const untouched = draft.prefilledFrom !== undefined && buildCommitMessage(draft) === draft.prefilledFrom;
    if (operationMessage) {
      if ((empty || untouched) && draft.prefilledFrom !== operationMessage) {
        const [summary, ...rest] = operationMessage.split("\n");
        onDraftChange({
          ...draft,
          message: summary,
          description: rest.join("\n").trim(),
          amend: false,
          prefilledFrom: operationMessage,
        });
      }
    } else if (draft.prefilledFrom) {
      onDraftChange(untouched
        ? { ...draft, message: "", description: "", prefilledFrom: undefined }
        : { ...draft, prefilledFrom: undefined });
    }
  }, [draft, onDraftChange, operationMessage]);
  const stagedItems = useMemo(() => toTreeItems(staged, "index"), [staged]);
  const unstagedItems = useMemo(() => toTreeItems(unstaged, "worktree"), [unstaged]);
  const conflictItems = useMemo(() => toTreeItems(conflicts, "worktree"), [conflicts]);

  const [fileMenu, setFileMenu] = useState<{ entry: StatusEntry; staged: boolean; x: number; y: number } | null>(null);
  const [folderMenu, setFolderMenu] = useState<{ path: string; entries: StatusEntry[]; conflicts: StatusEntry[]; staged: boolean; x: number; y: number } | null>(null);

  const openFileMenu = (entry: StatusEntry, staged: boolean, event: ReactMouseEvent) => {
    if (entry.conflicted) return;
    setFolderMenu(null);
    setFileMenu({ entry, staged, x: event.clientX, y: event.clientY });
  };

  const openFolderMenu = (folder: { path: string; items: StatusEntry[] }, staged: boolean, event: ReactMouseEvent) => {
    setFileMenu(null);
    setFolderMenu({
      path: folder.path,
      entries: folder.items.filter((entry) => !entry.conflicted),
      conflicts: folder.items.filter((entry) => entry.conflicted),
      staged,
      x: event.clientX,
      y: event.clientY,
    });
  };

  const fileMenuActions = useMemo<ContextAction[]>(() => {
    if (!fileMenu) return [];
    const base = fileMenu.entry.path.replaceAll("\\", "/");
    const name = base.split("/").at(-1) ?? base;
    const dot = name.lastIndexOf(".");
    const ext = dot > 0 ? name.slice(dot + 1) : "";
    const folder = base.includes("/") ? base.slice(0, base.lastIndexOf("/")) : "";
    const ignore: ContextAction[] = [{ id: "ignore-file", label: "Ignore this file" }];
    if (ext) ignore.push({ id: "ignore-ext", label: `Ignore all *.${ext} files` });
    if (folder) ignore.push({ id: "ignore-folder", label: `Ignore ${folder}/ folder` });
    const removed = (fileMenu.staged ? fileMenu.entry.index : fileMenu.entry.worktree) === "deleted";
    return [
      fileMenu.staged ? { id: "unstage", label: "Unstage" } : { id: "stage", label: "Stage" },
      { id: "discard", label: "Discard changes", danger: true },
      { id: "ignore", label: "Ignore", submenu: ignore },
      { id: "stash", label: "Stash file" },
      { id: "copy", label: "Copy file path", separatorBefore: true },
      { id: "reveal", label: REVEAL_LABEL, disabled: removed },
      { id: "patch", label: "Create patch from file changes" },
    ];
  }, [fileMenu]);

  const executeFileAction = (id: string) => {
    if (!fileMenu) return;
    const { entry, staged } = fileMenu;
    const base = entry.path.replaceAll("\\", "/");
    const name = base.split("/").at(-1) ?? base;
    const dot = name.lastIndexOf(".");
    const ext = dot > 0 ? name.slice(dot + 1) : "";
    const folder = base.includes("/") ? base.slice(0, base.lastIndexOf("/")) : "";
    switch (id) {
      case "stage": onStage([entry.path]); break;
      case "unstage": onUnstage([entry.path]); break;
      case "discard": onDiscard([entry.path]); break;
      case "stash": onStashFile([entry.path]); break;
      case "ignore-file": onIgnore([base]); break;
      case "ignore-ext": if (ext) onIgnore([`*.${ext}`]); break;
      case "ignore-folder": if (folder) onIgnore([`${folder}/`]); break;
      case "copy": void navigator.clipboard?.writeText(absoluteFilePath(repositoryRoot, entry.path)); break;
      case "reveal": onRevealFile(entry.path); break;
      case "patch": onCreatePatch([entry.path], staged); break;
    }
    setFileMenu(null);
  };

  const folderMenuActions = useMemo<ContextAction[]>(() => {
    if (!folderMenu) return [];
    const empty = !folderMenu.entries.length;
    const conflictCount = folderMenu.conflicts.length;
    const conflictActions: ContextAction[] = conflictCount
      ? [
        {
          id: "conflicts_mark_resolved",
          label: `Mark ${conflictCount} conflict${conflictCount === 1 ? "" : "s"} in folder resolved`,
          disabled: busy,
          separatorBefore: true,
        },
        { id: "conflicts_ours", label: `Take current (${labels.ours}) for folder`, disabled: busy },
        { id: "conflicts_theirs", label: `Take incoming (${labels.theirs}) for folder`, disabled: busy },
      ]
      : [];
    return [
      folderMenu.staged
        ? { id: "unstage", label: "Unstage folder", disabled: busy || empty }
        : { id: "stage", label: "Stage folder", disabled: busy || empty },
      { id: "discard", label: "Discard all changes in folder", danger: true, disabled: busy || empty },
      { id: "ignore", label: `Ignore all files in '${folderMenu.path}/'`, disabled: busy },
      { id: "stash", label: "Stash folder", disabled: busy || empty },
      { id: "patch", label: "Create Patch from changes in directory", disabled: empty },
      ...conflictActions,
      { id: "open", label: "Open folder", separatorBefore: true },
    ];
  }, [busy, folderMenu, labels]);

  const executeFolderAction = (id: string) => {
    if (!folderMenu) return;
    const { conflicts: folderConflicts, entries, path, staged } = folderMenu;
    const paths = entries.map((entry) => entry.path);
    const conflictPaths = folderConflicts.map((entry) => entry.path);
    switch (id) {
      case "conflicts_mark_resolved": onResolveConflicts(conflictPaths, "mark_resolved"); break;
      case "conflicts_ours": onResolveConflicts(conflictPaths, "ours"); break;
      case "conflicts_theirs": onResolveConflicts(conflictPaths, "theirs"); break;
      case "stage": onStage(paths, { path }); break;
      case "unstage": onUnstage(paths, { path }); break;
      case "discard": onDiscard(paths); break;
      case "stash": onStashFile(paths); break;
      case "ignore": onIgnore([`${path}/`]); break;
      case "patch": onCreatePatch(paths, staged); break;
      case "open": onOpenFolder(path); break;
    }
    setFolderMenu(null);
  };

  const submit = async () => {
    if (!canCommit) return;
    if (await onCommit(buildCommitMessage(draft), draft.amend, draft.signoff)) {
      onDraftChange({ ...draft, message: "", description: "", amend: false, prefilledFrom: undefined });
    }
  };

  const continueOperation = async () => {
    const message = buildCommitMessage(draft);
    if (await onContinueOperation(message || null)) {
      onDraftChange({ ...draft, message: "", description: "", amend: false, prefilledFrom: undefined });
    }
  };

  useEffect(() => {
    const submitFromKeybind = () => { void submit(); };
    window.addEventListener("gitcat:commit", submitFromKeybind);
    return () => window.removeEventListener("gitcat:commit", submitFromKeybind);
  });

  return (
    <SidePanel aria-label="Working tree">
      {inConflict ? (
        <ConflictHeader context={conflictContext} />
      ) : (
        <WorktreeHeader
          branchName={branchName}
          busy={busy}
          changeCount={status.entries.length}
          clean={status.clean}
          onDiscardAll={() => onDiscard(status.entries.map((entry) => entry.path))}
          stashCount={status.stash_count}
        />
      )}

      {operationAutostash ? <AutostashNotice /> : null}

      <FileTreeControls mode={fileViewMode} onModeChange={onFileViewModeChange} />

      {inConflict ? (
        <StatusSection
          actionLabel="Mark All Resolved"
          actionTone="conflict"
          busy={busy}
          conflictLabels={labels}
          headerAction={(
            <ConflictBulkMenu
              busy={busy}
              labels={labels}
              onAutoResolveConflicts={onAutoResolveConflicts}
              onResolveAllConflicts={(resolution) => onResolveConflicts(conflicts.map((entry) => entry.path), resolution)}
            />
          )}
          items={conflictItems}
          label="Conflicted Files"
          onAction={() => onResolveConflicts(conflicts.map((entry) => entry.path), "mark_resolved")}
          onEntryAction={(entry) => onResolveConflicts([entry.path], "mark_resolved")}
          onFolderContextMenu={(folder, event) => openFolderMenu(folder, false, event)}
          onOpenConflict={onOpenConflict}
          onOpenDiff={(entry) => onOpenDiff(entry, false)}
          onResolveConflict={onResolveConflict}
          onToggle={() => setConflictsOpen((open) => !open)}
          open={conflictsOpen}
          selectedId={selectedFile && !selectedFile.staged ? selectedFile.path : undefined}
          viewMode={fileViewMode}
        />
      ) : null}

      {inConflict ? null : (
        <StatusSection
          actionLabel="Stage all"
          actionDisabled={!stageable.length}
          busy={busy}
          collapse={collapse && !collapse.staged ? collapse : undefined}
          items={unstagedItems}
          label="Unstaged Files"
          onAction={() => onStage(stageable.map((entry) => entry.path), "all")}
          onEntryAction={(entry) => onStage([entry.path])}
          onItemContextMenu={(entry, event) => openFileMenu(entry, false, event)}
          onFolderContextMenu={(folder, event) => openFolderMenu(folder, false, event)}
          onOpenDiff={(entry) => onOpenDiff(entry, false)}
          onToggle={() => setUnstagedOpen((open) => !open)}
          open={unstagedOpen}
          plus
          selectedId={selectedFile && !selectedFile.staged ? selectedFile.path : undefined}
          viewMode={fileViewMode}
        />
      )}

      <StatusSection
        actionLabel="Unstage all"
        actionPriority
        busy={busy}
        collapse={collapse?.staged ? collapse : undefined}
        items={stagedItems}
        label={inConflict ? "Resolved Files" : "Staged Files"}
        onAction={() => onUnstage(staged.map((entry) => entry.path), "all")}
        onEntryAction={(entry) => onUnstage([entry.path])}
        onItemContextMenu={(entry, event) => openFileMenu(entry, true, event)}
        onFolderContextMenu={(folder, event) => openFolderMenu(folder, true, event)}
        onOpenDiff={(entry) => onOpenDiff(entry, true)}
        onToggle={() => setStagedOpen((open) => !open)}
        open={stagedOpen}
        selectedId={selectedFile?.staged ? selectedFile.path : undefined}
        viewMode={fileViewMode}
      />

      <CommitForm
        actions={operationPending || inConflict ? (
          <OperationActions
            busy={busy}
            conflictCount={conflicts.length}
            onAbort={onAbortOperation}
            onCommit={() => void submit()}
            onContinue={() => void continueOperation()}
            onSkip={onSkipOperation}
            operation={operation}
            stagedCount={staged.length}
          />
        ) : undefined}
        busy={busy}
        canCommit={canCommit}
        commitKeybind={commitKeybind}
        draft={draft}
        notice={operation === "rebase" && operationProgress
          ? `Rebasing commit ${operationProgress.current} out of ${operationProgress.total}`
          : undefined}
        onDraftChange={onDraftChange}
        // The keyboard shortcut only ever steps forward: skipping or
        // aborting is a button press, never a reflex.
        onSubmit={() => {
          if (!operationPending) void submit();
          else if (!inConflict && !busy) void continueOperation();
        }}
        options={!operationPending}
        stagedCount={staged.length}
      />

      {fileMenu ? createPortal(
        <ContextMenu
          actions={fileMenuActions}
          onAction={executeFileAction}
          onClose={() => setFileMenu(null)}
          x={fileMenu.x}
          y={fileMenu.y}
        />,
        document.body,
      ) : null}

      {folderMenu ? createPortal(
        <ContextMenu
          actions={folderMenuActions}
          onAction={executeFolderAction}
          onClose={() => setFolderMenu(null)}
          x={folderMenu.x}
          y={folderMenu.y}
        />,
        document.body,
      ) : null}
    </SidePanel>
  );
}
