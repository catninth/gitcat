import {
  Archive,
  ArchiveRestore,
  ChevronDown,
  Download,
  GitBranchPlus,
  PanelRightClose,
  PanelRightOpen,
  Search,
  Settings,
  Upload,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, ReactNode } from "react";

import { cx } from "../../lib";
import type { BranchInfo, PullMode, RepositoryOperationState } from "../../lib/types";
import { MenuHeading, MenuItem, MenuNote, MenuRadio, MenuSurface } from "../menu";
import { IconButton, Spinner } from "../ui";
import { CONFLICT_STATUS_TONE, ConflictIndicatorButton } from "./ConflictIndicatorButton";
import type { ConflictIndicator } from "./ConflictIndicatorButton";
import { RepositoryContext } from "./RepositoryContext";
import { ToolbarAction } from "./ToolbarAction";
import { handleMenuKeyDown } from "./menuKeys";

export type { ConflictIndicator };

export const PULL_LABELS: Record<PullMode, string> = {
  merge: "Pull (merge)",
  fast_forward_only: "Pull (fast-forward only)",
  rebase: "Pull (rebase)",
};

interface ToolbarProps {
  repositoryName: string;
  branchName: string;
  branches: BranchInfo[];
  operation: RepositoryOperationState;
  busy: boolean;
  refreshing?: boolean;
  canStash: boolean;
  canPop: boolean;
  pullMode: PullMode;
  rightPanelVisible: boolean;
  rightPanelKeybind: string;
  searchKeybind: string;
  settingsKeybind: string;
  conflictIndicator: ConflictIndicator;
  conflictTarget: string | null;
  conflictTargets: string[];
  onPullModeChange: (mode: PullMode) => void;
  onPull: (mode: PullMode) => void;
  onPush: () => void;
  onCheckoutBranch: (branch: BranchInfo) => void;
  onCreateBranch: () => void;
  onStash: () => void;
  onStashPop: () => void;
  onSearch: () => void;
  onSettings: () => void;
  onToggleRightPanel: () => void;
  onConflictIndicator: () => void;
  onConflictTargetChange: (target: string | null) => void;
}

