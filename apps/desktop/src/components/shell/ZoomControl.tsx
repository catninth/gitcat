import { ZoomIn } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { cx } from "../../lib";
import { ZOOM_LEVELS } from "../../lib/zoom";
import { MenuItem, MenuSurface } from "../menu";
import { StatusItem } from "./StatusBar";

export function ZoomControl({ percent, onChange }: { percent: number; onChange: (percent: number) => void }) {
  const [open, setOpen] = useState(false);
  const hostRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;
    const closeOutside = (event: MouseEvent) => {
      if (!hostRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      hostRef.current?.querySelector<HTMLButtonElement>("[aria-haspopup='menu']")?.focus();
    };
    window.addEventListener("mousedown", closeOutside);
    window.addEventListener("keydown", closeOnEscape);
    requestAnimationFrame(() => hostRef.current?.querySelector<HTMLButtonElement>("[aria-checked='true']")?.focus());
    return () => {
      window.removeEventListener("mousedown", closeOutside);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  return (
    <StatusItem className="relative">
      <span className="contents" ref={hostRef}>
        <button
          aria-expanded={open}
          aria-haspopup="menu"
          className="inline-flex cursor-pointer items-center gap-1 rounded-[3px] px-1 py-0.5 text-muted hover:bg-row-hover hover:text-foreground"
          onClick={() => setOpen((current) => !current)}
          title="Interface zoom (Ctrl+= / Ctrl+- / Ctrl+0)"
          type="button"
        >
          <ZoomIn size={14} /> {percent}%
        </button>
        {open ? (
          <MenuSurface className="absolute right-0 bottom-full z-120 mb-1 w-22 p-1!" role="menu">
            {[...ZOOM_LEVELS].reverse().map((level) => (
              <MenuItem
                aria-checked={level === percent}
                className={cx(
                  "min-h-7! justify-end py-0.5! font-mono text-[12px]",
                  level === percent && "bg-[color-mix(in_srgb,var(--gc-accent)_22%,transparent)] text-accent",
                )}
                key={level}
                onClick={() => {
                  onChange(level);
                  setOpen(false);
                }}
                role="menuitemradio"
              >
                {level}%
              </MenuItem>
            ))}
          </MenuSurface>
        ) : null}
      </span>
    </StatusItem>
  );
}
