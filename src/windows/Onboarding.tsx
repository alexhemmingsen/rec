import { useEffect, useRef, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText, type AppInfo, type HistoryItem } from "../lib/api";
import { ShortcutRecorder } from "../components/ShortcutRecorder";
import { Keys } from "../components/Keys";
import { PermissionRow } from "./PermissionStatus";
import { FfmpegRow, useFfmpeg } from "./FfmpegRow";
import iconUrl from "../../assets/icon.png";
import checkIcon from "../assets/icons/check.svg";

const STEP_KEY = "rec.onboarding.step";
const STEPS = 5;

function savedStep(): number {
  try {
    const n = Number(localStorage.getItem(STEP_KEY));
    return Number.isInteger(n) && n >= 0 && n < STEPS ? n : 0;
  } catch {
    return 0;
  }
}

/**
 * First run, no account: welcome → permission → shortcut + open at login →
 * optional ffmpeg check → a practice recording.
 */
export function Onboarding({ info, onInfo }: { info: AppInfo; onInfo: (info: AppInfo) => void }) {
  // The step survives the restart macOS may need after granting permission.
  const [step, setStepState] = useState(savedStep);
  const setStep = (n: number) => {
    setStepState(n);
    try {
      localStorage.setItem(STEP_KEY, String(n));
    } catch {
      /* storage unavailable: fine, we just start over next time */
    }
  };
  const next = () => setStep(Math.min(step + 1, STEPS - 1));

  return (
    <div className="onboarding">
      <div className="onboarding-top" data-tauri-drag-region />
      <div className="onboarding-body">
        {step === 0 && <Welcome onNext={next} />}
        {step === 1 && <PermissionStep info={info} onInfo={onInfo} onNext={next} />}
        {step === 2 && <ShortcutStep info={info} onInfo={onInfo} onNext={next} />}
        {step === 3 && <FfmpegStep onNext={next} />}
        {step === 4 && (
          <TryItStep
            info={info}
            onDone={() =>
              api.completeOnboarding().then((i) => {
                try {
                  localStorage.removeItem(STEP_KEY);
                } catch {
                  /* ignore */
                }
                onInfo(i);
              })
            }
          />
        )}
      </div>
      <div className="ob-foot" aria-label={`Step ${step + 1} of ${STEPS}`}>
        {Array.from({ length: STEPS }, (_, i) => (
          <span key={i} className={`ob-dot${i === step ? " on" : ""}`} />
        ))}
      </div>
    </div>
  );
}

function Welcome({ onNext }: { onNext: () => void }) {
  return (
    <div className="ob-card">
      <img src={iconUrl} className="ob-icon" alt="" draggable={false} />
      <h1>Welcome to Rec</h1>
      <p className="lead">
        Press a shortcut, drag a box, and the recording starts. Click Stop and the video's path is on your clipboard,
        ready to paste into Claude Code, Codex, Cursor or any other agent.
      </p>
      <div className="ob-actions">
        <button type="button" className="button primary" onClick={onNext}>
          Get started
        </button>
      </div>
    </div>
  );
}

function PermissionStep({ info, onInfo, onNext }: { info: AppInfo; onInfo: (i: AppInfo) => void; onNext: () => void }) {
  const ready =
    !info.recordingSupported || (info.permission !== "notGranted" && info.accessibility !== "notGranted");
  // Once Screen Recording is on: a throwaway capture, so macOS asks its
  // "bypass the private window picker" question here, not over your first
  // recording.
  const warmedUp = useRef(false);
  useEffect(() => {
    if (info.platform !== "macos" || info.permission !== "granted" || warmedUp.current) return;
    warmedUp.current = true;
    api.warmUpCapture().catch(() => {});
  }, [info.platform, info.permission]);
  return (
    <div className="ob-card wide">
      <h1>Allow access</h1>
      <p className="lead">
        {info.platform === "macos"
          ? "Screen recording lets Rec record the box you drag. Accessibility lets you keep using your Mac after the shortcut, until you start dragging."
          : "Your system doesn't need extra permission for Rec."}
      </p>
      <div className="ob-panel">
        <div className="group">
          <PermissionRow kind="screen" info={info} onInfo={onInfo} />
          {info.platform === "macos" && <PermissionRow kind="accessibility" info={info} onInfo={onInfo} />}
        </div>
      </div>
      <div className="ob-actions">
        <button type="button" className="button primary" disabled={!ready} onClick={onNext}>
          Continue
        </button>
        {!ready && (
          <button type="button" className="btn ghost" onClick={onNext}>
            Skip for now
          </button>
        )}
      </div>
    </div>
  );
}

