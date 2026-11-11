import { useId } from "react";
import type { ReactNode } from "react";

import { cx } from "../../lib";
import { matchesKeybind } from "../../lib/keybinds";
import { Button, Input, TextArea } from "../ui";

export interface CommitDraft {
  message: string;
  description: string;
  amend: boolean;
  signoff: boolean;
  /**
   * The operation message this draft was filled from. It keeps a message the
   * user cleared from coming back, and lets the draft be cleared again when
   * the operation ends with the prefilled text untouched.
   */
  prefilledFrom?: string;
}

const FIELD = "w-full rounded-[5px] border border-border bg-background leading-[1.45] outline-0 focus:border-accent";

export const SUMMARY_LIMIT = 72;

function SummaryCounter({ remaining }: { remaining: number }) {
  const tooltipId = useId();
  return (
    <span className="group/counter pointer-events-auto absolute right-2 top-1/2 -translate-y-1/2">
      <span
        aria-describedby={tooltipId}
        aria-live="polite"
        className={cx(
          "cursor-default select-none font-mono text-[11px] tabular-nums",
          remaining < 0 ? "text-warning" : "text-muted",
        )}
        tabIndex={0}
      >
        {remaining}
      </span>
      <span
        className="invisible pointer-events-none absolute right-0 top-[calc(100%+7px)] z-90 w-[min(276px,calc(100vw-24px))] -translate-y-0.75 rounded-[5px] border border-[color-mix(in_srgb,var(--gc-border)_82%,white_5%)] bg-menu px-2.25 py-2 text-[10px] leading-[1.45] text-foreground opacity-0 shadow-panel transition-[opacity,transform] duration-90 before:absolute before:-top-1.25 before:right-2 before:size-2 before:rotate-45 before:border-l before:border-t before:border-border before:bg-menu before:content-[''] group-hover/counter:visible group-hover/counter:translate-y-0 group-hover/counter:opacity-100 group-focus-within/counter:visible group-focus-within/counter:translate-y-0 group-focus-within/counter:opacity-100"
        id={tooltipId}
        role="tooltip"
      >
        Commit summaries no longer than {SUMMARY_LIMIT} characters will be fully displayed in most websites and CLI
        tools.
      </span>
    </span>
  );
}

export function CommitForm({
  actions,
  busy,
  canCommit,
  commitKeybind,
  draft,
  notice,
  options = true,
  stagedCount,
  onDraftChange,
  onSubmit,
}: {
  /** Replaces the commit button while an operation decides what pressing it means. */
  actions?: ReactNode;
  busy: boolean;
  canCommit: boolean;
  commitKeybind: string;
  draft: CommitDraft;
  /** A line above the message that is about the operation, not part of the message. */
  notice?: ReactNode;
  /** Amend and sign-off only make sense for an ordinary commit. */
  options?: boolean;
  stagedCount: number;
  onDraftChange: (draft: CommitDraft) => void;
  onSubmit: () => void;
}) {
  return (
    <div className="flex min-h-0 flex-[0_0_auto] flex-col gap-2 p-2.75">
      <label className="text-[10px] font-bold uppercase tracking-wider text-muted" htmlFor="commit-message">
        Commit message
      </label>
      {notice ? <strong className="text-[11px] font-[650] text-warning">{notice}</strong> : null}
      <div className="relative">
        <Input
          className={cx(FIELD, "py-1.75 pl-2.25 pr-10")}
          disabled={busy}
          id="commit-message"
          onChange={(event) => onDraftChange({ ...draft, message: event.target.value })}
          onKeyDown={(event) => {
            if (matchesKeybind(event.nativeEvent, commitKeybind)) {
              event.preventDefault();
              event.stopPropagation();
              onSubmit();
              return;
            }
            // Enter moves to the description instead of submitting a one-line commit.
            if (event.key === "Enter") {
              event.preventDefault();
              document.getElementById("commit-description")?.focus();
            }
          }}
          placeholder="Summary"
          type="text"
          value={draft.message}
        />
        <SummaryCounter remaining={SUMMARY_LIMIT - draft.message.length} />
      </div>
      <TextArea
        className={cx(FIELD, "h-21 min-h-21 flex-[0_0_auto] resize-none px-2.25 py-1.5")}
        disabled={busy}
        id="commit-description"
        onChange={(event) => onDraftChange({ ...draft, description: event.target.value })}
        onKeyDown={(event) => {
          if (matchesKeybind(event.nativeEvent, commitKeybind)) {
            event.preventDefault();
            event.stopPropagation();
            onSubmit();
          }
        }}
        placeholder="Description (optional)"
        rows={4}
        value={draft.description}
      />
      {options ? <div className="flex gap-3.5 text-[11px] text-muted">
        <label className="flex items-center gap-1.25">
          <input
            checked={draft.amend}
            className="accent-accent"
            disabled={busy}
            onChange={(event) => onDraftChange({ ...draft, amend: event.target.checked })}
            type="checkbox"
          />
          Amend
        </label>
        <label className="flex items-center gap-1.25">
          <input
            checked={draft.signoff}
            className="accent-accent"
            disabled={busy}
            onChange={(event) => onDraftChange({ ...draft, signoff: event.target.checked })}
            type="checkbox"
          />
          Sign off
        </label>
      </div> : null}
      {actions ?? (
        <Button disabled={!canCommit} onClick={onSubmit} tone="accent">
          {draft.amend
            ? "Amend commit"
            : stagedCount > 0
              ? `Commit Changes to ${stagedCount} File${stagedCount === 1 ? "" : "s"}`
              : "Stage Changes to Commit"}
        </Button>
      )}
    </div>
  );
}
