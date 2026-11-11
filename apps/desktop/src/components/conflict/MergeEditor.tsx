import { AlertTriangle, ChevronDown, ChevronUp, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { conflictSideLabels, mergePaneTitle, type ConflictContext } from "../../lib/conflicts";
import {
  baseSlices,
  composeMergeResult,
  defaultSelections,
  outputRows,
  parseConflictMarkers,
  plainRows,
  sideRows,
  unresolvedConflicts,
  type MergeSelection,
  type MergeSide,
} from "../../lib/mergeConflicts";
import type {
  ConflictFileDetails,
  ConflictLineEndingPolicy,
} from "../../lib/types";
import { Button, IconButton, TextArea } from "../ui";
import { MergePane, MergePaneHeader } from "./MergePane";
import { contentMessage } from "./content";

export interface MergeEditorProps {
  busy: boolean;
  closeKeybind?: string;
  /** Names the two sides: "Commit 8c69e9 on main" / "Applied patch or stash". */
  context: ConflictContext;
  details: ConflictFileDetails;
  onClose: () => void;
  onSave: (text: string, lineEnding: ConflictLineEndingPolicy) => void;
}

function sideText(version: ConflictFileDetails["ours"]): string | undefined {
  return version?.content.kind === "text" ? version.content.text : undefined;
}

export function MergeEditor({
  busy,
  closeKeybind,
  context,
  details,
  onClose,
  onSave,
}: MergeEditorProps) {
  const labels = conflictSideLabels(context);
  const resultText = details.result.kind === "text" ? details.result.text ?? "" : undefined;
  const parsed = useMemo(() => parseConflictMarkers(resultText ?? ""), [resultText]);
  const conflicts = parsed.conflicts;

  const [selections, setSelections] = useState<MergeSelection[]>(() => defaultSelections(conflicts.length));
  const [manual, setManual] = useState<string | null>(null);
  const [lineEnding, setLineEnding] = useState<ConflictLineEndingPolicy>("preserve");
  const [activeConflict, setActiveConflict] = useState(0);

  // A reload of the same file (after a refresh, or after picking another file)
  // starts the composition over; keeping stale picks would silently apply them
  // to different content.
  const fingerprint = `${details.path}:${details.expected_state.result.sha256 ?? ""}:${conflicts.length}`;
  const oursRef = useRef<HTMLDivElement>(null);
  const theirsRef = useRef<HTMLDivElement>(null);
  const outputRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setSelections(defaultSelections(conflicts.length));
    setManual(null);
    setLineEnding("preserve");
    setActiveConflict(0);
    // A conflict deep in a long file is what the editor was opened for, so it
    // opens there rather than at line one.
    if (!conflicts.length) return;
    const frame = requestAnimationFrame(() => {
      for (const pane of [oursRef, theirsRef, outputRef]) {
        pane.current?.querySelector<HTMLElement>('[data-conflict="0"]')?.scrollIntoView({ block: "center" });
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [fingerprint]);
  const syncingRef = useRef(false);

  const oursText = sideText(details.ours);
  const theirsText = sideText(details.theirs);
  const editable = resultText !== undefined || oursText !== undefined || theirsText !== undefined;

  const oursPaneRows = useMemo(
    () => (conflicts.length ? sideRows(parsed.segments, "ours") : plainRows(oursText ?? "")),
    [conflicts.length, oursText, parsed],
  );
  const theirsPaneRows = useMemo(
    () => (conflicts.length ? sideRows(parsed.segments, "theirs") : plainRows(theirsText ?? "")),
    [conflicts.length, parsed, theirsText],
  );
  const composed = useMemo(
    () => (conflicts.length ? composeMergeResult(parsed.segments, selections) : resultText ?? ""),
    [conflicts.length, parsed, resultText, selections],
  );
  const basePreview = useMemo(
    () => baseSlices(parsed.segments, sideText(details.base)),
    [details.base, parsed],
  );
  const resultRows = useMemo(
    () => (conflicts.length ? outputRows(parsed.segments, selections, basePreview) : plainRows(composed)),
    [basePreview, composed, conflicts.length, parsed, selections],
  );
  const unresolved = useMemo(() => unresolvedConflicts(conflicts, selections), [conflicts, selections]);

  const text = manual ?? composed;
  const mixedNeedsChoice = details.result.line_ending === "mixed" && lineEnding === "preserve";
  const canSave = !busy && editable && !mixedNeedsChoice && (manual !== null || !unresolved.length);

  const allSelected = (side: MergeSide) => conflicts.length > 0 && selections.every((selection) => selection[side]);
  const dirty = manual !== null || selections.some((selection) => selection.ours || selection.theirs);

  const toggleSide = (side: MergeSide) => {
    const next = !allSelected(side);
    setSelections((current) => current.map((selection) => ({ ...selection, [side]: next })));
  };

  const reset = () => {
    setSelections(defaultSelections(conflicts.length));
    setManual(null);
  };

  const toggleConflict = (index: number, side: MergeSide) => {
    setActiveConflict(index);
    setSelections((current) => current.map((selection, position) => (
      position === index ? { ...selection, [side]: !selection[side] } : selection
    )));
  };

  const scrollToConflict = (index: number) => {
    setActiveConflict(index);
    for (const pane of [oursRef, theirsRef, outputRef]) {
      pane.current
        ?.querySelector<HTMLElement>(`[data-conflict="${index}"]`)
        ?.scrollIntoView({ block: "center" });
    }
  };

  const stepConflict = (direction: 1 | -1) => {
    if (!conflicts.length) return;
    scrollToConflict((activeConflict + direction + conflicts.length) % conflicts.length);
  };

  // The two side panes show the same common context, so scrolling one and not
  // the other would misalign the very lines the user is comparing.
  const syncScroll = (from: typeof oursRef, to: typeof theirsRef) => () => {
    if (syncingRef.current || !from.current || !to.current) return;
    syncingRef.current = true;
    to.current.scrollTop = from.current.scrollTop;
    to.current.scrollLeft = from.current.scrollLeft;
    requestAnimationFrame(() => { syncingRef.current = false; });
  };

  return (
    <section aria-label={`Merge editor for ${details.path}`} className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
      <header className="flex min-h-12 flex-[0_0_auto] items-center gap-2.5 border-b border-border bg-[color-mix(in_srgb,var(--gc-panel)_66%,var(--gc-background))] py-1.5 pl-3.25 pr-2.5">
        <AlertTriangle aria-hidden="true" className="shrink-0 text-warning" size={16} />
        <h2 className="m-0 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[12px] font-[550] text-foreground" title={details.path}>
          {details.path}
          {conflicts.length ? ` (${conflicts.length} conflict${conflicts.length === 1 ? "" : "s"})` : ""}
        </h2>
        <small className="shrink-0 text-[10px] text-muted">
          {conflicts.length
            ? `${conflicts.length - unresolved.length}/${conflicts.length} resolved`
            : "no conflict markers"}
        </small>
        <span className="flex-1" />
        <Button compact disabled={!canSave} onClick={() => onSave(text, lineEnding)} tone="accent">
          Save &amp; stage
        </Button>
        <IconButton
          aria-label="Back to graph"
          onClick={onClose}
          title={closeKeybind ? `Back to graph (${closeKeybind})` : "Back to graph"}
        >
          <X size={16} />
        </IconButton>
      </header>

      <div className="flex min-h-0 min-w-0 flex-[3] border-b border-border">
        <MergePane
          ariaLabel={`${labels.ours} lines`}
          busy={busy}
          emptyMessage={conflicts.length || oursText !== undefined ? undefined : contentMessage(details.ours?.content ?? { kind: "missing" })}
          header={(
            <MergePaneHeader
              badge="A"
              badgeSide="ours"
              checked={allSelected("ours")}
              label={<span title={labels.oursDescription}>{mergePaneTitle("ours", context)}</span>}
              onToggleAll={conflicts.length && manual === null ? () => toggleSide("ours") : undefined}
            />
          )}
          isSelected={(index) => Boolean(selections[index]?.ours)}
          onScroll={syncScroll(oursRef, theirsRef)}
          onToggleConflict={manual === null ? (index) => toggleConflict(index, "ours") : undefined}
          rows={oursPaneRows}
          scrollRef={oursRef}
        />
        <MergePane
          ariaLabel={`${labels.theirs} lines`}
          busy={busy}
          emptyMessage={conflicts.length || theirsText !== undefined ? undefined : contentMessage(details.theirs?.content ?? { kind: "missing" })}
          header={(
            <MergePaneHeader
              badge="B"
              badgeSide="theirs"
              checked={allSelected("theirs")}
              label={<span title={labels.theirsDescription}>{mergePaneTitle("theirs", context, parsed.conflicts[0]?.theirsLabel)}</span>}
              onToggleAll={conflicts.length && manual === null ? () => toggleSide("theirs") : undefined}
            />
          )}
          isSelected={(index) => Boolean(selections[index]?.theirs)}
          onScroll={syncScroll(theirsRef, oursRef)}
          onToggleConflict={manual === null ? (index) => toggleConflict(index, "theirs") : undefined}
          rows={theirsPaneRows}
          scrollRef={theirsRef}
        />
      </div>

      <div className="flex min-h-0 min-w-0 flex-[2] flex-col">
        <header className="relative flex h-9 flex-[0_0_auto] items-center gap-2 border-b border-border bg-panel/74 px-2.5">
          <strong className="shrink-0 text-[11px]">Output</strong>
          <small className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[10px] text-muted">
            {text.length.toLocaleString()} characters · {details.result.line_ending?.replace("cr_lf", "CRLF").toUpperCase() ?? "new file"}
          </small>
          {conflicts.length ? (
            <span className="absolute left-1/2 flex -translate-x-1/2 items-center gap-1.5">
              <small className="text-[10px] text-accent">conflict {activeConflict + 1} of {conflicts.length}</small>
              <IconButton aria-label="Previous conflict" className="size-6!" onClick={() => stepConflict(-1)} title="Previous conflict">
                <ChevronUp size={14} />
              </IconButton>
              <IconButton aria-label="Next conflict" className="size-6!" onClick={() => stepConflict(1)} title="Next conflict">
                <ChevronDown size={14} />
              </IconButton>
            </span>
          ) : null}
          <span className="ml-auto flex shrink-0 items-center gap-1.5">
            {details.result.line_ending === "mixed" ? (
              <select
                aria-label="Result line ending policy"
                className="rounded border border-border bg-panel px-1.5 py-0.75 text-[10px] text-foreground"
                disabled={busy}
                onChange={(event) => setLineEnding(event.target.value as ConflictLineEndingPolicy)}
                value={lineEnding}
              >
                <option value="preserve">Mixed line endings — choose…</option>
                <option value="lf">Normalize to LF</option>
                <option value="cr_lf">Normalize to CRLF</option>
              </select>
            ) : null}
            <Button
              compact
              disabled={busy || !editable}
              onClick={() => {
                if (manual === null) setManual(composed);
                else if (!manual.length || manual === composed || window.confirm("Discard the manually edited result?")) setManual(null);
              }}
            >
              {manual === null ? "Edit manually" : "Back to picking"}
            </Button>
            <Button compact disabled={busy || !dirty} onClick={reset}>Reset</Button>
          </span>
        </header>
        {manual !== null ? (
          <TextArea
            aria-label="Resolved file content"
            className="block min-h-0 w-full flex-1 resize-none whitespace-pre border-0 bg-background px-3 py-2 font-mono text-[11px] leading-[1.7] text-foreground outline-0 tab-2 focus:shadow-[inset_0_0_0_1px_var(--gc-accent)]"
            disabled={busy}
            onChange={(event) => setManual(event.target.value)}
            spellCheck={false}
            value={manual}
          />
        ) : editable ? (
          <MergePane ariaLabel="Merged result lines" header={null} rows={resultRows} scrollRef={outputRef} showSideTag />
        ) : (
          <div className="flex min-h-0 flex-1 items-center justify-center gap-2 p-4 text-center text-[11px] text-muted">
            <AlertTriangle aria-hidden="true" className="text-warning" size={16} />
            {contentMessage(details.result)}
          </div>
        )}
      </div>
    </section>
  );
}
