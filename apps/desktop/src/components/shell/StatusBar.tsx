import type { ReactNode } from "react";

import { cx } from "../../lib";

export function StatusBar({ children }: { children: ReactNode }) {
  return (
    <footer className="flex h-7.5 flex-[0_0_30px] items-center gap-4 border-t border-border bg-[color-mix(in_srgb,var(--gc-surface)_95%,black)] px-3 font-mono text-[11px] text-muted">
      {children}
    </footer>
  );
}

export function StatusItem({ className = "", children }: { className?: string; children: ReactNode }) {
  return <span className={cx("inline-flex items-center gap-1.25 whitespace-nowrap", className)}>{children}</span>;
}

export function StatusSpacer() {
  return <span className="flex-1" />;
}

export function BuildIdentity({ title, children }: { title: string; children: ReactNode }) {
  return (
    <StatusItem className="border-l border-border pl-2.75 text-[color-mix(in_srgb,var(--gc-text)_72%,var(--gc-muted))]">
      <span title={title}>{children}</span>
    </StatusItem>
  );
}
