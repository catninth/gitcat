import { Check, ChevronDown, ChevronRight } from "lucide-react";
import type { MouseEvent as ReactMouseEvent, ReactNode } from "react";

import { cx } from "../../lib";
import type { ConflictResolution, RepositoryOperationState, StatusEntry } from "../../lib/types";
import { FileTree } from "../file-tree";
import type { FileTreeItem, FileViewMode, FolderCollapse } from "../file-tree";
import { ConflictQuickActions } from "./ConflictQuickActions";
import { BulkButton, StageButton } from "./StageButtons";
import type { BulkButtonTone } from "./StageButtons";

export function StatusSection({
  label,
  items,
  actionLabel,
  actionDisabled = false,
  actionPriority = false,
  actionTone,
  headerAction,
  branchName = "current branch",
  busy,
  collapse,
  onAction,
  onEntryAction,
  onItemContextMenu,
  onFolderContextMenu,
  onOpenDiff,
  onResolveConflict,
  onOpenConflict,
  onToggle,
  open,
  operation = "normal",
  plus = false,
  selectedId,
  viewMode,
}: {
  label: string;
  items: FileTreeItem<StatusEntry>[];
  actionLabel: string;
  actionDisabled?: boolean;
  actionPriority?: boolean;
  actionTone?: BulkButtonTone;
  headerAction?: ReactNode;
  branchName?: string;
  busy: boolean;
  collapse?: FolderCollapse;
  onAction: () => void;
  onEntryAction: (entry: StatusEntry) => void;
  onItemContextMenu?: (entry: StatusEntry, event: ReactMouseEvent) => void;
  onFolderContextMenu?: (folder: { path: string; items: StatusEntry[] }, event: ReactMouseEvent) => void;
  onOpenDiff: (entry: StatusEntry) => void;
  onResolveConflict?: (entry: StatusEntry, resolution: ConflictResolution) => void;
  onOpenConflict?: (entry: StatusEntry) => void;
  onToggle: () => void;
  open: boolean;
  operation?: RepositoryOperationState;
  plus?: boolean;
  selectedId?: string;
  viewMode: FileViewMode;
}) {
  return (
    <section className={cx("gc-no-select border-b border-border", open && "flex min-h-24 flex-1 flex-col")}>
      <header className="flex h-10.5 flex-[0_0_auto] items-center justify-between pl-1.25 pr-2">
        <button
          aria-expanded={open}
          className="flex min-w-0 flex-1 cursor-pointer items-center gap-1 bg-transparent px-0.75 text-left text-[12px] font-bold uppercase tracking-[0.04em] text-muted hover:text-foreground"
          onClick={onToggle}
          type="button"
        >
          {open ? <ChevronDown aria-hidden="true" size={15} /> : <ChevronRight aria-hidden="true" size={15} />}
          <span>{label} <b className="ml-0.75 text-foreground">{items.length}</b></span>
        </button>
        <span className="flex shrink-0 items-center gap-1">
          {actionDisabled || !items.length ? null : (
            <BulkButton busy={busy} label={actionLabel} onClick={onAction} priority={actionPriority} tone={actionTone} />
          )}
          {items.length ? headerAction : null}
        </span>
      </header>
      {open ? (
        <FileTree
          ariaLabel={`${label} files`}
          className="min-h-0 flex-1 px-1.25 pb-1.75"
          collapse={collapse}
          emptyClassName="min-h-0 flex-1"
          emptyState={<><Check aria-hidden="true" size={14} /> Nothing here</>}
          items={items}
          mode={viewMode}
          onItemContextMenu={onItemContextMenu}
          onFolderContextMenu={onFolderContextMenu}
          // A conflicted file opens the three-way editor: its worktree copy is
          // a file full of conflict markers, which is not a diff worth reading.
          onSelect={(entry) => (entry.conflicted && onOpenConflict ? onOpenConflict(entry) : onOpenDiff(entry))}
          renderAction={(entry) => (
            entry.conflicted && onResolveConflict ? (
              <ConflictQuickActions
                branchName={branchName}
                busy={busy}
                entry={entry}
                onOpen={() => onOpenConflict?.(entry)}
                onResolve={onResolveConflict}
                operation={operation}
              />
            ) : (
              <StageButton busy={busy} onClick={() => onEntryAction(entry)} path={entry.path} plus={plus} />
            )
          )}
          selectedId={selectedId}
        />
      ) : null}
    </section>
  );
}
