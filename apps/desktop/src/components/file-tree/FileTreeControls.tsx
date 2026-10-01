import { ArrowDownAZ, FolderTree, List } from "lucide-react";
import type { ReactNode } from "react";

import { cx } from "../../lib";
import type { FileViewMode } from "../../lib/types";

function ViewSwitchButton({ active, children, className = "", icon, onClick }: {
  active: boolean;
  children: string;
  className?: string;
  icon: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      aria-pressed={active}
      className={cx(
        "inline-flex h-7.5 min-w-18 cursor-pointer items-center justify-center gap-1.5 bg-transparent px-2.5 text-[13px] font-[650]",
        active
          ? "bg-row-selected text-foreground shadow-[inset_0_-2px_0_var(--gc-accent)]"
          : "text-muted hover:bg-row-hover hover:text-foreground",
        className,
      )}
      onClick={onClick}
      type="button"
    >
      {icon}
      {children}
    </button>
  );
}

export function FileTreeControls({
  className = "",
  mode,
  onModeChange,
}: {
  className?: string;
  mode: FileViewMode;
  onModeChange: (mode: FileViewMode) => void;
}) {
  return (
    <div
      className={cx(
        "relative flex min-h-9.75 flex-[0_0_39px] items-center justify-center border-b border-border/75 bg-panel/32 px-2.5 py-1.25",
        className,
      )}
    >
      <span
        aria-label="Files sorted alphabetically"
        className="absolute left-2.75 grid size-6.25 place-items-center text-muted"
        title="Sorted A–Z"
      >
        <ArrowDownAZ aria-hidden="true" size={18} />
      </span>
      <div aria-label="File list layout" className="inline-flex border border-border bg-background" role="group">
        <ViewSwitchButton
          active={mode === "path"}
          icon={<List aria-hidden="true" size={16} />}
          onClick={() => onModeChange("path")}
        >
          Path
        </ViewSwitchButton>
        <ViewSwitchButton
          active={mode === "tree"}
          className="border-l border-border"
          icon={<FolderTree aria-hidden="true" size={16} />}
          onClick={() => onModeChange("tree")}
        >
          Tree
        </ViewSwitchButton>
      </div>
    </div>
  );
}
