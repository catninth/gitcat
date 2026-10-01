import { ChevronDown, ChevronRight, GitBranch, Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";

import { cx } from "../../lib";
import type { BranchInfo } from "../../lib/types";
import { MenuItem, MenuNote, MenuSurface } from "../menu";
import { Input } from "../ui";
import { handleMenuKeyDown } from "./menuKeys";

const CAPTION = "text-[10px] leading-none text-muted";
const VALUE = "overflow-hidden text-ellipsis whitespace-nowrap text-[15px] font-semibold leading-tight";

// Repository / branch breadcrumb on the left of the toolbar. The repository is
// a plain label; the branch opens a filterable list that checks a branch out.
export function RepositoryContext({
  branchName,
  branches,
  disabled = false,
  onCheckout,
  repositoryName,
}: {
  branchName: string;
  branches: BranchInfo[];
  disabled?: boolean;
  onCheckout: (branch: BranchInfo) => void;
  repositoryName: string;
}) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const hostRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const visible = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    return needle ? branches.filter((branch) => branch.name.toLowerCase().includes(needle)) : branches;
  }, [branches, filter]);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (!hostRef.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    requestAnimationFrame(() => searchRef.current?.focus());
    return () => window.removeEventListener("mousedown", close);
  }, [open]);

  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);

  const choose = (branch: BranchInfo) => {
    setOpen(false);
    if (!branch.is_head) onCheckout(branch);
  };

  const onSearchKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      hostRef.current?.querySelector<HTMLButtonElement>("[role='menu'] button:not(:disabled)")?.focus();
    } else if (event.key === "Enter" && visible.length > 0) {
      event.preventDefault();
      choose(visible[0]);
    }
  };

  return (
    <div className="flex min-w-0 items-center gap-2">
      <div className="flex min-w-20 max-w-42.5 flex-col gap-1 px-1.5">
        <span className={CAPTION}>repository</span>
        <strong className={VALUE}>{repositoryName}</strong>
      </div>
      <ChevronRight aria-hidden="true" className="shrink-0 text-muted" size={18} />
      <div className="relative min-w-0" ref={hostRef}>
        <button
          aria-expanded={open}
          aria-haspopup="menu"
          aria-label={`Branch ${branchName}, switch branch`}
          className={cx(
            "flex min-w-25 max-w-52 cursor-pointer flex-col gap-1 rounded px-1.5 py-1 text-left text-foreground",
            "enabled:hover:bg-row-hover disabled:cursor-default focus-visible:outline-1 focus-visible:outline-accent",
            open && "bg-row-hover",
          )}
          disabled={disabled}
          onClick={() => {
            setFilter("");
            setOpen((value) => !value);
          }}
          ref={triggerRef}
          type="button"
        >
          <span className={CAPTION}>branch</span>
          <span className="flex min-w-0 items-center gap-2">
            <strong className={VALUE}>{branchName}</strong>
            <ChevronDown className="shrink-0 text-muted" size={14} />
          </span>
        </button>
        {open ? (
          <div
            className="absolute left-0 top-[calc(100%+4px)] z-120 flex w-64 flex-col gap-1.5 rounded-md border border-border bg-menu p-1.5 shadow-panel"
            onKeyDown={(event) => {
              if (event.key !== "Escape") return;
              event.preventDefault();
              setOpen(false);
              requestAnimationFrame(() => triggerRef.current?.focus());
            }}
          >
            <div className="flex h-8 items-center gap-2 rounded-[5px] border border-border bg-background px-2.25 text-muted focus-within:border-accent focus-within:text-accent">
              <Search size={14} />
              <Input
                aria-label="Search branches"
                className="min-w-0 flex-1 border-0 bg-transparent text-foreground outline-0 placeholder:text-muted"
                onChange={(event) => setFilter(event.target.value)}
                onKeyDown={onSearchKeyDown}
                placeholder="Search"
                ref={searchRef}
                value={filter}
              />
            </div>
            <MenuSurface
              className="max-h-[min(360px,calc(100vh-140px))] border-0 bg-transparent p-0 shadow-none"
              onKeyDown={(event) => handleMenuKeyDown(
                event,
                () => setOpen(false),
                () => triggerRef.current?.focus(),
              )}
              role="menu"
            >
              {visible.map((branch) => (
                <MenuItem
                  aria-current={branch.is_head ? "true" : undefined}
                  className={cx("w-full", branch.is_head && "bg-success/14!")}
                  density="dense"
                  key={branch.full_name}
                  onClick={() => choose(branch)}
                  role="menuitem"
                  title={branch.is_head ? `${branch.name} (checked out)` : `Check out ${branch.name}`}
                >
                  <GitBranch
                    className={cx(
                      "shrink-0",
                      branch.is_head
                        ? "text-[color-mix(in_srgb,var(--gc-success)_70%,var(--gc-text))]"
                        : "text-muted",
                    )}
                    size={13}
                  />
                  <span className="overflow-hidden text-ellipsis whitespace-nowrap">{branch.name}</span>
                </MenuItem>
              ))}
              {visible.length === 0 ? (
                <MenuNote className="block px-2 py-1.5">No matching branch</MenuNote>
              ) : null}
            </MenuSurface>
          </div>
        ) : null}
      </div>
    </div>
  );
}
