import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api } from "../lib/api";

const COLOR = "#ff3b30";
const WIDTH = 4;
/** How long a finished line stays sharp, then how long it takes to fade. */
const HOLD_MS = 3000;
const FADE_MS = 600;

type Point = { x: number; y: number };
type Stroke = { points: Point[]; endedAt: number | null };

/**
 * The pen: a see-through window exactly over the recorded region. While the
 * pen is on (the pen button in the Stop pill), drag to draw; each line stays
 * for a few seconds after you finish it, then fades out. This window is in
 * the video, so the drawing is too.
 */
export function Canvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const strokes = useRef<Stroke[]>([]);
  const current = useRef<Stroke | null>(null);
  const frame = useRef<number | null>(null);
  const [pen, setPen] = useState(false);

  /** Draw every line; true while any are still showing. */
  const draw = useCallback((): boolean => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return false;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(window.innerWidth * dpr);
    const h = Math.round(window.innerHeight * dpr);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);

    const now = performance.now();
    strokes.current = strokes.current.filter((s) => s.endedAt === null || now - s.endedAt < HOLD_MS + FADE_MS);
    ctx.strokeStyle = COLOR;
    ctx.fillStyle = COLOR;
    ctx.lineWidth = WIDTH;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    for (const s of strokes.current) {
      const age = s.endedAt === null ? 0 : now - s.endedAt;
      ctx.globalAlpha = age <= HOLD_MS ? 1 : Math.max(0, 1 - (age - HOLD_MS) / FADE_MS);
      const pts = s.points;
      if (pts.length === 1) {
        ctx.beginPath();
        ctx.arc(pts[0].x, pts[0].y, WIDTH / 2, 0, Math.PI * 2);
        ctx.fill();
        continue;
      }
      // Smooth the line through the midpoints between samples.
      ctx.beginPath();
      ctx.moveTo(pts[0].x, pts[0].y);
      for (let i = 1; i < pts.length - 1; i++) {
        const mx = (pts[i].x + pts[i + 1].x) / 2;
        const my = (pts[i].y + pts[i + 1].y) / 2;
        ctx.quadraticCurveTo(pts[i].x, pts[i].y, mx, my);
      }
      const last = pts[pts.length - 1];
      ctx.lineTo(last.x, last.y);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    return strokes.current.length > 0;
  }, []);

  /** Draw now, and keep animating while lines are fading. */
  const schedule = useCallback(() => {
    draw();
    if (frame.current !== null) return;
    const tick = () => {
      frame.current = null;
      if (draw()) frame.current = requestAnimationFrame(tick);
    };
    frame.current = requestAnimationFrame(tick);
  }, [draw]);

  const clear = useCallback(() => {
    strokes.current = [];
    current.current = null;
    setPen(false);
    draw();
  }, [draw]);

  useEffect(() => {
    api.uiLog(`drawing page ready ${window.innerWidth}x${window.innerHeight} dpr=${window.devicePixelRatio}`);
    const win = getCurrentWebviewWindow();
    const subs = [
      win.listen<boolean>("pen", (e) => {
        api.uiLog(`drawing page: pen ${e.payload ? "on" : "off"}`);
        setPen(e.payload);
        if (!e.payload && current.current) {
          current.current.endedAt = performance.now();
          current.current = null;
          schedule();
        }
      }),
      win.listen("canvas-reset", clear),
      win.listen("canvas-clear", clear),
    ];
    return () => {
      for (const s of subs) s.then((f) => f());
    };
  }, [clear, schedule]);

  const point = (e: React.PointerEvent): Point => ({ x: e.clientX, y: e.clientY });

  return (
    <div
      className={`canvas-root${pen ? " pen" : ""}`}
      onPointerDown={(e) => {
        api.uiLog(`drawing page: pointerdown at ${e.clientX},${e.clientY} pen=${pen}`);
        if (!pen || e.button !== 0) return;
        try {
          e.currentTarget.setPointerCapture(e.pointerId);
        } catch {
          /* best-effort */
        }
        const stroke = { points: [point(e)], endedAt: null };
        strokes.current.push(stroke);
        current.current = stroke;
        schedule();
      }}
      onPointerMove={(e) => {
        if (!current.current) return;
        current.current.points.push(point(e));
        schedule();
      }}
      onPointerUp={() => {
        if (!current.current) return;
        current.current.endedAt = performance.now();
        current.current = null;
        schedule();
      }}
      onPointerCancel={() => {
        if (!current.current) return;
        current.current.endedAt = performance.now();
        current.current = null;
        schedule();
      }}
    >
      <canvas ref={canvasRef} />
    </div>
  );
}
