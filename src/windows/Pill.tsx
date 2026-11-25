import { useEffect, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api, type AppInfo } from "../lib/api";
import { Keys } from "../components/Keys";
import { formatDuration } from "../lib/format";
import checkIcon from "../assets/icons/check.svg";
import penIcon from "../assets/icons/pen.svg";

type State = "recording" | "saving" | "done" | "failed";

/** The floating "■ Stop and save ⌘↩ 0:12" pill: click it, or press the keys it shows. */
export function Pill() {
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [now, setNow] = useState(Date.now());
  const [state, setState] = useState<State>("recording");
  // For the stop shortcut's keys.
  const [info, setInfo] = useState<AppInfo | null>(null);
  // Drawing on the recording (the pen button).
  const [pen, setPen] = useState(false);

  useEffect(() => {
    api.recordingStartedAt().then((t) => t && setStartedAt(t)).catch(() => {});
    api.appInfo().then(setInfo).catch(() => {});
    const unlisten = getCurrentWebviewWindow().listen<{ startedAtMs: number }>("pill-started", (e) => {
      setStartedAt(e.payload.startedAtMs);
      setState("recording");
      setPen(false);
      setNow(Date.now());
      api.appInfo().then(setInfo).catch(() => {});
    });
    const unlistenPen = getCurrentWebviewWindow().listen<boolean>("pen", (e) => setPen(e.payload));
    return () => {
      unlisten.then((f) => f());
      unlistenPen.then((f) => f());
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

  // The stop shortcut stops through here too, so it looks the same as a click.
  const stopRef = useRef(stop);
  stopRef.current = stop;
  useEffect(() => {
    const unlisten = getCurrentWebviewWindow().listen("pill-stop", () => stopRef.current());
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const elapsed = startedAt ? (now - startedAt) / 1000 : 0;

  // The window is larger than the pill so its shadow fits; the pill sits in
  // the middle.
  if (state === "done") {
    return (
      <div className="pill-frame">
        <div className="pill done" role="status">
          <img src={checkIcon} width={16} height={16} alt="" draggable={false} />
          <span>Path copied</span>
        </div>
      </div>
    );
  }
  if (state === "failed") {
    return (
      <div className="pill-frame">
        <div className="pill failed" role="alert">
          <span>Couldn't save</span>
        </div>
      </div>
    );
  }
  return (
    <div className="pill-frame">
      <div className="pill-row">
        {state === "recording" && (
          <button
            type="button"
            className={`pen-button${pen ? " on" : ""}`}
            aria-pressed={pen}
            title={pen ? "Stop drawing" : "Draw on the recording"}
            onClick={() => {
              const next = !pen;
              setPen(next);
              api.setPen(next).catch(() => {});
            }}
          >
            <img src={penIcon} width={16} height={16} alt="" draggable={false} />
            {info?.penShortcut && <Keys shortcut={info.penShortcut} platform={info.platform} />}
          </button>
        )}
        <button
          type="button"
          className={`pill ${state}`}
          onPointerDown={(e) => api.uiLog(`pill pointerdown at ${e.clientX},${e.clientY} state=${state}`)}
          onPointerUp={(e) => api.uiLog(`pill pointerup at ${e.clientX},${e.clientY}`)}
          onClick={() => {
            api.uiLog(`pill click, state=${state}`);
            stop();
          }}
          title="Stop recording"
        >
          <span className="pill-stop" aria-hidden />
          <span>{state === "saving" ? "Saving…" : "Stop and save"}</span>
          {state === "recording" && info && <Keys shortcut={info.stopShortcut} platform={info.platform} />}
          <span className="pill-time">{formatDuration(elapsed)}</span>
        </button>
      </div>
    </div>
  );
}
