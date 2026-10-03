import { House, X } from "lucide-react";
import { useEffect, useRef } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent } from "react";

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
}: {
  tab: TabView;
  active: boolean;
  actionsDisabled: boolean;
  onSelect: (tabId: string) => void;
  onClose: (tabId: string) => void;
  onContextMenu: (request: RepositoryTabContextMenuRequest) => void;
  onNavigate: (tabId: string, direction: "previous" | "next" | "first" | "last") => void;
}) {
  const tabRef = useRef<HTMLDivElement>(null);

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
        active
          ? "z-1 h-8.5 rounded-t-lg border-border border-b-0 bg-[color-mix(in_srgb,var(--gc-panel)_91%,black)] text-foreground"
          : "mb-1 h-7 rounded-md border-transparent bg-foreground/5 text-muted hover:bg-foreground/9 hover:text-foreground",
        tab.unavailable && "border-dashed opacity-70",
        tab.unavailable && !active && "border-border",
      )}
      data-repository-tab={tab.id}
      onAuxClick={(event) => {
        if (event.button === 1 && !actionsDisabled) onClose(tab.id);
      }}
      onContextMenu={openContextMenu}
      ref={tabRef}
      title={repositoryTabDescription(tab)}
    >
      <button
        aria-selected={active}
        className={cx(
          "flex h-full min-w-0 flex-1 cursor-pointer items-center gap-1.5 rounded-[inherit] bg-transparent pl-3 pr-7 text-inherit",
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
    </div>
  );
}
