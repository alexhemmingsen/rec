import { useState } from "react";
import { api, errorText, type AppInfo } from "../lib/api";
import { tildify } from "../lib/format";
import { ShortcutRecorder } from "../components/ShortcutRecorder";
import { Toggle } from "../components/Toggle";
import { PermissionRow } from "./PermissionStatus";
import { FfmpegRow, useFfmpeg } from "./FfmpegRow";
import keyboardIcon from "../assets/icons/keyboard.svg";
import stopIcon from "../assets/icons/stop.svg";
import downloadIcon from "../assets/icons/download.svg";
import openAtLoginIcon from "../assets/icons/open-at-login.svg";

/** Shortcuts, permissions, save folder, launching at startup and ffmpeg. */
export function General({
  info,
  onInfo,
  onToast,
}: {
  info: AppInfo;
  onInfo: (info: AppInfo) => void;
  onToast: (text: string) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const ffmpeg = useFfmpeg();

  const run = (p: Promise<AppInfo>) =>
    p
      .then((i) => {
        setError(null);
        onInfo(i);
      })
      .catch((e) => setError(errorText(e)));

  return (
    <div className="page">
      <h1 className="page-title">General</h1>
      {(error || ffmpeg.error) && <p className="error-text">{error ?? ffmpeg.error}</p>}
      <div className="group">
        <div className="group-row">
          <div className="row-label">
            <img src={keyboardIcon} className="icon" width={16} height={16} alt="" draggable={false} />
            <span>Keyboard shortcut</span>
          </div>
          <ShortcutRecorder
            shortcut={info.shortcut}
            platform={info.platform}
            onSaved={(i) => {
              onInfo(i);
              onToast("Shortcut saved");
            }}
          />
        </div>
        <div className="group-row">
          <div className="row-label">
            <img src={stopIcon} className="icon" width={16} height={16} alt="" draggable={false} />
            <span>Stop recording shortcut</span>
          </div>
          <ShortcutRecorder
            shortcut={info.stopShortcut}
            platform={info.platform}
            save={api.setStopShortcut}
            onSaved={(i) => {
              onInfo(i);
              onToast("Shortcut saved");
            }}
          />
        </div>
        <PermissionRow kind="screen" info={info} onInfo={onInfo} />
        {info.platform === "macos" && <PermissionRow kind="accessibility" info={info} onInfo={onInfo} />}
        <div className="group-row">
          <div className="row-label">
            <span className="icon-rotate">
              <img src={downloadIcon} className="icon" width={16} height={16} alt="" draggable={false} />
            </span>
            <span>Download to folder</span>
          </div>
          <div className="row-controls">
            <span className="row-value" title={info.saveDir}>
              {tildify(info.saveDir)}
            </span>
            <span className="vrule" />
            <button type="button" className="button" onClick={() => run(api.chooseSaveDir())}>
              Change
            </button>
          </div>
        </div>
        <div className="group-row">
          <div className="row-label">
            <img src={openAtLoginIcon} className="icon" width={16} height={16} alt="" draggable={false} />
            <span>Launch Rec when starting device</span>
          </div>
          <Toggle label="Launch Rec when starting device" checked={info.openAtLogin} onChange={(v) => run(api.setOpenAtLogin(v))} />
        </div>
        <FfmpegRow ffmpeg={ffmpeg} />
      </div>
    </div>
  );
}
