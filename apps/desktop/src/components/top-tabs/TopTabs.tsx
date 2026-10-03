import { Plus } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";

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
  onReorder: (tabId: string, toIndex: number) => void;
  actionsDisabled?: boolean;
}

// How far the pointer travels before a press on a tab becomes a drag, so an
// ordinary click never nudges the row.
const DRAG_THRESHOLD = 4;
const EDGE_SCROLL_ZONE = 28;
const EDGE_SCROLL_STEP = 12;
const SETTLE = "transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1)";

interface DragSession {
  id: string;
  from: number;
  target: number;
  startClientX: number;
  startContentX: number;
  // Where the grabbed tab already stood when it was picked up mid-slide.
  startShift: number;
  active: boolean;
  elements: HTMLElement[];
  // Where each tab sat when the drag began, in the row's scrolled coordinates.
  slots: { left: number; width: number }[];
  gap: number;
  end: () => void;
}

// The horizontal offset a tab is drawn at right now, mid-slide included.
function currentShift(element: HTMLElement): number {
  const transform = getComputedStyle(element).transform;
  return transform && transform !== "none" ? new DOMMatrixReadOnly(transform).m41 : 0;
}

function settleTransition(): string {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "none" : SETTLE;
}

export function TopTabs({
  tabs,
  activeTabId,
  onSelect,
  onClose,
  onOpen,
  onTabContextMenu,
  onReorder,
  actionsDisabled = false,
}: TopTabsProps) {
  const tabListRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<DragSession | null>(null);
  // Each tab's on-screen left edge just before a reorder lands, so the tab can
  // be slid from where it was dropped into the slot the new order gives it.
  const flipRef = useRef<Map<string, number> | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);

  const tabElements = () => [
    ...(tabListRef.current?.querySelectorAll<HTMLElement>("[data-repository-tab]") ?? []),
  ];

  const rememberPositions = () => {
    flipRef.current = new Map(tabElements().map((element) => [
      element.dataset.repositoryTab ?? "",
      element.getBoundingClientRect().left,
    ]));
  };

  // Slides every tab from the position remembered in flipRef to where the
  // layout now puts it. The transforms always end at zero, so whatever happened
  // in between, no tab can be left standing where it was dropped.
  const playFlip = () => {
    const before = flipRef.current;
    if (!before) return;
    flipRef.current = null;
    const moved: HTMLElement[] = [];
    for (const element of tabElements()) {
      element.style.transition = "none";
      element.style.transform = "";
      const previous = before.get(element.dataset.repositoryTab ?? "");
      if (previous === undefined) continue;
      const delta = previous - element.getBoundingClientRect().left;
      if (Math.abs(delta) < 0.5) continue;
      element.style.transform = `translateX(${delta}px)`;
      moved.push(element);
    }
    if (!moved.length) return;
    void tabListRef.current?.offsetWidth;
    const transition = settleTransition();
    for (const element of moved) {
      element.style.transition = transition;
      element.style.transform = "";
    }
  };

  useLayoutEffect(playFlip, [tabs]);

  const startDrag = (tabId: string, event: ReactPointerEvent<HTMLDivElement>) => {
    const tabList = tabListRef.current;
    if (!tabList) return;
    // A press that arrives while an earlier drag is still open means that
    // drag's release was lost; close it before starting over.
    dragRef.current?.end();
    const elements = tabElements();
    const from = elements.findIndex((element) => element.dataset.repositoryTab === tabId);
    if (from < 0) return;

    // Slots come from the layout, not from where a still-sliding tab is drawn.
    const listLeft = tabList.getBoundingClientRect().left;
    const slots = elements.map((element) => {
      const bounds = element.getBoundingClientRect();
      return { left: bounds.left - currentShift(element) - listLeft + tabList.scrollLeft, width: bounds.width };
    });
    const gap = slots.length > 1 ? slots[1].left - slots[0].left - slots[0].width : 4;
    dragRef.current = {
      id: tabId,
      from,
      target: from,
      startClientX: event.clientX,
      startContentX: event.clientX + tabList.scrollLeft,
      startShift: currentShift(elements[from]),
      active: false,
      elements,
      slots,
      gap,
      end: () => finish(),
    };

    const move = (moveEvent: PointerEvent) => {
      const session = dragRef.current;
      if (!session) return;
      // The button came up somewhere this window never heard about.
      if ((moveEvent.buttons & 1) === 0) {
        finish();
        return;
      }
      if (!session.active) {
        if (Math.abs(moveEvent.clientX - session.startClientX) < DRAG_THRESHOLD) return;
        session.active = true;
        try {
          session.elements[session.from].setPointerCapture(moveEvent.pointerId);
        } catch {
          // The pointer is already gone; the next event ends the drag.
        }
        setDraggingId(session.id);
        const transition = settleTransition();
        session.elements.forEach((element, index) => {
          element.style.transition = index === session.from ? "none" : transition;
        });
      }

      const listBounds = tabList.getBoundingClientRect();
      if (moveEvent.clientX < listBounds.left + EDGE_SCROLL_ZONE) tabList.scrollLeft -= EDGE_SCROLL_STEP;
      else if (moveEvent.clientX > listBounds.right - EDGE_SCROLL_ZONE) tabList.scrollLeft += EDGE_SCROLL_STEP;

      const { from: origin, slots } = session;
      const first = slots[0];
      const last = slots[slots.length - 1];
      const own = slots[origin];
      const offset = Math.min(
        Math.max(
          moveEvent.clientX + tabList.scrollLeft - session.startContentX + session.startShift,
          first.left - own.left,
        ),
        last.left + last.width - (own.left + own.width),
      );

      const center = own.left + own.width / 2 + offset;
      let target = origin;
      slots.forEach((slot, index) => {
        const slotCenter = slot.left + slot.width / 2;
        if (index > origin && center > slotCenter) target = Math.max(target, index);
        if (index < origin && center < slotCenter) target = Math.min(target, index);
      });
      session.target = target;

      const shift = own.width + session.gap;
      session.elements.forEach((element, index) => {
        if (index === origin) {
          element.style.transform = `translateX(${offset}px)`;
          return;
        }
        const displacement = origin < index && index <= target
          ? -shift
          : target <= index && index < origin
            ? shift
            : 0;
        element.style.transform = displacement ? `translateX(${displacement}px)` : "";
      });
    };

    const dragged = elements[from];
    const releaseEvents = ["pointerup", "pointercancel", "mouseup"] as const;
    let finished = false;
    const finish = () => {
      if (finished) return;
      finished = true;
      window.removeEventListener("pointermove", move, true);
      for (const name of releaseEvents) window.removeEventListener(name, finish, true);
      window.removeEventListener("blur", finish);
      dragged.removeEventListener("lostpointercapture", finish);
      const session = dragRef.current;
      if (session?.id === tabId) dragRef.current = null;
      if (!session?.active) return;
      setDraggingId(null);
      swallowNextClick();

      // Every tab leaves its drag offset now, whatever happens to the reorder;
      // the slide from the drop point is replayed from the remembered positions.
      rememberPositions();
      for (const element of session.elements) {
        element.style.transition = "none";
        element.style.transform = "";
      }
      if (session.target !== session.from) onReorder(session.id, session.target);
      // A reorder replays the slide from its layout effect; when nothing
      // re-renders -- the tab went back to its own slot -- the frame callback
      // plays it instead.
      requestAnimationFrame(playFlip);
    };

    // Capture phase, so nothing further down can swallow the release.
    window.addEventListener("pointermove", move, true);
    for (const name of releaseEvents) window.addEventListener(name, finish, true);
    // Not in the capture phase: there it would also hear every element losing
    // focus, including the tab the press just moved focus away from.
    window.addEventListener("blur", finish);
    dragged.addEventListener("lostpointercapture", finish);
  };

  // Letting go of a dragged tab also completes a click on it. Moving a tab is
  // not choosing it -- activating would load the repository and hold the row
  // busy -- so that one click is dropped.
  const swallowNextClick = () => {
    const swallow = (event: MouseEvent) => {
      event.stopPropagation();
      event.preventDefault();
    };
    window.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallow, true), 0);
  };

  const shiftTab = (tabId: string, step: -1 | 1) => {
    const index = tabs.findIndex((tab) => tab.id === tabId);
    const target = index + step;
    if (index < 0 || target < 0 || target >= tabs.length) return;
    rememberPositions();
    onReorder(tabId, target);
    requestAnimationFrame(() => {
      tabElements()
        .find((element) => element.dataset.repositoryTab === tabId)
        ?.querySelector<HTMLButtonElement>("[data-tab-main]")
        ?.focus();
    });
  };

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
              dragging={draggingId === tab.id}
              key={tab.id}
              onClose={onClose}
              onContextMenu={onTabContextMenu}
              onDragStart={startDrag}
              onNavigate={navigateFromTab}
              onShift={shiftTab}
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
