import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText, type AppInfo, type FfmpegStatus, type HistoryItem } from "../lib/api";
import { ShortcutRecorder } from "../components/ShortcutRecorder";
import { Keys } from "../components/Keys";
import { PermissionStatus } from "./PermissionStatus";
import iconUrl from "../../assets/icon.png";

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
        <button type="button" className="btn primary large" onClick={onNext}>
          Get started
        </button>
      </div>
    </div>
  );
}

function PermissionStep({ info, onInfo, onNext }: { info: AppInfo; onInfo: (i: AppInfo) => void; onNext: () => void }) {
  const ready = info.permission !== "notGranted" || !info.recordingSupported;
  return (
    <div className="ob-card">
      <h1>Allow screen recording</h1>
      <p className="lead">
        {info.platform === "macos"
          ? "macOS asks before any app can record the screen. Rec only records the area you select, and only while the Stop button is showing."
          : "Your system doesn't need extra permission for Rec."}
      </p>
      <div className="card ob-panel">
        <PermissionStatus info={info} onInfo={onInfo} />
      </div>
      <div className="ob-actions">
        <button type="button" className="btn primary large" disabled={!ready} onClick={onNext}>
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
            <strong>Open at login</strong>
            <span className="row-hint" style={{ display: "block" }}>
              Rec starts in the background, so the shortcut always works.
            </span>
          </span>
        </label>
        {error && <p className="error-text">{error}</p>}
      </div>
      <div className="ob-actions">
        <button type="button" className="btn primary large" onClick={finish}>
          Continue
        </button>
      </div>
    </div>
  );
}

function FfmpegStep({ onNext }: { onNext: () => void }) {
  const [status, setStatus] = useState<FfmpegStatus | null>(null);
  const [copied, setCopied] = useState(false);
  const check = () => {
    setStatus(null);
    api.checkFfmpeg().then(setStatus).catch(() => setStatus({ found: false, path: null, installCommand: "brew install ffmpeg" }));
  };
  useEffect(check, []);

  return (
    <div className="ob-card">
      <h1>Help your agent read video</h1>
      <p className="lead">
        Agents read recordings with a video tool. We suggest ffmpeg, but any tool your agent likes works.
      </p>
      <div className="card ob-panel" style={{ padding: 16 }}>
        {status === null ? (
          <span className="waiting">Checking for ffmpeg…</span>
        ) : status.found ? (
          <div>
            <span className="status ok">ffmpeg is installed</span>
            <p className="path" style={{ marginTop: 6 }}>
              {status.path}
            </p>
          </div>
        ) : (
          <div>
            <p style={{ marginBottom: 10 }}>ffmpeg isn't installed. Paste this into a terminal:</p>
            <div className="command">
              <code>{status.installCommand}</code>
              <button
                type="button"
                className="btn"
                onClick={() =>
                  api.copyText(status.installCommand).then(() => {
                    setCopied(true);
                    window.setTimeout(() => setCopied(false), 1500);
                  })
                }
              >
                {copied ? "Copied" : "Copy"}
              </button>
            </div>
            <p className="row-hint" style={{ marginTop: 8 }}>
              …or use any tool you like.
            </p>
          </div>
        )}
      </div>
      <div className="ob-actions">
        {status && !status.found ? (
          <>
            <button type="button" className="btn primary large" onClick={check}>
              Check again
            </button>
            <button type="button" className="btn ghost" onClick={onNext}>
              Skip
            </button>
          </>
        ) : (
          <button type="button" className="btn primary large" disabled={status === null} onClick={onNext}>
            Continue
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
      <div className="ob-card">
        <div className="success-mark" aria-hidden>
          ✓
        </div>
        <h1>Copied! Paste it anywhere.</h1>
        <p className="lead">The path to your recording is on the clipboard:</p>
        <div className="command ob-panel">
          <code>{result.path}</code>
        </div>
        <div className="ob-actions">
          <button type="button" className="btn primary large" onClick={onDone}>
            Done
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="ob-card">
      <h1>Try it now</h1>
      <p className="lead">Press your shortcut, drag a box over anything, then click Stop.</p>
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
