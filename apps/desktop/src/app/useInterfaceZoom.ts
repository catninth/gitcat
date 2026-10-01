import { useEffect } from "react";

import { applyZoom, DEFAULT_ZOOM_PERCENT, stepZoom } from "../lib/zoom";

// Ctrl+= / Ctrl+- / Ctrl+0 are the zoom keys every browser and GitKraken
// share, so they are fixed rather than offered in the keybind editor.
export function useInterfaceZoom(zoomPercent: number, setZoomPercent: (percent: number) => void) {
    useEffect(() => {
        applyZoom(zoomPercent).catch((error) => console.error("Could not apply interface zoom", error));
    }, [zoomPercent]);

    useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
            let next: number | null = null;
            if (event.key === "=" || event.key === "+") next = stepZoom(zoomPercent, 1);
            else if (event.key === "-") next = stepZoom(zoomPercent, -1);
            else if (event.key === "0") next = DEFAULT_ZOOM_PERCENT;
            if (next === null) return;
            event.preventDefault();
            if (next !== zoomPercent) setZoomPercent(next);
        };
        window.addEventListener("keydown", onKeyDown);
        return () => window.removeEventListener("keydown", onKeyDown);
    }, [setZoomPercent, zoomPercent]);
}
