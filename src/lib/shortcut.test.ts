import { describe, expect, it } from "vitest";
import { acceleratorFromEvent, formatShortcut, keyFromCode, shortcutParts } from "./shortcut";

const key = (code: string, mods: Partial<{ ctrl: boolean; alt: boolean; shift: boolean; meta: boolean }> = {}) => ({
  code,
  ctrlKey: !!mods.ctrl,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
  metaKey: !!mods.meta,
});

describe("formatShortcut", () => {
  it("shows the default as ⌃⇧5 on macOS and Ctrl+Shift+5 elsewhere", () => {
    expect(formatShortcut("Control+Shift+5", "macos")).toBe("⌃⇧5");
    expect(formatShortcut("Control+Shift+5", "windows")).toBe("Ctrl+Shift+5");
    expect(formatShortcut("Control+Shift+5", "linux")).toBe("Ctrl+Shift+5");
  });

  it("orders modifiers the macOS way regardless of input order", () => {
    expect(formatShortcut("Super+Shift+Alt+Control+R", "macos")).toBe("⌃⌥⇧⌘R");
  });

  it("accepts aliases and code-style keys", () => {
    expect(shortcutParts("Cmd+Option+Digit2", "macos")).toEqual(["⌥", "⌘", "2"]);
    expect(shortcutParts("Ctrl+Shift+Space", "windows")).toEqual(["Ctrl", "Shift", "Space"]);
  });
});

describe("acceleratorFromEvent", () => {
  it("builds accelerators in a stable order", () => {
    expect(acceleratorFromEvent(key("Digit5", { ctrl: true, shift: true }))).toEqual({
      kind: "ok",
      accelerator: "Control+Shift+5",
    });
    expect(acceleratorFromEvent(key("KeyR", { meta: true, alt: true }))).toEqual({
      kind: "ok",
      accelerator: "Alt+Super+R",
    });
    expect(acceleratorFromEvent(key("F5", { ctrl: true }))).toEqual({ kind: "ok", accelerator: "Control+F5" });
  });

  it("waits while only modifiers are held", () => {
    expect(acceleratorFromEvent(key("ShiftLeft", { shift: true }))).toEqual({ kind: "incomplete" });
    expect(acceleratorFromEvent(key("MetaRight", { meta: true }))).toEqual({ kind: "incomplete" });
  });

  it("rejects shortcuts that would get in the way", () => {
    expect(acceleratorFromEvent(key("KeyA")).kind).toBe("invalid");
    expect(acceleratorFromEvent(key("KeyA", { shift: true })).kind).toBe("invalid");
    expect(acceleratorFromEvent(key("Escape", { ctrl: true })).kind).toBe("invalid");
  });
});

describe("keyFromCode", () => {
  it("maps digits, letters and function keys", () => {
    expect(keyFromCode("Digit0")).toBe("0");
    expect(keyFromCode("KeyZ")).toBe("Z");
    expect(keyFromCode("F12")).toBe("F12");
    expect(keyFromCode("ControlLeft")).toBeNull();
  });
});
