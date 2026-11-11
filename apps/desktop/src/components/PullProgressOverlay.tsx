import type { PullPhase } from "../lib/types";
import { Spinner } from "./ui";

/** What a running pull is doing, in the words the user would use for it. */
export function pullPhaseLabel(phase: PullPhase): string {
  switch (phase.phase) {
    case "stashing":
      return "Stashing local changes";
    case "pulling":
      return "Pulling";
    case "rebasing":
      return `Rebasing commit ${phase.current} of ${phase.total}`;
    case "restoring":
      return "Restoring local changes";
  }
}

/**
 * The step a pull has reached. A pull that holds local changes aside is
 * several Git commands, and a bare spinner leaves the user guessing which one
 * is taking the time.
 */
export function PullProgressOverlay({ phase }: { phase: PullPhase | null }) {
  if (!phase) return null;
  const label = pullPhaseLabel(phase);
  return (
    <div
      aria-live="polite"
      className="pointer-events-none fixed right-4 bottom-9.75 z-400 flex items-center gap-2.25 rounded-md border border-border bg-menu px-3 py-2.5 shadow-[0_12px_34px_rgb(0_0_0/35%)]"
      role="status"
    >
      <Spinner label={label} />
      <span className="text-[12px] font-semibold">{label}</span>
    </div>
  );
}
