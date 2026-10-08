import { Folder, GitBranch, House, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type {
  KeyboardEvent as ReactKeyboardEvent,
  MouseEvent as ReactMouseEvent,
  PointerEvent as ReactPointerEvent,
} from "react";

import { cx } from "../../lib";
import { IconButton } from "../ui";
import { repositoryTabDescription } from "./tabPresentation";

export interface TabView {
  id: string;
  label: string;
  path: string;
  kind?: "repository" | "start";
  dirty?: boolean;
  conflictCount?: number;
  unavailable?: boolean;
}

export interface RepositoryTabContextMenuRequest {
  tab: TabView;
  clientX: number;
  clientY: number;
}

export function RepositoryTab({
  tab,
  active,
  actionsDisabled,
  onSelect,
  onClose,
  onContextMenu,
  onNavigate,
  onDragStart,
  onShift,
  dragging,
}: {
  tab: TabView;
  active: boolean;
  actionsDisabled: boolean;
  onSelect: (tabId: string) => void;
  onClose: (tabId: string) => void;
  onContextMenu: (request: RepositoryTabContextMenuRequest) => void;
  onNavigate: (tabId: string, direction: "previous" | "next" | "first" | "last") => void;
  onDragStart: (tabId: string, event: ReactPointerEvent<HTMLDivElement>) => void;
  onShift: (tabId: string, step: -1 | 1) => void;
  dragging: boolean;
}) {
  const tabRef = useRef<HTMLDivElement>(null);
  const tooltipTimer = useRef<number | undefined>(undefined);
  const [tooltipPosition, setTooltipPosition] = useState<{ left: number; top: number } | null>(null);

  const hideTooltip = () => {
    window.clearTimeout(tooltipTimer.current);
    setTooltipPosition(null);
  };

  const scheduleTooltip = () => {
    window.clearTimeout(tooltipTimer.current);
    tooltipTimer.current = window.setTimeout(() => {
      const bounds = tabRef.current?.getBoundingClientRect();
      if (!bounds) return;
      const left = Math.max(8, Math.min(bounds.left, window.innerWidth - 468));
      setTooltipPosition({ left, top: bounds.bottom + 4 });
    }, 450);
  };

  useEffect(() => () => window.clearTimeout(tooltipTimer.current), []);

  useEffect(() => {
    const tabElement = tabRef.current;
    if (!active || !tabElement) return;

    const keepActiveTabVisible = () => {
      tabElement.scrollIntoView({ block: "nearest", inline: "nearest" });
    };
    keepActiveTabVisible();

    const tabList = tabElement.closest('[role="tablist"]');
    if (!tabList || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(keepActiveTabVisible);
    observer.observe(tabList);
    return () => observer.disconnect();
  }, [active]);

  const openContextMenu = (event: ReactMouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    if (actionsDisabled) return;
    event.currentTarget.querySelector<HTMLButtonElement>("[data-tab-main]")?.focus();
    onContextMenu({ tab, clientX: event.clientX, clientY: event.clientY });
  };

  const openKeyboardContextMenu = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault();
      if (actionsDisabled) return;
      const bounds = event.currentTarget.getBoundingClientRect();
      onContextMenu({ tab, clientX: bounds.left + 12, clientY: bounds.bottom - 2 });
      return;
    }

    if (event.ctrlKey && event.shiftKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      event.preventDefault();
      onShift(tab.id, event.key === "ArrowLeft" ? -1 : 1);
      return;
    }

    const direction = {
      ArrowLeft: "previous",
      ArrowRight: "next",
      Home: "first",
      End: "last",
    }[event.key] as "previous" | "next" | "first" | "last" | undefined;
    if (!direction) return;
    event.preventDefault();
    onNavigate(tab.id, direction);
  };

  return (
    <div
      aria-disabled={tab.unavailable || undefined}
      className={cx(
        "group/tab relative flex min-w-0 max-w-60 shrink-0 items-center border transition-[background-color,border-color,color,opacity] duration-120",
        // The active tab takes the toolbar's colour and runs into it, so the two read as one surface.
        // The others are solid too, so a tab dragged across another hides it.
        active
          ? "z-1 h-8.5 rounded-t-lg border-border border-b-0 bg-[color-mix(in_srgb,var(--gc-panel)_91%,black)] text-foreground"
          : "mb-1 h-7 rounded-md border-transparent bg-[color-mix(in_srgb,var(--gc-text)_6%,color-mix(in_srgb,var(--gc-surface)_93%,black))] text-muted hover:bg-[color-mix(in_srgb,var(--gc-text)_10%,color-mix(in_srgb,var(--gc-surface)_93%,black))] hover:text-foreground",
        tab.unavailable && "border-dashed",
        tab.unavailable && !active && "border-border",
        dragging ? "z-10 cursor-grabbing shadow-[0_4px_14px_rgb(0_0_0/35%)]" : "cursor-pointer",
      )}
      data-repository-tab={tab.id}
      onAuxClick={(event) => {
        if (event.button === 1 && !actionsDisabled) onClose(tab.id);
      }}
      onContextMenu={openContextMenu}
      onPointerDown={(event) => {
        hideTooltip();
        // Reordering only rearranges the row, so it stays available while a
        // command runs.
        if (event.button !== 0) return;
        if ((event.target as HTMLElement).closest("[data-tab-close]")) return;
        onDragStart(tab.id, event);
      }}
      onPointerEnter={scheduleTooltip}
      onPointerLeave={hideTooltip}
      ref={tabRef}
    >
      <button
        aria-description={repositoryTabDescription(tab)}
        aria-selected={active}
        className={cx(
          "flex h-full min-w-0 flex-1 cursor-[inherit] items-center gap-1.5 rounded-[inherit] bg-transparent pl-3 pr-7 text-inherit",
          "focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent",
        )}
        data-tab-main=""
        onClick={() => onSelect(tab.id)}
        onKeyDown={openKeyboardContextMenu}
        role="tab"
        tabIndex={active ? 0 : -1}
        type="button"
      >
        {tab.kind === "start" ? <House className="shrink-0" size={13} strokeWidth={1.9} /> : null}
        <span
          className={cx(
            "min-w-0 truncate text-[12px] leading-4",
            active ? "font-[620]" : "font-[540]",
            tab.unavailable && "text-danger line-through decoration-danger/50",
          )}
        >
          {tab.label}
        </span>
      </button>
      <IconButton
        aria-label={`Close ${tab.label}`}
        data-tab-close=""
        className={cx(
          "absolute right-1 size-5.5! rounded-sm! transition-opacity duration-100",
          active
            ? "opacity-70 hover:opacity-100"
            : "pointer-events-none opacity-0 group-hover/tab:pointer-events-auto group-hover/tab:opacity-70 group-focus-within/tab:pointer-events-auto group-focus-within/tab:opacity-70",
        )}
        disabled={actionsDisabled}
        onClick={() => onClose(tab.id)}
        tabIndex={active ? 0 : -1}
        title={`Close ${tab.label}`}
      >
        <X size={12} />
      </IconButton>
      {tooltipPosition && !dragging ? createPortal(<RepositoryTabTooltip position={tooltipPosition} tab={tab} />, document.body) : null}
    </div>
  );
}

