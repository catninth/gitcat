import type { ConflictFileContent } from "../../lib/types";

// Why a side or the working copy cannot be shown as text.
export function contentMessage(content: ConflictFileContent): string {
  switch (content.kind) {
    case "missing": return "File does not exist on this side.";
    case "binary": return `Binary content${content.size === undefined ? "" : ` · ${content.size} bytes`}.`;
    // The editor renders every line; past 1 MiB that stops being usable, so
    // a file that large is resolved by taking a side from its menu.
    case "too_large": return `Content is too large for the built-in editor (1 MiB)${content.size === undefined ? "" : ` · ${content.size} bytes`}. Take current or Take incoming from the file's menu, or resolve it in an external tool.`;
    case "text": return "Text content unavailable.";
  }
}
