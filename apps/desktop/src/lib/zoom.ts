import { invokeTauri, isTauriEnvironment } from "./platform";

// Mirrors the bounds in gitcat-contracts' `AppSettings`.
export const MIN_ZOOM_PERCENT = 80;
export const MAX_ZOOM_PERCENT = 300;
export const DEFAULT_ZOOM_PERCENT = 110;

/** The steps the status bar and preferences offer, and that the hotkeys walk. */
export const ZOOM_LEVELS = [80, 90, 100, 110, 120, 130, 140, 150, 175, 200, 250, 300] as const;

/** The next offered level above or below `current`, or `current` at either end. */
export function stepZoom(current: number, direction: 1 | -1): number {
    if (direction > 0) return ZOOM_LEVELS.find((level) => level > current) ?? current;
    return [...ZOOM_LEVELS].reverse().find((level) => level < current) ?? current;
}

let requestedZoom: number | null = null;
let appliedZoom: number | null = null;
let pending: Promise<void> = Promise.resolve();

// The native webview zoom scales layout, hit-testing and device pixels
// together, the same way a browser zoom does. CSS `zoom` is only the fallback
// for the browser preview, where there is no webview to ask.
//
// Requests are chained and each step applies the latest one: closing the
// preferences restores the saved zoom in the same commit that applies a new
// one, and the two must not land out of order.
export function applyZoom(percent: number): Promise<void> {
    requestedZoom = percent;
    const step = pending.then(async () => {
        const target = requestedZoom;
        if (target === null || target === appliedZoom) return;
        if (isTauriEnvironment()) {
            await invokeTauri("set_interface_zoom", { percent: target });
        } else {
            document.documentElement.style.setProperty("zoom", String(target / 100));
        }
        appliedZoom = target;
    });
    pending = step.catch(() => undefined);
    return step;
}
