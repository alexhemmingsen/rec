import { useCallback, useEffect, useState } from "react";
import { api, errorText, type FfmpegStatus } from "../lib/api";
import ffmpegIcon from "../assets/icons/ffmpeg.svg";

export interface Ffmpeg {
  status: FfmpegStatus | null;
  found: boolean;
  installing: boolean;
  error: string | null;
  install: () => void;
}

/** Whether ffmpeg is installed, kept fresh, and a way to install it. */
export function useFfmpeg(): Ffmpeg {
  const [status, setStatus] = useState<FfmpegStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const found = status?.found ?? false;

  const check = useCallback(() => {
    api
      .checkFfmpeg()
      .then(setStatus)
      .catch(() => setStatus((s) => s ?? { found: false, path: null, installCommand: "" }));
  }, []);
  useEffect(check, [check]);

  // Look again when Rec comes back to the front, and every few seconds while
  // the install runs in Terminal.
  useEffect(() => {
    if (found) return;
    window.addEventListener("focus", check);
    const id = installing ? window.setInterval(check, 3000) : undefined;
    return () => {
      window.removeEventListener("focus", check);
      window.clearInterval(id);
    };
  }, [found, installing, check]);

  const install = () => {
    setError(null);
    api
      .installFfmpeg()
      .then(() => setInstalling(true))
      .catch((e) => setError(errorText(e)));
  };

  return { status, found, installing, error, install };
}

/** The ffmpeg row: "Installed" (green) or "Not installed" (red) and Install. */
export function FfmpegRow({ ffmpeg }: { ffmpeg: Ffmpeg }) {
  const { status, found, installing, install } = ffmpeg;
  return (
    <div className="group-row">
      <div className="row-label">
        <img src={ffmpegIcon} className="icon" width={16} height={16} alt="" draggable={false} />
        <span>ffmpeg</span>
      </div>
      <div className="row-controls">
        {status === null ? (
          <span className="status">Checking…</span>
        ) : found ? (
          <span className="status granted">Installed</span>
        ) : (
          <>
            <span className={installing ? "status" : "status denied"}>
              {installing ? "Installing in Terminal…" : "Not installed"}
            </span>
            <span className="vrule" />
            <button type="button" className="button primary" onClick={install}>
              Install
            </button>
          </>
        )}
      </div>
    </div>
  );
}
