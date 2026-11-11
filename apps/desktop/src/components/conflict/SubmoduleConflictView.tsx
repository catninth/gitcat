import { FolderGit2, X } from "lucide-react";

import type { SubmoduleConflictDetails, SubmoduleConflictSide } from "../../lib/types";
import { Button, IconButton } from "../ui";

function SideCard({ title, side }: { title: string; side: SubmoduleConflictSide | undefined }) {
  return (
    <section className="flex min-w-0 flex-1 flex-col gap-1 rounded-[5px] border border-border bg-panel px-3 py-2.5">
      <strong className="text-[11px] uppercase tracking-wider text-muted">{title}</strong>
      {side ? (
        <>
          <span className="font-mono text-[12px] text-foreground">{side.oid.slice(0, 7)}</span>
          <span className="min-w-0 truncate text-[12px] text-foreground" title={side.summary}>
            {side.summary ?? "Commit not available in the submodule"}
          </span>
          {side.author || side.committed_at ? (
            <small className="text-[11px] text-muted">
              {side.author ?? ""}
              {side.author && side.committed_at ? " · " : ""}
              {side.committed_at ? new Date(side.committed_at * 1000).toLocaleString() : ""}
            </small>
          ) : null}
        </>
      ) : (
        <span className="text-[12px] text-muted">Not present on this side</span>
      )}
    </section>
  );
}

// A submodule conflict is a disagreement about which commit the pointer names,
// so there are no lines to merge: the view shows the commit the submodule has
// checked out next to the two the branches recorded. GitKraken offers only
// "Use current"; taking the incoming pointer is the other half of the answer.
export function SubmoduleConflictView({
  busy,
  closeKeybind,
  details,
  onClose,
  onOpenSubmodule,
  onUseCurrent,
  onUseIncoming,
}: {
  busy: boolean;
  closeKeybind?: string;
  details: SubmoduleConflictDetails;
  onClose: () => void;
  onOpenSubmodule: () => void;
  onUseCurrent: () => void;
  onUseIncoming: () => void;
}) {
  return (
    <section aria-label={`Submodule conflict in ${details.path}`} className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto bg-background">
      <header className="flex min-h-12 flex-[0_0_auto] items-center gap-2.5 border-b border-border px-3.25 py-1.5">
        <FolderGit2 aria-hidden="true" className="shrink-0 text-warning" size={16} />
        <h2 className="m-0 min-w-0 flex-1 truncate font-mono text-[12px] font-[550] text-foreground">{details.path}</h2>
        <Button compact disabled={busy || !details.current} onClick={onUseCurrent} tone="accent">Use current</Button>
        <Button compact disabled={busy || !details.theirs} onClick={onUseIncoming}>Use incoming</Button>
        <Button compact disabled={busy || !details.current} onClick={onOpenSubmodule}>Open submodule</Button>
        <IconButton
          aria-label="Back to graph"
          onClick={onClose}
          title={closeKeybind ? `Back to graph (${closeKeybind})` : "Back to graph"}
        >
          <X size={16} />
        </IconButton>
      </header>
      <div className="flex flex-wrap gap-2.5 p-3.25">
        <SideCard side={details.current} title="Current" />
        <SideCard side={details.ours} title="Ours" />
        <SideCard side={details.theirs} title="Theirs" />
      </div>
      {details.current ? null : (
        <p className="m-0 px-3.25 text-[11px] text-muted">
          The submodule is not checked out, so its commits cannot be described or opened.
        </p>
      )}
    </section>
  );
}
