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

const SIZES = {
  sm: { text: "text-[9px]", icon: 10 },
  md: { text: "text-[10px]", icon: 11 },
} as const;

export function ChangeCount({ icon: Icon, size = "sm", tone, children }: {
  icon: LucideIcon;
  size?: keyof typeof SIZES;
  tone: string;
  children: ReactNode;
}) {
  const { text, icon } = SIZES[size];
  return (
    <span className={cx("flex shrink-0 items-center gap-px font-mono leading-none text-foreground", text)}>
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
