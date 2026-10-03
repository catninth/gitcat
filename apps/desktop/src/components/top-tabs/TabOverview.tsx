import { Check, House, LayoutList, Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { cx } from "../../lib";
import { MenuSurface } from "../menu";
import { IconButton } from "../ui";
import type { TabView } from "./RepositoryTab";
import { repositoryLocation, repositoryTabDescription } from "./tabPresentation";

interface TabOverviewProps {
  activeTabId?: string;
  disabled: boolean;
  onSelect: (tabId: string) => void;
  tabs: TabView[];
}

function OverviewRow({
  active,
  highlighted,
  onHover,
  onSelect,
  showLocation,
  tab,
}: {
  active: boolean;
  highlighted: boolean;
  onHover: () => void;
  onSelect: () => void;
  showLocation: boolean;
  tab: TabView;
}) {
  return (
    <button
      aria-current={active ? "page" : undefined}
      aria-label={repositoryTabDescription(tab)}
      className={cx(
        "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2.5 text-left transition-colors duration-75",
        highlighted ? "bg-foreground/7 text-foreground" : "text-muted",
      )}
      data-overview-row={tab.id}
      onClick={onSelect}
      onMouseMove={onHover}
      tabIndex={-1}
      title={tab.path}
      type="button"
    >
      {tab.kind === "start" ? <House className="shrink-0" size={13} strokeWidth={1.9} /> : null}
      <span
        className={cx(
          "min-w-0 truncate text-[12px]",
          active ? "font-[620] text-foreground" : "font-[540]",
          tab.unavailable && "text-danger line-through decoration-danger/50",
        )}
      >
        {tab.label}
      </span>
      {/* Only a name shared by two open repositories needs its folder to tell them apart. */}
      {showLocation ? (
        <span className="min-w-0 shrink truncate font-mono text-[10px] text-muted/80">
          {repositoryLocation(tab.path)}
        </span>
      ) : null}
      <span className="flex-1" />
      {active ? <Check aria-hidden="true" className="shrink-0 text-accent" size={14} /> : null}
    </button>
  );
}

export function TabOverview({
  activeTabId,
  disabled,
  onSelect,
  tabs,
}: TabOverviewProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [highlightedId, setHighlightedId] = useState<string | undefined>();
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const tabCount = tabs.length;

  const visibleTabs = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle) return tabs;
    return tabs.filter((tab) => `${tab.label} ${tab.path}`.toLocaleLowerCase().includes(needle));
  }, [query, tabs]);

  const duplicateLabels = useMemo(() => {
    const counts = new Map<string, number>();
    for (const tab of tabs) counts.set(tab.label, (counts.get(tab.label) ?? 0) + 1);
    return new Set([...counts].filter(([, count]) => count > 1).map(([label]) => label));
  }, [tabs]);

  const highlighted = visibleTabs.find((tab) => tab.id === highlightedId) ?? visibleTabs[0];

  const choose = (tabId: string) => {
    onSelect(tabId);
    setOpen(false);
  };

  const moveHighlight = (step: number) => {
    if (!visibleTabs.length) return;
    const index = highlighted ? visibleTabs.indexOf(highlighted) : -1;
    const next = visibleTabs[(index + step + visibleTabs.length) % visibleTabs.length];
    setHighlightedId(next.id);
    requestAnimationFrame(() => {
      [...(rootRef.current?.querySelectorAll<HTMLElement>("[data-overview-row]") ?? [])]
        .find((element) => element.dataset.overviewRow === next.id)
        ?.scrollIntoView({ block: "nearest" });
    });
  };

  useEffect(() => {
    if (!open) return;

    const closeOnOutsideClick = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };

    window.addEventListener("pointerdown", closeOnOutsideClick);
    window.addEventListener("keydown", closeOnEscape);
    requestAnimationFrame(() => inputRef.current?.focus());
    return () => {
      window.removeEventListener("pointerdown", closeOnOutsideClick);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  return (
    <div className="relative" ref={rootRef}>
      <IconButton
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label={`Browse ${tabCount} open ${tabCount === 1 ? "repository" : "repositories"}`}
        className={cx(
          "relative size-7! rounded-md!",
          open && "border-border-strong! bg-control-hover! text-foreground!",
        )}
        disabled={disabled}
        onClick={() => {
          setOpen((current) => !current);
          setQuery("");
          setHighlightedId(activeTabId);
        }}
        title="All open repositories"
      >
        <LayoutList size={15} />
        {tabCount ? (
          <span className="absolute -right-1 -top-1 grid min-w-3.75 place-items-center rounded-full bg-foreground/12 px-0.75 font-mono text-[8px] font-bold leading-3.75 text-foreground/80">
            {tabCount}
          </span>
        ) : null}
      </IconButton>

      {open ? (
        <MenuSurface
          aria-label="Open repositories"
          className="absolute right-0 top-[calc(100%+6px)] z-80 flex max-h-[min(420px,calc(100vh-80px))] w-[min(320px,calc(100vw-18px))] flex-col overflow-hidden p-0!"
          role="dialog"
        >
          <label className="flex h-10 flex-[0_0_auto] items-center gap-2 border-b border-border px-3 text-muted">
            <Search aria-hidden="true" size={14} />
            <input
              aria-label="Find open repository"
              className="min-w-0 flex-1 bg-transparent text-[12.5px] text-foreground outline-none placeholder:text-muted/70"
              onChange={(event) => {
                setQuery(event.target.value);
                setHighlightedId(undefined);
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                  event.preventDefault();
                  moveHighlight(event.key === "ArrowDown" ? 1 : -1);
                } else if (event.key === "Enter" && highlighted) {
                  event.preventDefault();
                  choose(highlighted.id);
                }
              }}
              placeholder="Find repository"
              ref={inputRef}
              type="search"
              value={query}
            />
          </label>

          <div className="min-h-0 overflow-y-auto p-1">
            {visibleTabs.length ? visibleTabs.map((tab) => (
              <OverviewRow
                active={tab.id === activeTabId}
                highlighted={tab.id === highlighted?.id}
                key={tab.id}
                onHover={() => setHighlightedId(tab.id)}
                onSelect={() => choose(tab.id)}
                showLocation={duplicateLabels.has(tab.label)}
                tab={tab}
              />
            )) : (
              <div className="px-3 py-6 text-center text-[11.5px] text-muted">
                No repository matches “{query.trim()}”
              </div>
            )}
          </div>
        </MenuSurface>
      ) : null}
    </div>
  );
}
