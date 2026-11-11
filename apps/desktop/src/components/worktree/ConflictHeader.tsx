import { TriangleAlert } from "lucide-react";

import { conflictHeadline, conflictSubtitle, type ConflictContext } from "../../lib/conflicts";
import { Badge } from "../ui";

// Replaces the working tree header while files are conflicted. It names both
// sides only where there are two branches to name -- a merge and a rebase --
// and it drops "discard all changes", which here would throw away the
// resolutions along with everything else.
export function ConflictHeader({ context }: { context: ConflictContext }) {
  const subtitle = conflictSubtitle(context);

  return (
    <header className="flex min-h-14.25 flex-[0_0_auto] flex-col justify-center gap-1 border-b border-border px-2.75 py-2">
      <span className="flex min-w-0 items-center justify-center gap-1.5 text-[12px] font-[650] text-warning">
        <TriangleAlert aria-hidden="true" size={13} />
        <span className="min-w-0 truncate">{conflictHeadline(context.operation)}</span>
      </span>
      {subtitle ? (
        <div className="flex min-w-0 flex-wrap items-center justify-center gap-1.25 text-[11px] text-muted">
          <span>{subtitle.verb}</span>
          <Badge className="min-w-0 max-w-45 overflow-hidden text-ellipsis whitespace-nowrap" tone="accent">
            {subtitle.incoming}
          </Badge>
          <span>{subtitle.preposition}</span>
          <Badge className="min-w-0 max-w-45 overflow-hidden text-ellipsis whitespace-nowrap" tone="accent">
            {subtitle.target}
          </Badge>
        </div>
      ) : null}
    </header>
  );
}

/**
 * GitKraken never says that Git put local changes aside for the operation; a
 * user who cannot see them assumes they are gone.
 */
export function AutostashNotice() {
  return (
    <p className="m-0 border-b border-border px-2.75 py-1.5 text-center text-[11px] text-muted">
      Your local changes are held aside and come back when this operation finishes or is aborted.
    </p>
  );
}
