import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/api";
import { clampPoint, isClick, rectFromPoints, sizeLabel, type Point } from "../lib/selection";

/**
 * Covers one display. Drag a box: recording starts the moment you let go.
 * Esc, or a click without dragging, cancels.
 */
export function Overlay({ displayId }: { displayId: number }) {
  const [start, setStart] = useState<Point | null>(null);
  const [current, setCurrent] = useState<Point | null>(null);
  const busy = useRef(false);

  const reset = useCallback(() => {
    setStart(null);
    setCurrent(null);
    busy.current = false;
  }, []);

  useEffect(() => {
    const unlisten = listen("overlay-reset", reset);
    return () => {
      unlisten.then((f) => f());
    };
  }, [reset]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        reset();
        api.cancelSelection().catch(() => {});
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [reset]);

  const point = (e: React.MouseEvent): Point => clampPoint({ x: e.clientX, y: e.clientY }, window.innerWidth, window.innerHeight);

  const onMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0 || busy.current) return;
    const p = point(e);
    setStart(p);
    setCurrent(p);
  };

  const onMouseMove = (e: React.MouseEvent) => {
    if (start) setCurrent(point(e));
  };

  const onMouseUp = (e: React.MouseEvent) => {
    if (!start || busy.current) return;
    const rect = rectFromPoints(start, point(e));
    busy.current = true;
    if (isClick(rect)) {
      reset();
      api.cancelSelection().catch(() => {});
      return;
    }
    api
      .startRecording(displayId, rect)
      .catch(() => {})
      .finally(reset);
  };

  const rect = start && current ? rectFromPoints(start, current) : null;
  const scale = window.devicePixelRatio || 1;

  return (
    <div className="overlay" onMouseDown={onMouseDown} onMouseMove={onMouseMove} onMouseUp={onMouseUp}>
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
        <>
          <div className="overlay-dim" />
          <div className="overlay-hint">
            Drag to record &nbsp;·&nbsp; <kbd className="key">esc</kbd> to cancel
          </div>
        </>
      )}
    </div>
  );
}
