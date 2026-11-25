import { useEffect, useState } from "react";
import { api, type AppInfo, type Permission } from "../lib/api";
import screenRecordingIcon from "../assets/icons/screen-recording.svg";
import accessibilityIcon from "../assets/icons/accessibility.svg";

export type PermissionKind = "screen" | "accessibility";

const KINDS = {
  screen: {
    label: "Screen recording permissions",
    icon: screenRecordingIcon,
    current: (info: AppInfo) => info.permission,
    status: api.permissionStatus,
    request: api.requestScreenPermission,
    openSettings: api.openPermissionSettings,
    // macOS often only reports a new Screen Recording grant after a restart.
    restartAfterAsking: true,
  },
  accessibility: {
    label: "Accessibility permissions",
    icon: accessibilityIcon,
    current: (info: AppInfo) => info.accessibility,
    status: api.accessibilityStatus,
    request: api.requestAccessibility,
    openSettings: api.openAccessibilitySettings,
    restartAfterAsking: false,
  },
};

/**
 * A permission's status ("Granted" / "Not granted") and the action that fits
 * it. After asking for Screen Recording, "Restart Rec" appears too, since
 * macOS often only reports that grant after a restart.
 */
export function PermissionControl({
  kind,
  info,
  onInfo,
}: {
  kind: PermissionKind;
  info: AppInfo;
  onInfo: (info: AppInfo) => void;
}) {
  const k = KINDS[kind];
  const current = k.current(info);
  const [permission, setPermission] = useState<Permission>(current);
  const [asked, setAsked] = useState(false);

  useEffect(() => setPermission(current), [current]);

  // Keep the status fresh while it's not granted.
  useEffect(() => {
    if (permission !== "notGranted") return;
    const id = window.setInterval(() => {
      k.status()
        .then((p) => {
          setPermission(p);
          if (p === "granted") api.appInfo().then(onInfo).catch(() => {});
        })
        .catch(() => {});
    }, 1500);
    return () => window.clearInterval(id);
  }, [permission, onInfo, k]);

  if (permission !== "notGranted") {
    return (
      <div className="row-controls">
        <span className="status granted">{permission === "notRequired" ? "Not needed" : "Granted"}</span>
        {info.platform === "macos" && (
          <>
            <span className="vrule" />
            <button type="button" className="button" onClick={() => k.openSettings()}>
              Open system settings
            </button>
          </>
        )}
      </div>
    );
  }

  // One button: the first click asks macOS (its dialog), later clicks open
  // System Settings (macOS asks only once). Rec remembers which applies.
  const restart = asked && k.restartAfterAsking;
  return (
    <div className="row-controls">
      <span className="status denied">Not granted</span>
      <span className="vrule" />
      <button
        type="button"
        className={restart ? "button" : "button primary"}
        onClick={() => {
          setAsked(true);
          k.request().then(setPermission).catch(() => {});
        }}
      >
        Grant access
      </button>
      {restart && (
        <button type="button" className="button primary" onClick={() => api.relaunch()}>
          Restart Rec
        </button>
      )}
    </div>
  );
}

/** A permission as a settings row: icon, name, status and action. */
export function PermissionRow({
  kind,
  info,
  onInfo,
}: {
  kind: PermissionKind;
  info: AppInfo;
  onInfo: (info: AppInfo) => void;
}) {
  const k = KINDS[kind];
  return (
    <div className="group-row">
      <div className="row-label">
        <img src={k.icon} className="icon" width={16} height={16} alt="" draggable={false} />
        <span>{k.label}</span>
      </div>
      <PermissionControl kind={kind} info={info} onInfo={onInfo} />
    </div>
  );
}
