import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent, RefObject } from "react";

import type { DiffMap, MarkKind } from "./rows";

const MARK_BACKGROUND: Record<MarkKind, string> = {
  addition: "color-mix(in srgb, var(--gc-success) 82%, transparent)",
  deletion: "color-mix(in srgb, var(--gc-danger) 82%, transparent)",
  change:
    "linear-gradient(to right, color-mix(in srgb, var(--gc-danger) 82%, transparent) 0 50%, color-mix(in srgb, var(--gc-success) 82%, transparent) 50% 100%)",
};

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

interface MarkBox {
  top: number;
  height: number;
}

// Places each mark where its rows actually sit in the scroller, as a fraction of
// the scrollable height. A file shorter than the viewport therefore marks only the
// part of the track its lines cover, and hunk headers between rows are accounted for.
function measureMarks(map: DiffMap, scroller: HTMLElement): MarkBox[] | null {
  const rows = scroller.querySelectorAll<HTMLElement>("tr.gc-diff-line");
  if (rows.length < map.totalRows || scroller.offsetHeight === 0) return null;

  const box = scroller.getBoundingClientRect();
  const scale = box.height / scroller.offsetHeight || 1;
  const origin = box.top - scroller.scrollTop * scale;
  const total = Math.max(scroller.scrollHeight, scroller.clientHeight) * scale;

  return map.marks.map((mark) => {
    const first = rows[mark.start].getBoundingClientRect();
    const last = rows[mark.start + mark.length - 1].getBoundingClientRect();
    return {
      top: clamp((first.top - origin) / total, 0, 1),
      height: clamp((last.bottom - first.top) / total, 0, 1),
    };
  });
}

export function DiffMinimap({ map, scrollRef }: {
  map: DiffMap;
  scrollRef: RefObject<HTMLDivElement | null>;
}) {
  const trackRef = useRef<HTMLDivElement>(null);
  const thumbRef = useRef<HTMLDivElement>(null);
  const dragOffsetRef = useRef(0);
  const pendingClientYRef = useRef<number | null>(null);
  const frameRef = useRef(0);
  const [boxes, setBoxes] = useState<{ map: DiffMap; boxes: MarkBox[] | null } | null>(null);

  const syncThumb = useCallback(() => {
    const scroller = scrollRef.current;
    const thumb = thumbRef.current;
    if (!scroller || !thumb) return;

    const total = scroller.scrollHeight || 1;
    thumb.style.top = `${clamp(scroller.scrollTop / total, 0, 1) * 100}%`;
    thumb.style.height = `max(6px, ${clamp(scroller.clientHeight / total, 0, 1) * 100}%)`;
  }, [scrollRef]);

  // A layout effect, so the first measurement lands before the browser paints and
  // the marks never show at an unmeasured position.
  useLayoutEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;

    const sync = () => {
      syncThumb();
      setBoxes({ map, boxes: measureMarks(map, scroller) });
    };
    sync();
    scroller.addEventListener("scroll", syncThumb, { passive: true });
    const observer = new ResizeObserver(sync);
    observer.observe(scroller);
    for (const child of Array.from(scroller.children)) observer.observe(child);

    return () => {
      scroller.removeEventListener("scroll", syncThumb);
      observer.disconnect();
    };
  }, [map, scrollRef, syncThumb]);

  useEffect(() => () => {
    if (frameRef.current) cancelAnimationFrame(frameRef.current);
  }, []);

  const pointerFraction = useCallback((clientY: number): number | null => {
    const track = trackRef.current;
    if (!track) return null;

    const rect = track.getBoundingClientRect();
    if (rect.height === 0) return null;
    return clamp((clientY - rect.top) / rect.height, 0, 1);
  }, []);

  const applyPendingScroll = useCallback(() => {
    frameRef.current = 0;
    const clientY = pendingClientYRef.current;
    const scroller = scrollRef.current;
    if (clientY === null || !scroller) return;
    pendingClientYRef.current = null;

    const fraction = pointerFraction(clientY);
    if (fraction === null) return;

    const total = scroller.scrollHeight || 1;
    const thumb = clamp(scroller.clientHeight / total, 0, 1);
    const top = clamp(fraction - dragOffsetRef.current, 0, Math.max(0, 1 - thumb));
    scroller.scrollTop = top * total;
    syncThumb();
  }, [pointerFraction, scrollRef, syncThumb]);

  const queueScroll = useCallback((clientY: number) => {
    pendingClientYRef.current = clientY;
    if (frameRef.current) return;
    frameRef.current = requestAnimationFrame(applyPendingScroll);
  }, [applyPendingScroll]);

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    const scroller = scrollRef.current;
    const fraction = pointerFraction(event.clientY);
    if (!scroller || fraction === null) return;

    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);

    const total = scroller.scrollHeight || 1;
    const thumb = clamp(scroller.clientHeight / total, 0, 1);
    const thumbTop = clamp(scroller.scrollTop / total, 0, 1);
    const insideThumb = fraction >= thumbTop && fraction <= thumbTop + thumb;
    dragOffsetRef.current = insideThumb ? fraction - thumbTop : thumb / 2;
    queueScroll(event.clientY);
  };

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.buttons !== 1) return;
    queueScroll(event.clientY);
  };

  if (map.totalRows === 0) return null;

  return (
    <div
      aria-hidden="true"
      className="relative w-[18px] shrink-0 touch-none border-l border-border bg-[color-mix(in_srgb,var(--gc-panel)_55%,var(--gc-background))]"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      ref={trackRef}
    >
      {boxes?.map === map && map.marks.map((mark, index) => {
        const box = boxes.boxes?.[index] ?? {
          top: mark.start / map.totalRows,
          height: mark.length / map.totalRows,
        };
        return (
          <div
            className="absolute inset-x-px rounded-[1px]"
            key={`${mark.kind}:${mark.start}`}
            style={{
              background: MARK_BACKGROUND[mark.kind],
              top: `${box.top * 100}%`,
              height: `max(3px, ${box.height * 100}%)`,
            }}
          />
        );
      })}
      <div
        className="pointer-events-none absolute inset-x-0 border-y border-[color-mix(in_srgb,var(--gc-muted)_45%,transparent)] bg-[color-mix(in_srgb,var(--gc-muted)_16%,transparent)]"
        ref={thumbRef}
        style={{ top: 0, height: "6px" }}
      />
    </div>
  );
}
