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
  save = api.setShortcut,
}: {
  shortcut: string;
  platform: Platform;
  onSaved: (info: AppInfo) => void;
  /** Stores the new keys; the start shortcut by default. */
  save?: (accelerator: string) => Promise<AppInfo>;
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
      save(result.accelerator)
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
  }, [listening, onSaved, save]);

  const field = (
    <button
      type="button"
      className={`shortcut-field${listening ? " listening" : ""}`}
      onClick={() => {
        setError(null);
        setListening((l) => !l);
      }}
    >
      {listening ? <span>Press new shortcut</span> : <Keys shortcut={shortcut} platform={platform} />}
    </button>
  );

  return (
    <div>
      {listening ? (
        <div className="shortcut-editing">
          {field}
          <span className="vrule" />
          <span className="esc-hint">
            <kbd className="key esc">esc</kbd>
            to cancel
          </span>
        </div>
      ) : (
        field
      )}
      {error && <p className="error-text">{error}</p>}
    </div>
  );
}
