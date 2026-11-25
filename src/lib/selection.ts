// Geometry for the drag-to-select overlay. Coordinates are CSS pixels in the
// overlay window, which covers one display exactly (CSS px = points on macOS).

export interface Point {
  x: number;
  y: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A drag smaller than this (in either direction) counts as a click: cancel. */
export const MIN_DRAG = 4;

/** The rectangle spanned by two corners, whichever way the drag went. */
export function rectFromPoints(a: Point, b: Point): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
  };
}

/** Clamp a point into a `width` x `height` area. */
export function clampPoint(p: Point, width: number, height: number): Point {
  return { x: Math.min(Math.max(p.x, 0), width), y: Math.min(Math.max(p.y, 0), height) };
}

export function isClick(r: Rect): boolean {
  return r.width < MIN_DRAG || r.height < MIN_DRAG;
}

/** Label shown next to the selection: its size in real pixels. */
export function sizeLabel(r: Rect, scale: number): string {
  const s = scale > 0 && Number.isFinite(scale) ? scale : 1;
  return `${Math.round(r.width * s)} × ${Math.round(r.height * s)}`;
}

/** The display id encoded in an overlay window label ("overlay-3" → 3). */
export function displayIdFromLabel(label: string): number | null {
  const m = /^overlay-(\d+)$/.exec(label);
  return m ? Number(m[1]) : null;
}
