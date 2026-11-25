import { useEffect, useState } from "react";
import { api, errorText, type AppInfo } from "../lib/api";
import { acceleratorFromEvent, type Platform } from "../lib/shortcut";
import { Keys } from "./Keys";

/**
 * Click, then press a new combination. The current global shortcut is paused
 * while listening, so pressing it here doesn't start a selection.
 */
export function ShortcutRecorder({
  shortcut,
  platform,
  onSaved,
}: {
  shortcut: string;
  platform: Platform;
  onSaved: (info: AppInfo) => void;
}) {
  const [listening, setListening] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!listening) return;
    api.setShortcutSuspended(true).catch(() => {});
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape" && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
        setListening(false);
        return;
      }
      const result = acceleratorFromEvent(e);
      if (result.kind === "incomplete") return;
      if (result.kind === "invalid") {
        setError(result.reason);
        return;
      }
      setListening(false);
      api
        .setShortcut(result.accelerator)
        .then((info) => {
          setError(null);
          onSaved(info);
        })
        .catch((err) => setError(errorText(err)));
    };
    const stop = () => setListening(false);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", stop);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", stop);
      api.setShortcutSuspended(false).catch(() => {});
    };
  }, [listening, onSaved]);

  return (
    <div>
      <button
        type="button"
        className={`shortcut-field${listening ? " listening" : ""}`}
        onClick={() => {
          setError(null);
          setListening((l) => !l);
        }}
      >
        {listening ? <span>Press a new shortcut…</span> : <Keys shortcut={shortcut} platform={platform} />}
      </button>
      {error && <p className="error-text">{error}</p>}
    </div>
  );
}
