import { GitMerge } from "lucide-react";
import { useState } from "react";
import { createPortal } from "react-dom";

import type { ConflictSideLabels } from "../../lib/conflicts";
import type { ConflictResolution, StatusEntry } from "../../lib/types";
import { ContextMenu, type ContextAction } from "../ContextMenu";
import { IconButton } from "../ui";

const MENU_WIDTH = 244;

export function ConflictQuickActions({
  busy,
  entry,
  labels,
  onOpen,
  onResolve,
}: {
  busy: boolean;
  entry: StatusEntry;
  labels: ConflictSideLabels;
  onOpen: () => void;
  onResolve: (entry: StatusEntry, resolution: ConflictResolution) => void;
}) {
  const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(null);
  const resolve = (resolution: ConflictResolution) => {
    setMenuPosition(null);
    onResolve(entry, resolution);
  };
  const actions: ContextAction[] = [
    { id: "open_editor", label: "Open merge editor…" },
    { id: "ours", label: `Take current (${labels.ours})`, separatorBefore: true },
    { id: "theirs", label: `Take incoming (${labels.theirs})` },
    { id: "mark_resolved", label: "Mark resolved" },
    { id: "delete", label: "Delete file", danger: true, separatorBefore: true },
  ];

  return (
    <span className="relative inline-grid">
      <IconButton
        aria-expanded={Boolean(menuPosition)}
        aria-haspopup="menu"
        aria-label={`Resolve conflict in ${entry.path}`}
        className="size-6.25! text-warning!"
        disabled={busy}
        onClick={(event) => {
          const bounds = event.currentTarget.getBoundingClientRect();
          setMenuPosition((current) => current ? null : { x: bounds.right - MENU_WIDTH, y: bounds.bottom + 3 });
        }}
        title="Resolve conflict"
      >
        <GitMerge aria-hidden="true" size={14} />
      </IconButton>
      {menuPosition ? createPortal(
        <ContextMenu
          actions={actions}
          onAction={(action) => {
            if (action === "open_editor") {
              setMenuPosition(null);
              onOpen();
            } else resolve(action as ConflictResolution);
          }}
          onClose={() => setMenuPosition(null)}
          x={menuPosition.x}
          y={menuPosition.y}
        />,
        document.body,
      ) : null}
    </span>
  );
}
