import type { ReactNode } from "react";

import { cx } from "../../lib";

export const FIELD_INPUT =
  "h-8.5 w-full rounded-[5px] border border-border bg-background px-2.25 outline-0 focus:border-accent";

export function SectionHeading({ className = "", children }: { className?: string; children: ReactNode }) {
  return (
    <h3 className={cx("mb-3 mt-6.25 text-[12px] uppercase tracking-[0.07em] text-foreground first:mt-0.5", className)}>
      {children}
    </h3>
  );
}

// Label above a full-width control; `hint` renders right-aligned next to it.
export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="mb-3.25 flex flex-col gap-1.5 text-[13px] text-muted">
      <span className="flex justify-between">
        {label}
        {hint ? <small className="text-muted/72">{hint}</small> : null}
      </span>
      {children}
    </label>
  );
}

// `disabled` is for an option that only means something while another one is
// on: it stays visible and keeps its stored value rather than disappearing.
export function CheckField({ checked, children, disabled = false, onChange }: {
  checked: boolean;
  children: string;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label
      className={cx(
        "flex items-start gap-1.75 text-[13px] leading-[1.4] text-muted",
        disabled && "opacity-55",
      )}
    >
      <input
        checked={checked}
        className="mt-0.5 accent-accent"
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        type="checkbox"
      />
      {children}
    </label>
  );
}