function RepositoryTabTooltip({ tab, position }: { tab: TabView; position: { left: number; top: number } }) {
  const states: string[] = [];
  if (tab.dirty) states.push("Uncommitted changes");
  if (tab.conflictCount) {
    states.push(`${tab.conflictCount} unresolved conflict${tab.conflictCount === 1 ? "" : "s"}`);
  }
  if (tab.unavailable) states.push("Repository unavailable");

  return (
    <span
      className="pointer-events-none fixed z-260 flex max-w-[min(460px,calc(100vw-16px))] flex-col gap-1 rounded-[5px] border border-border bg-menu px-2.5 py-2 text-foreground shadow-panel"
      role="tooltip"
      style={position}
    >
      <span className="flex min-w-0 items-center gap-1.5 text-[12px] font-[620] leading-4">
        {tab.kind === "start" ? (
          <House className="shrink-0 text-muted" size={13} strokeWidth={1.9} />
        ) : (
          <GitBranch className="shrink-0 text-muted" size={13} strokeWidth={1.9} />
        )}
        <span className="min-w-0 truncate">{tab.label}</span>
      </span>
      {tab.kind === "start" ? (
        <span className="text-[11px] leading-4 text-muted">Start page</span>
      ) : tab.path ? (
        <span className="flex min-w-0 items-center gap-1.5 text-[11px] leading-4 text-muted">
          <Folder className="shrink-0" size={12} strokeWidth={1.9} />
          {/* Right-to-left so a long path loses its start, keeping the folders nearest the repository. */}
          <span className="min-w-0 truncate text-left" dir="rtl">
            <bdi dir="ltr">{tab.path}</bdi>
          </span>
        </span>
      ) : null}
      {states.length ? <span className="text-[11px] leading-4 text-muted">{states.join(" · ")}</span> : null}
    </span>
  );
}
