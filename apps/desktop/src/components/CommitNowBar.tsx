import { CircleHelp } from "lucide-react";

import { Button } from "./ui";

export type CommitNowKind = "cherry_pick" | "revert";

// Asked before a cherry-pick or a revert starts, as GitKraken does: "No"
// applies the change with `--no-commit` and leaves it staged for the user to
// shape into a commit of their own.
export function CommitNowBar({
  busy,
  kind,
  onAnswer,
  onCancel,
}: {
  busy: boolean;
  kind: CommitNowKind;
  onAnswer: (commit: boolean) => void;
  onCancel: () => void;
}) {
  const question = kind === "cherry_pick"
    ? "Do you want to immediately commit the cherry picked changes?"
    : "Do you want to immediately commit the reverted changes?";

  return (
    <div
      aria-label={question}
      className="z-15 flex min-h-11 flex-[0_0_auto] flex-wrap items-center gap-2.5 border-b border-border bg-panel px-3.5 py-1.5"
      role="alertdialog"
    >
      <CircleHelp aria-hidden="true" className="shrink-0 text-accent" size={16} />
      <span className="min-w-0 flex-1 text-[12px] text-foreground">{question}</span>
      <span className="flex shrink-0 items-center gap-1.5">
        <Button compact disabled={busy} onClick={() => onAnswer(true)} tone="accent">Yes</Button>
        <Button compact disabled={busy} onClick={() => onAnswer(false)}>No</Button>
        <Button compact disabled={busy} onClick={onCancel}>Cancel</Button>
      </span>
    </div>
  );
}
