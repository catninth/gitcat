import { FilePen, Minus, Plus } from "lucide-react";
import { PencilFilled } from "../ui/PencilFilled";
import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import { cx } from "../../lib";
import type { FileChangeCounts } from "./tree";

type ChangeCountKind = keyof FileChangeCounts;

const CHANGE_COUNT_PARTS: readonly { kind: ChangeCountKind; icon: LucideIcon; tone: string; label: string }[] = [
  { kind: "modified", icon: PencilFilled, tone: "text-warning", label: "modified" },
  { kind: "added", icon: Plus, tone: "text-success", label: "added" },
  { kind: "deleted", icon: Minus, tone: "text-danger", label: "deleted" },
  { kind: "renamed", icon: FilePen, tone: "text-accent", label: "renamed" },
];

// Sized against GitKraken at 100%: the count reads as large as the file name
// beside it, not as a footnote to it.
const SIZES = {
  sm: { text: "text-[13px]", icon: 14 },
  md: { text: "text-[12px]", icon: 13 },
} as const;

export function ChangeCount({ icon: Icon, size = "sm", tone, children }: {
  icon: LucideIcon;
  size?: keyof typeof SIZES;
  tone: string;
  children: ReactNode;
}) {
  const { text, icon } = SIZES[size];
  return (
    <span className={cx("flex shrink-0 items-center gap-1 font-semibold leading-none tabular-nums text-foreground", text)}>
      <Icon aria-hidden="true" className={cx("shrink-0", tone)} size={icon} strokeWidth={3} />
      {children}
    </span>
  );
}

export function ChangeCountSummary({ counts, labels = false, size = "sm" }: {
  counts: FileChangeCounts;
  labels?: boolean;
  size?: keyof typeof SIZES;
}) {
  return (
    <>
      {CHANGE_COUNT_PARTS.map(({ kind, icon, tone, label }) => {
        const value = counts[kind];
        if (!value) return null;
        return (
          <ChangeCount icon={icon} key={kind} size={size} tone={tone}>
            {value}
            {labels ? <span className="ml-1 font-sans text-muted">{label}</span> : null}
          </ChangeCount>
        );
      })}
    </>
  );
}
