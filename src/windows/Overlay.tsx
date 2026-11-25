import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/api";
import { clampPoint, isClick, rectFromPoints, sizeLabel, type Point } from "../lib/selection";

/**
 * Covers one display. Drag a box: recording starts the moment you let go.
 * Esc, or a click without dragging, cancels.
 *
 * Pointer capture keeps every move and the release on this overlay, even if
 * the window manager or WebKit would otherwise route them elsewhere mid-drag.
 */
export function Overlay({ displayId }: { displayId: number }) {
  const [start, setStart] = useState<Point | null>(null);
  const [current, setCurrent] = useState<Point | null>(null);
  // Refs mirror the state so the window-level fallback sees current values.
  const startRef = useRef<Point | null>(null);
  const busy = useRef(false);
  const root = useRef<HTMLDivElement>(null);

  const reset = useCallback(() => {
    startRef.current = null;
    setStart(null);
    setCurrent(null);
    busy.current = false;
  }, []);

  useEffect(() => {
    api.uiLog(`overlay ready ${window.innerWidth}x${window.innerHeight} dpr=${window.devicePixelRatio}`);
    const unlisten = listen("overlay-reset", () => {
      reset();
      root.current?.focus();
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [reset]);

  const point = (x: number, y: number): Point => clampPoint({ x, y }, window.innerWidth, window.innerHeight);

  const finish = useCallback(
    (x: number, y: number, how: string) => {
      const from = startRef.current;
      api.uiLog(`${how} at ${x},${y} start=${from ? `${from.x},${from.y}` : "none"} busy=${busy.current}`);
      if (!from || busy.current) return;
      busy.current = true;
      const rect = rectFromPoints(from, point(x, y));
      if (isClick(rect)) {
        reset();
        api.cancelSelection().catch(() => {});
        return;
      }
      api
        .startRecording(displayId, rect)
        .catch(() => {})
        .finally(reset);
    },
    [displayId, reset],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        reset();
        api.cancelSelection().catch(() => {});
      }
    };
    // Fallback: a release that escapes the overlay still ends the drag.
    const onUp = (e: PointerEvent) => finish(e.clientX, e.clientY, "window pointerup");
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerup", onUp);
    };
  }, [reset, finish]);

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    api.uiLog(`pointerdown button=${e.button} at ${e.clientX},${e.clientY} busy=${busy.current}`);
    if (e.button !== 0 || busy.current) return;
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      /* capture is best-effort */
    }
    const p = point(e.clientX, e.clientY);
    startRef.current = p;
    setStart(p);
    setCurrent(p);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (startRef.current) setCurrent(point(e.clientX, e.clientY));
  };

  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    e.stopPropagation();
    finish(e.clientX, e.clientY, "pointerup");
  };

  const rect = start && current ? rectFromPoints(start, current) : null;
  const scale = window.devicePixelRatio || 1;

  return (
    <div
      ref={root}
      tabIndex={-1}
      className="overlay"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
    >
      {/* Always painted, so the window never has see-through pixels that
          would let clicks fall through to the app behind. */}
      <div className="overlay-dim" style={{ opacity: rect && !isClick(rect) ? 0 : 1 }} />
      {rect && !isClick(rect) ? (
        <>
          <div className="selection" style={{ left: rect.x, top: rect.y, width: rect.width, height: rect.height }} />
          <div
            className="size-label"
            style={{
              left: rect.x + rect.width + 8 + 90 > window.innerWidth ? rect.x : rect.x + rect.width + 8,
              top: rect.y + rect.height + 26 > window.innerHeight ? rect.y - 24 : rect.y + rect.height + 6,
            }}
          >
            {sizeLabel(rect, scale)}
          </div>
        </>
      ) : (
        <div className="overlay-hint">
          Drag to record &nbsp;·&nbsp; <kbd className="key">esc</kbd> to cancel
        </div>
      )}
    </div>
  );
}