function ShortcutStep({ info, onInfo, onNext }: { info: AppInfo; onInfo: (i: AppInfo) => void; onNext: () => void }) {
  const [openAtLogin, setOpenAtLogin] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const finish = () => {
    api
      .setOpenAtLogin(openAtLogin)
      .then((i) => {
        onInfo(i);
        onNext();
      })
      .catch((e) => setError(errorText(e)));
  };
  return (
    <div className="ob-card">
      <h1>Your shortcut</h1>
      <p className="lead">Press it from any app to start. Click the box to change it.</p>
      <div className="card ob-panel" style={{ padding: 18 }}>
        <div style={{ display: "flex", justifyContent: "center" }}>
          <ShortcutRecorder shortcut={info.shortcut} platform={info.platform} onSaved={onInfo} />
        </div>
        <label className="checkbox">
          <input type="checkbox" checked={openAtLogin} onChange={(e) => setOpenAtLogin(e.target.checked)} />
          <span>
            <strong>Launch Rec when starting device</strong>
            <span className="row-hint" style={{ display: "block" }}>
              Rec starts in the background, so the shortcut always works.
            </span>
          </span>
        </label>
        {error && <p className="error-text">{error}</p>}
      </div>
      <div className="ob-actions">
        <button type="button" className="button primary" onClick={finish}>
          Continue
        </button>
      </div>
    </div>
  );
}

function FfmpegStep({ onNext }: { onNext: () => void }) {
  const ffmpeg = useFfmpeg();
  return (
    <div className="ob-card wide">
      <h1>Help your agent read video</h1>
      <p className="lead">
        Agents read recordings with a video tool. We suggest ffmpeg, but any tool your agent likes works.
      </p>
      <div className="ob-panel">
        <div className="group">
          <FfmpegRow ffmpeg={ffmpeg} />
        </div>
        {ffmpeg.error && <p className="error-text">{ffmpeg.error}</p>}
      </div>
      <div className="ob-actions">
        <button type="button" className="button primary" disabled={!ffmpeg.found} onClick={onNext}>
          Continue
        </button>
        {!ffmpeg.found && (
          <button type="button" className="button ghost" onClick={onNext}>
            I use another tool
          </button>
        )}
      </div>
    </div>
  );
}

function TryItStep({ info, onDone }: { info: AppInfo; onDone: () => void }) {
  const [result, setResult] = useState<HistoryItem | null>(null);
  useEffect(() => {
    const unlisten = listen<HistoryItem>("recording-finished", (e) => setResult(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  if (result) {
    return (
      <div className="ob-card wide">
        <div className="success-mark" aria-hidden>
          <span className="success-check" style={{ "--icon": `url("${checkIcon}")` } as CSSProperties} />
        </div>
        <h1>Copied! Paste it anywhere.</h1>
        <p className="lead">The path to your recording is on the clipboard:</p>
        <div className="command ob-panel">
          <code>{result.path}</code>
        </div>
        <div className="ob-actions">
          <button type="button" className="button primary" onClick={onDone}>
            Done
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="ob-card">
      <h1>Try it now</h1>
      <p className="lead">
        Press your shortcut, drag a box over anything, then click Stop or press{" "}
        <Keys shortcut={info.stopShortcut} platform={info.platform} />.
      </p>
      <div style={{ marginBottom: 26 }}>
        <Keys shortcut={info.shortcut} platform={info.platform} large />
      </div>
      <div className="ob-actions">
        <span className="waiting">
          <span className="pulse" /> Waiting for your first recording…
        </span>
      </div>
      <div className="ob-actions" style={{ marginTop: 18 }}>
        <button type="button" className="btn ghost" onClick={onDone}>
          Skip
        </button>
      </div>
    </div>
  );
}
