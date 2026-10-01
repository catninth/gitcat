import { ArrowRight, Copy, FileType, Minus, Plus, TriangleAlert } from "lucide-react";
import { PencilFilled } from "../ui/PencilFilled";
import type { LucideIcon } from "lucide-react";
import type { ComponentPropsWithRef } from "react";

import { cx } from "../../lib";
import { treeIndent } from "./tree";

const FILE_STATUS_TONE: Record<string, string> = {
  added: "text-success",
  untracked: "text-success",
  deleted: "text-danger",
  unmerged: "text-warning",
  modified: "text-warning",
  type_changed: "text-warning",
  renamed: "text-warning",
  copied: "text-warning",
};

const FILE_STATUS_ICON: Record<string, LucideIcon> = {
  added: Plus,
  untracked: Plus,
  modified: PencilFilled,
  deleted: Minus,
  renamed: ArrowRight,
  copied: Copy,
  type_changed: FileType,
  unmerged: TriangleAlert,
};

export function fileStatusIcon(status: string): LucideIcon {
  return FILE_STATUS_ICON[status] ?? PencilFilled;
}

export function fileStatusClass(status: string): string {
  return cx(
    "grid size-4.75 place-items-center font-mono text-[10px] font-[750] leading-none",
    FILE_STATUS_TONE[status] ?? "text-muted",
  );
}

// A conflicted row keeps its warning tint over the selection colour: an unmerged
// path needs attention before anything else in the list, and a conflict is a
// decision to make rather than a loss to warn about.
export function TreeRow({ children, selected, unmerged, ...props }: ComponentPropsWithRef<"div"> & {
  selected?: boolean;
  unmerged?: boolean;
}) {
  return (
    <div
      className={cx(
        "group/row relative flex min-w-0 items-center rounded-[3px]",
        unmerged ? "bg-warning/8" : selected ? "bg-row-selected" : "hover:bg-row-hover",
      )}
      {...props}
    >
      {children}
    </div>
  );
}

export function TreeEntry({ className = "", depth, ...props }: ComponentPropsWithRef<"button"> & { depth: number }) {
  return (
    <button
      className={cx(
        "flex min-h-7.25 min-w-0 cursor-pointer items-center gap-1.75 bg-transparent pl-[calc(7px+var(--gc-tree-depth)*15px)] pr-1.5 text-left text-[14px] font-medium leading-[1.35] text-muted hover:text-foreground",
        className,
      )}
      style={treeIndent(depth)}
      type="button"
      {...props}
    />
  );
}

// A folder name does not grow, so its change counts sit right after it.
export function EntryName({ children, grow = true }: { children: string; grow?: boolean }) {
  return <span className={cx("min-w-0 overflow-hidden text-ellipsis whitespace-nowrap", grow && "flex-1")}>{children}</span>;
}

// Revealed on row hover, or pinned open while a conflict is unresolved.
export function RowAction({ children, pinned }: { children: React.ReactNode; pinned: boolean }) {
  return (
    <span
      className={cx(
        "absolute right-1.25 top-1/2 z-2 grid w-23 -translate-y-1/2 place-items-end transition-opacity",
        pinned
          ? "opacity-100"
          : "pointer-events-none opacity-0 group-hover/row:pointer-events-auto group-hover/row:opacity-100 group-focus-within/row:pointer-events-auto group-focus-within/row:opacity-100",
      )}
    >
      {children}
    </span>
  );
}
