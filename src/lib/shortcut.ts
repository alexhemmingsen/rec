// Global shortcuts use Tauri's accelerator syntax: "Control+Shift+5".

export type Platform = "macos" | "windows" | "linux";

const MODIFIER_ORDER = ["Control", "Alt", "Shift", "Super"] as const;
type Modifier = (typeof MODIFIER_ORDER)[number];

const MAC_SYMBOLS: Record<Modifier, string> = { Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘" };
const PC_NAMES: Record<Modifier, string> = { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Win" };

const MODIFIER_CODES = new Set([
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

const KEY_LABELS: Record<string, string> = {
  Space: "Space",
  Enter: "↩",
  Tab: "⇥",
  Backspace: "⌫",
  Delete: "⌦",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};

/** The key part of an accelerator for a KeyboardEvent.code. */
export function keyFromCode(code: string): string | null {
  if (MODIFIER_CODES.has(code)) return null;
  const digit = /^Digit(\d)$/.exec(code);
  if (digit) return digit[1];
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (code === "Escape") return null;
  return code || null;
}

export interface KeyLike {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

export type RecordResult =
  | { kind: "incomplete" } // only modifiers held so far
  | { kind: "invalid"; reason: string }
  | { kind: "ok"; accelerator: string };

/** Turn a keydown into an accelerator, or explain why it can't be one. */
export function acceleratorFromEvent(e: KeyLike): RecordResult {
  const mods: Modifier[] = [];
  if (e.ctrlKey) mods.push("Control");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  const key = keyFromCode(e.code);
  if (key === null) {
    if (e.code === "Escape") return { kind: "invalid", reason: "Esc cancels a selection, so it can't be the shortcut." };
    return { kind: "incomplete" };
  }
  if (mods.length === 0) return { kind: "invalid", reason: "Hold at least one modifier key." };
  if (mods.length === 1 && mods[0] === "Shift") {
    return { kind: "invalid", reason: "Shift alone would block typing. Add another modifier." };
  }
  return { kind: "ok", accelerator: [...mods, key].join("+") };
}

/** The parts of an accelerator, in display order: modifiers first. */
export function shortcutParts(accelerator: string, platform: Platform): string[] {
  const tokens = accelerator.split("+").filter(Boolean);
  const mods: Modifier[] = [];
  let key = "";
  for (const raw of tokens) {
    const t = normalizeModifier(raw);
    if (t) mods.push(t);
    else key = raw;
  }
  mods.sort((a, b) => MODIFIER_ORDER.indexOf(a) - MODIFIER_ORDER.indexOf(b));
  const names = platform === "macos" ? MAC_SYMBOLS : PC_NAMES;
  const keyLabel = KEY_LABELS[key] ?? key.replace(/^Digit/, "").replace(/^Key/, "");
  return [...mods.map((m) => names[m]), keyLabel];
}

/** Human-readable shortcut: "⌃⇧5" on macOS, "Ctrl+Shift+5" elsewhere. */
export function formatShortcut(accelerator: string, platform: Platform): string {
  const parts = shortcutParts(accelerator, platform);
  return platform === "macos" ? parts.join("") : parts.join("+");
}

function normalizeModifier(token: string): Modifier | null {
  switch (token.toLowerCase()) {
    case "control":
    case "ctrl":
      return "Control";
    case "alt":
    case "option":
      return "Alt";
    case "shift":
      return "Shift";
    case "super":
    case "cmd":
    case "command":
    case "meta":
      return "Super";
    default:
      return null;
  }
}
