import { shortcutParts, type Platform } from "../lib/shortcut";

export function Keys({ shortcut, platform, large }: { shortcut: string; platform: Platform; large?: boolean }) {
  const parts = shortcutParts(shortcut, platform);
  return (
    <span className={`keys${large ? " large" : ""}`} aria-label={parts.join(" ")}>
      {parts.map((p, i) => (
        <kbd className="key" key={i}>
          {p}
        </kbd>
      ))}
    </span>
  );
}
