import { operationButtons } from "../../lib/conflicts";
import type { RepositoryOperationState } from "../../lib/types";
import { Button } from "../ui";

// The buttons under the commit message while an operation is in progress or a
// change stopped on conflicts. GitKraken's layout: the step forward on the left,
// the way out on the right. Unlike GitKraken, a cherry-pick and a revert keep
// their Abort -- Git can undo both, and the panel is the only place to ask.
export function OperationActions({
  busy,
  conflictCount,
  operation,
  stagedCount,
  onAbort,
  onCommit,
  onContinue,
  onSkip,
}: {
  busy: boolean;
  conflictCount: number;
  operation: RepositoryOperationState;
  stagedCount: number;
  onAbort: () => void;
  onCommit: () => void;
  onContinue: () => void;
  onSkip: () => void;
}) {
  const { primary, abortLabel } = operationButtons(operation, conflictCount, stagedCount);
  const run = primary.action === "skip" ? onSkip : primary.action === "continue" ? onContinue : onCommit;

  return (
    <div className="flex gap-1.75">
      <Button className="flex-1" disabled={busy || primary.disabled} onClick={run} tone="accent">
        {primary.label}
      </Button>
      {abortLabel ? (
        <Button className="flex-1" disabled={busy} onClick={onAbort} tone="danger">
          {abortLabel}
        </Button>
      ) : null}
    </div>
  );
}
