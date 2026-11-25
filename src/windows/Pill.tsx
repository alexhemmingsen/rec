import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api } from "../lib/api";
import { formatDuration } from "../lib/format";

type State = "recording" | "saving" | "done" | "failed";

/** The floating "■ Stop 0:12" pill. The only way to stop a recording. */
export function Pill() {
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [now, setNow] = useState(Date.now());
  const [state, setState] = useState<State>("recording");

  useEffect(() => {
    api.recordingStartedAt().then((t) => t && setStartedAt(t)).catch(() => {});
    const unlisten = getCurrentWebviewWindow().listen<{ startedAtMs: number }>("pill-started", (e) => {
      setStartedAt(e.payload.startedAtMs);
      setState("recording");
      setNow(Date.now());
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (state !== "recording") return;
    const id = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(id);
  }, [state]);

  const stop = () => {
    if (state !== "recording") return;
    setState("saving");
    api
      .stopRecording()
      .then(() => {
        setState("done");
        window.setTimeout(() => api.pillDone().catch(() => {}), 1600);
      })
      .catch(() => {
        setState("failed");
        window.setTimeout(() => api.pillDone().catch(() => {}), 2500);
      });
  };

  const elapsed = startedAt ? (now - startedAt) / 1000 : 0;

  if (state === "done") {
    return (
      <div className="pill done" role="status">
        <span aria-hidden>✓</span>
        <span className="pill-label">Path copied</span>
      </div>
    );
  }
  if (state === "failed") {
    return (
      <div className="pill failed" role="alert">
        <span className="pill-label">Couldn't save</span>
      </div>
    );
  }
  return (
    <button type="button" className={`pill ${state}`} onClick={stop} title="Stop recording">
      <span className="pill-stop" aria-hidden />
      <span className="pill-label">{state === "saving" ? "Saving…" : "Stop"}</span>
      <span className="pill-time">{formatDuration(elapsed)}</span>
    </button>
  );
}
