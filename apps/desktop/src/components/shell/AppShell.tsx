import type { ReactNode } from "react";

import { cx } from "../../lib";

// Window-sized root. The window's own minimum size guards the layout; a CSS
// minimum here would be multiplied by the interface zoom and push the status
// bar off the bottom of the window at 200%.
export function AppShell({ className = "", children }: { className?: string; children: ReactNode }) {
  return (
    <div
      className={cx(
        "isolate flex size-full flex-col overflow-hidden bg-[radial-gradient(circle_at_78%_-15%,color-mix(in_srgb,var(--gc-accent)_5%,transparent),transparent_34%),var(--gc-background)]",
        className,
      )}
    >
      {children}
    </div>
  );
}
