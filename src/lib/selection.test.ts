import { describe, expect, it } from "vitest";
import { clampPoint, displayIdFromLabel, isClick, rectFromPoints, sizeLabel } from "./selection";

describe("rectFromPoints", () => {
  it("normalises drags in any direction", () => {
    const expected = { x: 10, y: 20, width: 90, height: 60 };
    expect(rectFromPoints({ x: 10, y: 20 }, { x: 100, y: 80 })).toEqual(expected);
    expect(rectFromPoints({ x: 100, y: 80 }, { x: 10, y: 20 })).toEqual(expected);
    expect(rectFromPoints({ x: 100, y: 20 }, { x: 10, y: 80 })).toEqual(expected);
  });
});

describe("isClick", () => {
  it("treats tiny drags as a click (which cancels)", () => {
    expect(isClick({ x: 0, y: 0, width: 0, height: 0 })).toBe(true);
    expect(isClick({ x: 0, y: 0, width: 300, height: 3 })).toBe(true);
    expect(isClick({ x: 0, y: 0, width: 4, height: 4 })).toBe(false);
  });
});

describe("clampPoint", () => {
  it("keeps the drag on the display", () => {
    expect(clampPoint({ x: -5, y: 2000 }, 1440, 900)).toEqual({ x: 0, y: 900 });
  });
});

describe("sizeLabel", () => {
  it("shows real pixels on Retina", () => {
    expect(sizeLabel({ x: 0, y: 0, width: 400, height: 300 }, 2)).toBe("800 × 600");
    expect(sizeLabel({ x: 0, y: 0, width: 400, height: 300 }, 0)).toBe("400 × 300");
  });
});

describe("displayIdFromLabel", () => {
  it("reads the display id from the window label", () => {
    expect(displayIdFromLabel("overlay-3")).toBe(3);
    expect(displayIdFromLabel("overlay-4127")).toBe(4127);
    expect(displayIdFromLabel("main")).toBeNull();
  });
});
