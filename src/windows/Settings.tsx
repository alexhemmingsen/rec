import { useState } from "react";
import { api, errorText, type AppInfo } from "../lib/api";
import { tildify } from "../lib/format";
import { ShortcutRecorder } from "../components/ShortcutRecorder";
import { Toggle } from "../components/Toggle";
import { PermissionStatus } from "./PermissionStatus";

/** Permissions, shortcut, save folder, open at login and the update check. */
export function Settings({
  info,
  onInfo,
  onToast,
}: {
  info: AppInfo;
  onInfo: (info: AppInfo) => void;
  onToast: (text: string) => void;
}) {
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = (p: Promise<AppInfo>) =>
    p
      .then((i) => {
        setError(null);
        onInfo(i);
      })
      .catch((e) => setError(errorText(e)));

  const checkNow = () => {
    setChecking(true);
    api
      .checkForUpdate()
      .then((u) => onToast(u ? `Rec ${u.version} is available` : "You're up to date"))
      .catch(() => onToast("Couldn't reach GitHub to check for updates"))
      .finally(() => setChecking(false));
  };

  return (
    <div className="page">
      <h1 className="page-title">Settings</h1>
      <p className="page-sub">Everything Rec does stays on your computer.</p>
      {error && <p className="error-text" style={{ marginBottom: 12 }}>{error}</p>}

      <div className="section-label">Permissions</div>
      <div className="card">
        <PermissionStatus info={info} onInfo={onInfo} />
      </div>

      <div className="section-label">Recording</div>
      <div className="card">
        <div className="row">
          <div className="row-main">
            <p className="row-title">Shortcut</p>
            <p className="row-hint">Click, then press a new combination with at least one modifier.</p>
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
        <div className="row">
          <div className="row-main">
            <p className="row-title">Save recordings to</p>
            <p className="path" title={info.saveDir}>
              {tildify(info.saveDir)}
            </p>
          </div>
          <div className="row-actions">
            {!info.saveDirIsDefault && (
              <button type="button" className="btn ghost" onClick={() => run(api.resetSaveDir())}>
                Use Downloads
              </button>
            )}
            <button type="button" className="btn" onClick={() => run(api.chooseSaveDir())}>
              Change…
            </button>
          </div>
        </div>
      </div>

      <div className="section-label">General</div>
      <div className="card">
        <div className="row">
          <div className="row-main">
            <p className="row-title">Open at login</p>
            <p className="row-hint">Starts Rec in the background so the shortcut works right after a restart.</p>
          </div>
          <Toggle label="Open at login" checked={info.openAtLogin} onChange={(v) => run(api.setOpenAtLogin(v))} />
        </div>
        <div className="row">
          <div className="row-main">
            <p className="row-title">Check for updates</p>
            <p className="row-hint">
              Asks GitHub for a newer version when Rec starts. Sends no recordings and no personal data.
            </p>
          </div>
          <div className="row-actions">
            <button type="button" className="btn ghost" disabled={checking} onClick={checkNow}>
              {checking ? "Checking…" : "Check now"}
            </button>
            <Toggle
              label="Check for updates"
              checked={info.checkForUpdates}
              onChange={(v) => run(api.setCheckForUpdates(v))}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
