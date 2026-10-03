import { Keyboard, RotateCcw } from "lucide-react";
import { useState } from "react";

import { cx } from "../../lib";
import {
  DEFAULT_KEYBINDS,
  KEYBIND_DEFINITIONS,
  keybindFromEvent,
  keybindValidationError,
  type KeybindAction,
} from "../../lib/keybinds";
import type { KeybindSettings } from "../../lib/types";
import { SectionHeading } from "./SettingsField";

// Sits on the row border, so it needs the panel background to punch through it.
function KeybindError({ children }: { children: string }) {
  return <span className="absolute -bottom-1.25 right-10.75 bg-panel px-0.75 text-[10px] text-danger">{children}</span>;
}

export function KeybindEditor({
  duplicateActions,
  keybinds,
  onChange,
}: {
  duplicateActions: ReadonlySet<KeybindAction>;
  keybinds: KeybindSettings;
  onChange: (action: KeybindAction, binding: string) => void;
}) {
  const [recording, setRecording] = useState<KeybindAction | null>(null);
  const [captureError, setCaptureError] = useState<{ action: KeybindAction; message: string } | null>(null);

  const capture = (action: KeybindAction, event: React.KeyboardEvent<HTMLButtonElement>) => {
    if (recording !== action) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setRecording(null);
      return;
    }
    if (event.key === "Backspace" || event.key === "Delete") {
      onChange(action, "");
      setCaptureError(null);
      setRecording(null);
      return;
    }
    const binding = keybindFromEvent(event.nativeEvent);
    if (!binding) return;
    const validationError = keybindValidationError(binding);
    if (validationError) {
      setCaptureError({ action, message: validationError });
      setRecording(null);
      return;
    }
    onChange(action, binding);
    setCaptureError(null);
    setRecording(null);
  };

  return (
    <section>
      <SectionHeading className="flex items-center gap-1.75"><Keyboard size={14} /> Keybinds</SectionHeading>
      <p className="-mt-1.25 mb-3 text-[12px] text-muted">
        Click a shortcut, then press the new key combination. Backspace/Delete clears it. Duplicate and reserved shortcuts are rejected.
      </p>
      <div className="grid grid-cols-2 gap-x-3 gap-y-1.5 max-[1080px]:grid-cols-1">
        {KEYBIND_DEFINITIONS.map((definition) => {
          const duplicate = duplicateActions.has(definition.action);
          return (
            <div
              className={cx(
                "relative grid min-w-0 grid-cols-[minmax(0,1fr)_136px_27px] items-center gap-1.75 rounded-[5px] border bg-background/48 px-2 py-1.75",
                duplicate ? "border-[color-mix(in_srgb,var(--gc-danger)_65%,var(--gc-border))]" : "border-border",
              )}
              key={definition.action}
            >
              <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-x-1.75 gap-y-0.5">
                <strong className="overflow-hidden text-ellipsis whitespace-nowrap text-[12px]">{definition.label}</strong>
                <span className="col-span-full overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-muted">{definition.description}</span>
                <small className="col-start-2 row-start-1 text-[10px] uppercase text-[color-mix(in_srgb,var(--gc-accent)_75%,var(--gc-muted))]">{definition.scope}</small>
              </div>
              <button
                aria-label={`Change ${definition.label} shortcut`}
                className={cx(
                  "grid h-7.25 cursor-pointer place-items-center rounded border px-1.75",
                  recording === definition.action
                    ? "border-accent bg-[color-mix(in_srgb,var(--gc-accent)_9%,var(--gc-background))] text-accent"
                    : "border-border bg-background text-foreground hover:border-muted",
                )}
                onBlur={() => setRecording((current) => current === definition.action ? null : current)}
                onClick={() => {
                  setCaptureError(null);
                  setRecording(definition.action);
                }}
                onKeyDown={(event) => capture(definition.action, event)}
                type="button"
              >
                <kbd className="overflow-hidden text-ellipsis whitespace-nowrap text-[11px]">
                  {recording === definition.action ? "Press keys…" : keybinds[definition.action] || "Unassigned"}
                </kbd>
              </button>
              <button
                aria-label={`Reset ${definition.label} shortcut`}
                className="grid size-6.75 cursor-pointer place-items-center rounded bg-transparent text-muted enabled:hover:bg-row-hover enabled:hover:text-foreground"
                disabled={keybinds[definition.action] === DEFAULT_KEYBINDS[definition.action]}
                onClick={() => onChange(definition.action, DEFAULT_KEYBINDS[definition.action])}
                title="Reset shortcut"
                type="button"
              >
                <RotateCcw size={13} />
              </button>
              {duplicate ? <KeybindError>Duplicate</KeybindError> : null}
              {captureError?.action === definition.action ? <KeybindError>{captureError.message}</KeybindError> : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}
