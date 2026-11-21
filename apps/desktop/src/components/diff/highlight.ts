import { createContext, useContext, useEffect, useState } from "react";

import { languageForPath, tokenizeLinesAsync } from "../../lib/syntax";
import type { SyntaxToken } from "../../lib/syntax";
import type { DiffHunk, DiffLine, FileDiff } from "../../lib/types";
import type { ChangeRange, IntralineMap } from "./intraline";

export type HighlightMap = ReadonlyMap<DiffLine, SyntaxToken[]>;

const MAX_HIGHLIGHT_LINES = 12000;

export const HighlightContext = createContext<HighlightMap | null>(null);

export const IntralineContext = createContext<IntralineMap | null>(null);

export function useLineChanges(line: DiffLine): readonly ChangeRange[] | undefined {
  return useContext(IntralineContext)?.get(line);
}

export function useHighlightedLine(line: DiffLine): SyntaxToken[] | null {
  return useContext(HighlightContext)?.get(line) ?? null;
}

async function tokenizeSide(
  hunk: DiffHunk,
  side: "old" | "new",
  lang: string,
  into: Map<DiffLine, SyntaxToken[]>,
): Promise<void> {
  const changed = side === "old" ? "deletion" : "addition";
  const lines = hunk.lines.filter((line) => line.kind === "context" || line.kind === changed);
  if (side === "old" && !lines.some((line) => line.kind === "deletion")) return;
  if (lines.length === 0) return;

  const tokens = await tokenizeLinesAsync(lines.map((line) => line.content).join("\n"), lang);
  if (!tokens) return;

  lines.forEach((line, index) => {
    const lineTokens = tokens[index];
    if (!lineTokens) return;
    if (side === "new" || line.kind === "deletion") into.set(line, lineTokens);
  });
}

async function buildHighlightMap(diff: FileDiff): Promise<HighlightMap | null> {
  const newLang = languageForPath(diff.new_path);
  const oldLang = languageForPath(diff.old_path ?? diff.new_path);
  if (!newLang && !oldLang) return null;

  const map = new Map<DiffLine, SyntaxToken[]>();
  for (const hunk of diff.hunks) {
    if (oldLang) await tokenizeSide(hunk, "old", oldLang, map);
    if (newLang) await tokenizeSide(hunk, "new", newLang, map);
  }

  return map.size === 0 ? null : map;
}

function carryOverHighlight(diff: FileDiff, previous: HighlightMap | null): HighlightMap | null {
  if (!previous || previous.size === 0) return null;

  const byContent = new Map<string, SyntaxToken[]>();
  for (const [line, tokens] of previous) {
    if (!byContent.has(line.content)) byContent.set(line.content, tokens);
  }

  const seeded = new Map<DiffLine, SyntaxToken[]>();
  for (const hunk of diff.hunks) {
    for (const line of hunk.lines) {
      const tokens = byContent.get(line.content);
      if (tokens) seeded.set(line, tokens);
    }
  }

  return seeded.size === 0 ? null : seeded;
}

export function useDiffHighlight(diff: FileDiff | null): HighlightMap | null {
  const [map, setMap] = useState<HighlightMap | null>(null);

  useEffect(() => {
    if (!diff || diff.binary) {
      setMap(null);
      return;
    }

    const lineCount = diff.hunks.reduce((total, hunk) => total + hunk.lines.length, 0);
    if (lineCount === 0 || lineCount > MAX_HIGHLIGHT_LINES) {
      setMap(null);
      return;
    }

    setMap((previous) => carryOverHighlight(diff, previous));

    let cancelled = false;
    void buildHighlightMap(diff).then((result) => {
      if (!cancelled) setMap(result);
    });

    return () => { cancelled = true; };
  }, [diff]);

  return map;
}
