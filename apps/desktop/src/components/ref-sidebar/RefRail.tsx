import { Cloud, Monitor, PanelLeftOpen, Tag } from "lucide-react";
import type { ReactNode } from "react";

import { IconButton } from "../ui";
import type { RefSectionKey } from "./sections";

// Width of the collapsed rail; App sizes the workspace column with it.
export const REF_RAIL_WIDTH = 38;

interface RefRailProps {
  localCount: number;
  remoteCount: number;
  tagCount: number;
  expandKeybind: string;
  onExpand: (section?: RefSectionKey) => void;
}

// Collapsed stand-in for the reference sidebar: one icon per section with its
// count, plus the expand button the toolbar used to carry.
export function RefRail({ localCount, remoteCount, tagCount, expandKeybind, onExpand }: RefRailProps) {
  return (
    <aside
      aria-label="References"
      className="gc-no-select flex size-full min-h-0 flex-col items-center gap-0.5 overflow-y-auto overflow-x-hidden border-r border-border/72 bg-sunken py-2.5"
    >
      <IconButton
        aria-label="Show branches panel"
        className="mb-1.5"
        onClick={() => onExpand()}
        title={`Show branches panel (${expandKeybind})`}
      >
        <PanelLeftOpen size={16} />
      </IconButton>

      <RailSection
        count={localCount}
        icon={<Monitor size={15} />}
        label="Local branches"
        onClick={() => onExpand("local")}
      />
      <RailSection
        count={remoteCount}
        icon={<Cloud size={15} />}
        label="Remote branches"
        onClick={() => onExpand("remote")}
      />
      <RailSection count={tagCount} icon={<Tag size={15} />} label="Tags" onClick={() => onExpand("tags")} />
    </aside>
  );
}

function RailSection({ count, icon, label, onClick }: {
  count: number;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={`${label} (${count})`}
      className="flex w-8 cursor-pointer flex-col items-center gap-px rounded-[5px] border border-transparent bg-transparent px-0 py-1 text-muted hover:border-border/75 hover:bg-foreground/7 hover:text-foreground"
      onClick={onClick}
      title={`${label} (${count})`}
      type="button"
    >
      {icon}
      <b className="text-[11px] font-[750] leading-none text-[color-mix(in_srgb,var(--gc-accent)_75%,var(--gc-muted))]">
        {count}
      </b>
    </button>
  );
}
