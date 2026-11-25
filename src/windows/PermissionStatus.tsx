import { useEffect, useState } from "react";
import { api, type AppInfo, type Permission } from "../lib/api";

/**
 * Screen Recording permission status with the actions to fix it. macOS often
 * only reports a new grant after the app restarts, hence the Restart button.
 */
export function PermissionStatus({ info, onInfo }: { info: AppInfo; onInfo: (info: AppInfo) => void }) {
  const [permission, setPermission] = useState<Permission>(info.permission);
  const [asked, setAsked] = useState(false);

  useEffect(() => setPermission(info.permission), [info.permission]);

  // Keep the status fresh while it's not granted.
  useEffect(() => {
    if (permission === "granted" || permission === "notRequired") return;
    const id = window.setInterval(() => {
      api
        .permissionStatus()
        .then((p) => {
          setPermission(p);
          if (p === "granted") api.appInfo().then(onInfo).catch(() => {});
        })
        .catch(() => {});
    }, 1500);
    return () => window.clearInterval(id);
  }, [permission, onInfo]);

  if (!info.recordingSupported) {
    return (
      <div className="row">
        <div className="row-main">
          <p className="row-title">Screen recording</p>
          <p className="row-hint">Recording isn't available on this platform yet. Rec ships on macOS first.</p>
        </div>
        <span className="status neutral">Coming soon</span>
      </div>
    );
  }

  if (permission === "granted" || permission === "notRequired") {
    return (
      <div className="row">
        <div className="row-main">
          <p className="row-title">Screen Recording</p>
          <p className="row-hint">Rec can record your screen.</p>
        </div>
        <span className="status ok">Granted</span>
      </div>
    );
  }

  return (
    <div className="row">
      <div className="row-main">
        <p className="row-title">Screen Recording</p>
        <p className="row-hint">
          {asked
            ? "Turn on Rec in System Settings → Privacy & Security → Screen & System Audio Recording, then restart Rec."
            : "macOS needs your permission before Rec can record."}
        </p>
      </div>
      <div className="row-actions">
        {asked ? (
          <>
            <button type="button" className="btn ghost" onClick={() => api.openPermissionSettings()}>
              Open System Settings
            </button>
            <button type="button" className="btn primary" onClick={() => api.relaunch()}>
              Restart Rec
            </button>
          </>
        ) : (
          <button
            type="button"
            className="btn primary"
            onClick={() => {
              setAsked(true);
              api.requestScreenPermission().then(setPermission).catch(() => {});
            }}
          >
            Grant access
          </button>
        )}
      </div>
    </div>
  );
}
