import type { LucideIcon } from "lucide-react";
import { forwardRef } from "react";

// Lucide ships outline icons only, and filling its open pencil path still reads as an outline
// at small sizes, so this draws a solid pencil. It keeps the LucideIcon shape so it drops into
// the icon maps; stroke props are ignored because nothing is stroked.
export const PencilFilled: LucideIcon = forwardRef<SVGSVGElement, React.ComponentProps<LucideIcon>>(
  ({ size = 24, color = "currentColor", strokeWidth: _s, absoluteStrokeWidth: _a, children: _c, ...props }, ref) => (
    <svg
      fill={color}
      height={size}
      ref={ref}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a1 1 0 0 0 0-1.41l-2.34-2.34a1 1 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z" />
    </svg>
  ),
);
PencilFilled.displayName = "PencilFilled";
