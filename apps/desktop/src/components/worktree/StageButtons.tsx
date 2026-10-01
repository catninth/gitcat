import { cx } from "../../lib";

const TONE = {
  add: "border-success bg-[color-mix(in_srgb,var(--gc-success)_16%,var(--gc-background))] enabled:hover:bg-success",
  remove: "border-danger bg-[color-mix(in_srgb,var(--gc-danger)_16%,var(--gc-background))] enabled:hover:bg-danger",
  conflict: "border-warning bg-[color-mix(in_srgb,var(--gc-warning)_18%,var(--gc-background))] text-warning! enabled:hover:bg-warning enabled:hover:text-[#1c1204]!",
} as const;

export type BulkButtonTone = keyof typeof TONE;

// Section-wide "Stage all"/"Unstage all"; unstaging is the destructive tone.
export function BulkButton({ busy, label, priority, tone, onClick }: {
  busy: boolean;
  label: string;
  priority: boolean;
  tone?: BulkButtonTone;
  onClick: () => void;
}) {
  return (
    <button
      className={cx(
        "cursor-pointer whitespace-nowrap rounded border px-2 py-1 text-[12px] font-[650] text-white disabled:cursor-default disabled:opacity-100",
        tone ? TONE[tone] : priority ? cx(TONE.remove, "font-bold") : TONE.add,
      )}
      disabled={busy}
      onClick={onClick}
      type="button"
    >
      {label}
    </button>
  );
}

// Row-level stage toggle, revealed on hover by the file tree.
export function StageButton({ busy, path, plus, onClick }: {
  busy: boolean;
  path: string;
  plus: boolean;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={`${plus ? "Stage" : "Unstage"} ${path}`}
      className={cx(
        "min-w-21.5 cursor-pointer whitespace-nowrap rounded border px-2.5 py-1 text-[13px] font-semibold leading-[1.35] text-white shadow-[0_2px_9px_color-mix(in_srgb,black_42%,transparent)] disabled:cursor-default disabled:opacity-100",
        plus ? TONE.add : TONE.remove,
      )}
      disabled={busy}
      onClick={onClick}
      type="button"
    >
      {plus ? "Stage File" : "Unstage File"}
    </button>
  );
}