export function Toolbar({
  repositoryName,
  branchName,
  branches,
  operation,
  busy,
  refreshing = false,
  canStash,
  canPop,
  pullMode,
  rightPanelVisible,
  rightPanelKeybind,
  searchKeybind,
  settingsKeybind,
  conflictIndicator,
  conflictTarget,
  conflictTargets,
  onPullModeChange,
  onPull,
  onPush,
  onCheckoutBranch,
  onCreateBranch,
  onStash,
  onStashPop,
  onSearch,
  onSettings,
  onToggleRightPanel,
  onConflictIndicator,
  onConflictTargetChange,
}: ToolbarProps) {
  const [pullOpen, setPullOpen] = useState(false);
  const [conflictOpen, setConflictOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const conflictMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!pullOpen) return;
    const close = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setPullOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [pullOpen]);

  useEffect(() => {
    if (!conflictOpen) return;
    const close = (event: MouseEvent) => {
      if (!conflictMenuRef.current?.contains(event.target as Node)) setConflictOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [conflictOpen]);

  useEffect(() => {
    if (!pullOpen && !conflictOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setPullOpen(false);
      setConflictOpen(false);
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [conflictOpen, pullOpen]);

  useEffect(() => {
    if (pullOpen) requestAnimationFrame(() => menuRef.current?.querySelector<HTMLDivElement>("[role='menu']")?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus());
  }, [pullOpen]);

  useEffect(() => {
    if (conflictOpen) requestAnimationFrame(() => conflictMenuRef.current?.querySelector<HTMLDivElement>("[role='menu']")?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus());
  }, [conflictOpen]);

  return (
    <header className="gc-no-select z-15 grid min-h-15.25 flex-[0_0_61px] grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2.75 border-b border-border bg-[color-mix(in_srgb,var(--gc-panel)_91%,black)] px-3 py-2">
      <div className="flex min-w-0 items-center gap-2">
        <RepositoryContext
          branchName={branchName}
          branches={branches}
          disabled={busy}
          onCheckout={onCheckoutBranch}
          repositoryName={repositoryName}
        />
      </div>

      <div className="flex min-w-0 items-center gap-1.25 justify-self-center" aria-label="Repository actions">
        <div className="relative flex items-center gap-px" ref={menuRef}>
          <ToolbarAction
            disabled={busy}
            icon={<Download size={18} />}
            label="Pull"
            onClick={() => onPull(pullMode)}
            title={PULL_LABELS[pullMode]}
          />
          <IconButton
            aria-expanded={pullOpen}
            aria-haspopup="menu"
            aria-label="Choose pull mode"
            className="h-11! w-4.5! border-0! enabled:hover:bg-row-hover!"
            disabled={busy}
            onClick={() => setPullOpen((open) => !open)}
          >
            <ChevronDown size={14} />
          </IconButton>
          {pullOpen ? (
            <MenuSurface
              className="absolute left-0 top-11.5 z-120 w-60.5 rounded-[7px]! p-2!"
              onKeyDown={(event) => handleMenuKeyDown(
                event,
                () => setPullOpen(false),
                () => menuRef.current?.querySelector<HTMLButtonElement>("[aria-haspopup='menu']")?.focus(),
              )}
              role="menu"
            >
              <MenuHeading className="mx-1.75 mb-1.75 mt-0.75 text-[10px] font-bold tracking-[0.07em]">
                Pull behavior
              </MenuHeading>
              {(Object.keys(PULL_LABELS) as PullMode[]).map((mode) => (
                <MenuItem
                  aria-checked={pullMode === mode}
                  density="roomy"
                  key={mode}
                  onClick={() => {
                    onPullModeChange(mode);
                    setPullOpen(false);
                  }}
                  role="menuitemradio"
                >
                  <MenuRadio
                    checked={pullMode === mode}
                    className={pullMode === mode ? "size-2.5 border-[3px]" : "size-2.5 border"}
                  />
                  <span>{PULL_LABELS[mode]}</span>
                </MenuItem>
              ))}
              <MenuNote className="block border-t border-border px-1.75 pb-0.5 pt-1.75">
                Choice overrides global Git pull settings.
              </MenuNote>
            </MenuSurface>
          ) : null}
        </div>
        <ToolbarAction
          disabled={busy}
          icon={<Upload size={18} />}
          label="Push"
          onClick={onPush}
        />
        <ToolbarAction
          disabled={busy}
          icon={<GitBranchPlus size={18} />}
          label="Branch"
          onClick={onCreateBranch}
          title="Create branch at HEAD"
        />
        <ToolbarAction
          disabled={busy || !canStash}
          icon={<Archive size={18} />}
          label="Stash"
          onClick={onStash}
          title={canStash ? "Stash uncommitted changes" : "Nothing to stash"}
        />
        <ToolbarAction
          disabled={busy || !canPop}
          icon={<ArchiveRestore size={18} />}
          label="Pop"
          onClick={onStashPop}
          title={canPop ? "Pop latest stash" : "No stash to pop"}
        />
      </div>

      <div className="flex min-w-0 items-center justify-end gap-1.5">
        {operation !== "normal" ? (
          <span className="rounded border border-[color-mix(in_srgb,var(--gc-warning)_55%,var(--gc-border))] px-2 py-1 text-[10px] font-bold uppercase text-warning">
            {operation.replace("_", " ")}
          </span>
        ) : null}
        {busy || refreshing ? <Spinner label={busy ? "Repository operation running" : "Refreshing repository"} /> : null}
        <div className="relative grid" ref={conflictMenuRef}>
          <ConflictIndicatorButton
            expanded={conflictOpen}
            indicator={conflictIndicator}
            onClick={() => { setConflictOpen((open) => !open); setPullOpen(false); }}
          />
          {conflictOpen ? (
            <MenuSurface
              className="absolute right-0 top-8.75 z-95 flex max-h-[min(430px,calc(100vh-108px))] w-71.5 flex-col"
              onKeyDown={(event) => handleMenuKeyDown(
                event,
                () => setConflictOpen(false),
                () => conflictMenuRef.current?.querySelector<HTMLButtonElement>("[aria-haspopup='menu']")?.focus(),
              )}
              role="menu"
            >
              <MenuItem
                className="flex-col items-start rounded-b-none rounded-t-[3px] border-b border-border"
                density="dense"
                onClick={() => { onConflictIndicator(); setConflictOpen(false); }}
                role="menuitem"
              >
                <strong className={cx("text-[10px]", CONFLICT_STATUS_TONE[conflictIndicator.state])}>
                  {conflictIndicator.label}
                </strong>
                <small className="text-[9px] text-muted">Show status details</small>
              </MenuItem>
              <MenuHeading className="mx-2 mb-1 mt-1.75 text-[9px] font-[750] tracking-[0.06em]">
                Compare current branch with
              </MenuHeading>
              <MenuItem
                aria-checked={!conflictTarget}
                density="dense"
                onClick={() => { onConflictTargetChange(null); setConflictOpen(false); }}
                role="menuitemradio"
              >
                <MenuRadio checked={!conflictTarget} className="size-2 border" filled />
                <span className="overflow-hidden text-ellipsis whitespace-nowrap">No target</span>
              </MenuItem>
              {conflictTargets.map((target) => (
                <MenuItem
                  aria-checked={conflictTarget === target}
                  density="dense"
                  key={target}
                  onClick={() => { onConflictTargetChange(target); setConflictOpen(false); }}
                  role="menuitemradio"
                >
                  <MenuRadio checked={conflictTarget === target} className="size-2 border" filled />
                  <span className="overflow-hidden text-ellipsis whitespace-nowrap">{target}</span>
                </MenuItem>
              ))}
              <MenuNote className="px-2 pb-1 pt-1.75 text-[9px]">
                Read-only merge preview. Your worktree and index are not changed.
              </MenuNote>
            </MenuSurface>
          ) : null}
        </div>
        <IconButton
          aria-label={`${rightPanelVisible ? "Hide" : "Show"} commit panel`}
          onClick={onToggleRightPanel}
          title={`${rightPanelVisible ? "Hide" : "Show"} commit panel (${rightPanelKeybind})`}
        >
          {rightPanelVisible ? <PanelRightClose size={18} /> : <PanelRightOpen size={18} />}
        </IconButton>
        <IconButton aria-label="Search commits" onClick={onSearch} title={`Search commits (${searchKeybind})`}>
          <Search size={18} />
        </IconButton>
        <IconButton aria-label="Settings" onClick={onSettings} title={`Settings (${settingsKeybind})`}>
          <Settings size={18} />
        </IconButton>
      </div>
    </header>
  );
}
