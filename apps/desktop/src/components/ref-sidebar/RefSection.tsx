import { ChevronDown, ChevronRight } from "lucide-react";
import type { ReactNode } from "react";

export function SidebarEmpty({ children }: { children: string }) {
  return <p className="mx-5 my-1.5 text-[11px] text-muted">{children}</p>;
}

export function RefSection({
  label,
  count,
  icon,
  open,
  onToggle,
  trailing,
  children,
}: {
  label: string;
  count: number;
  icon: ReactNode;
  open: boolean;
  onToggle: () => void;
  trailing?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="border-t border-border/72">
      <header className="flex h-9.25 items-center pl-0.75 pr-1">
        <button
          aria-expanded={open}
          className="flex min-w-0 flex-1 cursor-pointer items-center gap-1.75 bg-transparent px-1.25 text-[11px] font-[750] tracking-[0.07em] text-muted hover:text-foreground"
          onClick={onToggle}
          type="button"
        >
          {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          {icon}
          <span>{label}</span>
          <b className="ml-auto text-[12px] tabular-nums text-[color-mix(in_srgb,var(--gc-accent)_75%,var(--gc-muted))]">
            {count}
          </b>
        </button>
        {trailing}
      </header>
      {open ? <div className="px-1.25 pb-2">{children}</div> : null}
    </section>
  );
}
