import { TriangleAlert } from "lucide-react";

import type { ConflictDecision } from "../../lib/conflicts";
import type { ConflictResolution } from "../../lib/types";
import { Button } from "../ui";

// A conflict that is not two versions of one text -- a file deleted on one
// side, kept on the other, or a name both sides used differently -- has
// nothing to merge line by line. GitKraken asks the one question it raises in
// a bar above the view, and so does this.
export function ConflictDecisionBar({
  busy,
  decision,
  onCancel,
  onChoose,
}: {
  busy: boolean;
  decision: ConflictDecision;
  onCancel: () => void;
  onChoose: (resolution: ConflictResolution) => void;
}) {
  return (
    <div
      aria-label="Conflict decision"
      className="z-15 flex min-h-11 flex-[0_0_auto] flex-wrap items-center gap-2.5 border-b border-[color-mix(in_srgb,var(--gc-warning)_37%,var(--gc-border))] bg-[color-mix(in_srgb,var(--gc-warning)_9%,var(--gc-surface))] px-3.5 py-1.5"
      role="alertdialog"
    >
      <TriangleAlert aria-hidden="true" className="shrink-0 text-warning" size={16} />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="text-[12px] text-foreground">{decision.message}</span>
        {decision.helperFile ? (
          <small className="text-[11px] text-muted">
            Git renamed this file aside because a folder with the same name exists on the other side.
          </small>
        ) : null}
      </span>
      <span className="flex shrink-0 flex-wrap items-center gap-1.5">
        {decision.choices.map((choice) => (
          <Button
            compact
            disabled={busy}
            key={choice.resolution}
            onClick={() => onChoose(choice.resolution)}
            tone={choice.danger ? "danger" : "accent"}
          >
            {choice.label}
          </Button>
        ))}
        <Button compact disabled={busy} onClick={onCancel}>Cancel</Button>
      </span>
    </div>
  );
}
