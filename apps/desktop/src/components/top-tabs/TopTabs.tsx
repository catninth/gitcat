import { Plus } from "lucide-react";
import { useEffect, useRef } from "react";

import { IconButton } from "../ui";
import { RepositoryTab } from "./RepositoryTab";
import type { RepositoryTabContextMenuRequest, TabView } from "./RepositoryTab";
import { TabOverview } from "./TabOverview";

interface TopTabsProps {
  tabs: TabView[];
  activeTabId?: string;
  onSelect: (tabId: string) => void;
  onClose: (tabId: string) => void;
  onOpen: () => void;
  onTabContextMenu: (request: RepositoryTabContextMenuRequest) => void;
  actionsDisabled?: boolean;
}

export function TopTabs({
  tabs,
  activeTabId,
  onSelect,
  onClose,
  onOpen,
  onTabContextMenu,
  actionsDisabled = false,
}: TopTabsProps) {

  const tabListRef = useRef<HTMLDivElement>(null);

  // A mouse wheel only scrolls vertically, and the row only scrolls
  // sideways, so the wheel moves the row. React's wheel handler is passive and
  // cannot stop the page from taking the event, hence the native listener.
  useEffect(() => {
    const tabList = tabListRef.current;
    if (!tabList) return;
    const scrollSideways = (event: WheelEvent) => {
      if (event.deltaY === 0 || Math.abs(event.deltaX) > Math.abs(event.deltaY)) return;
      if (tabList.scrollWidth <= tabList.clientWidth) return;
      event.preventDefault();
      tabList.scrollLeft += event.deltaY;
    };
    tabList.addEventListener("wheel", scrollSideways, { passive: false });
    return () => tabList.removeEventListener("wheel", scrollSideways);
  }, []);

  const navigateFromTab = (
    tabId: string,
    direction: "previous" | "next" | "first" | "last",
  ) => {
    if (!tabs.length) return;
    const currentIndex = tabs.findIndex((tab) => tab.id === tabId);
    const nextIndex = direction === "first"
      ? 0
      : direction === "last"
        ? tabs.length - 1
        : direction === "previous"
          ? (currentIndex - 1 + tabs.length) % tabs.length
          : (currentIndex + 1) % tabs.length;
    const nextTab = tabs[nextIndex];
    if (!nextTab) return;
    onSelect(nextTab.id);
    requestAnimationFrame(() => {
      const tabElement = [...document.querySelectorAll<HTMLElement>("[data-repository-tab]")]
        .find((element) => element.dataset.repositoryTab === nextTab.id);
      tabElement?.querySelector<HTMLButtonElement>("[data-tab-main]")?.focus();
    });
  };

  return (
    <div
      className="z-20 flex h-10 flex-[0_0_40px] select-none items-end gap-1 bg-[color-mix(in_srgb,var(--gc-surface)_93%,black)] px-3 shadow-[inset_0_-1px_0_var(--gc-border)]"
      aria-label="Repositories"
    >
      <div className="flex min-w-0 items-end gap-1 self-stretch">
        <div
          className="flex min-w-0 items-end gap-1 self-stretch overflow-x-auto overflow-y-hidden pt-0.5 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
          ref={tabListRef}
          role="tablist"
          aria-label="Open repositories"
        >
          {tabs.map((tab) => (
            <RepositoryTab
              active={activeTabId === tab.id}
              actionsDisabled={actionsDisabled}
              key={tab.id}
              onClose={onClose}
              onContextMenu={onTabContextMenu}
              onNavigate={navigateFromTab}
              onSelect={onSelect}
              tab={tab}
            />
          ))}
          {!tabs.length ? (
            <span className="mb-1 flex h-7 items-center px-2.5 text-[11px] text-muted">
              No repositories open
            </span>
          ) : null}
        </div>
        <IconButton
          aria-label="Open repository"
          className="mb-1 size-7! rounded-md!"
          disabled={actionsDisabled}
          onClick={onOpen}
          title="Open repository"
        >
          <Plus size={15} strokeWidth={2} />
        </IconButton>
      </div>
      <span className="flex-1" />
      <div className="mb-1 flex flex-[0_0_auto] items-center">
        <TabOverview
          activeTabId={activeTabId}
          disabled={actionsDisabled}
          onSelect={onSelect}
          tabs={tabs}
        />
      </div>
    </div>
  );
}
