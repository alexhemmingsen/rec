import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText, type AppInfo, type HistoryItem, type RecordingError, type UpdateInfo } from "../lib/api";
import { Onboarding } from "./Onboarding";
import { Overview } from "./Overview";
import { Settings } from "./Settings";
import { History } from "./History";

type Page = "overview" | "settings" | "history";

export function Main() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [page, setPage] = useState<Page>("overview");
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [error, setError] = useState<RecordingError | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updating, setUpdating] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const refreshInfo = useCallback(() => api.appInfo().then(setInfo).catch(() => {}), []);
  const refreshHistory = useCallback(() => api.listHistory().then(setHistory).catch(() => {}), []);

  useEffect(() => {
    refreshInfo();
    refreshHistory();
    const subs = [
      listen<HistoryItem>("recording-finished", () => {
        setError(null);
        refreshHistory();
      }),
      listen<RecordingError>("recording-error", (e) => {
        setError(e.payload);
        refreshInfo();
      }),
      listen<UpdateInfo>("update-available", (e) => setUpdate(e.payload)),
    ];
    // Permission can change in System Settings while Rec is open.
    const onFocus = () => {
      refreshInfo();
      refreshHistory();
    };
    window.addEventListener("focus", onFocus);
    return () => {
      subs.forEach((s) => s.then((f) => f()));
      window.removeEventListener("focus", onFocus);
    };
  }, [refreshInfo, refreshHistory]);

  const showToast = useCallback((text: string) => {
    setToast(text);
    window.setTimeout(() => setToast(null), 1800);
  }, []);

  if (!info) return null;

  if (!info.onboardingComplete) {
    return <Onboarding info={info} onInfo={setInfo} />;
  }

  const installUpdate = () => {
    setUpdating(true);
    api.installUpdate().catch((e) => {
      setUpdating(false);
      showToast(errorText(e));
    });
  };

  return (
    <div className="shell">
      <nav className="sidebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <span className="brand-dot" />
          Rec
        </div>
        {(
          [
            ["overview", "Overview"],
            ["settings", "Settings"],
            ["history", "History"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            className="nav-item"
            aria-current={page === id ? "page" : undefined}
            onClick={() => setPage(id)}
          >
            {label}
            {id === "history" && history.length > 0 && <span className="nav-count">{history.length}</span>}
          </button>
        ))}
        <div className="sidebar-foot">Version {info.version}</div>
      </nav>
      <main className="content">
        <div className="drag-strip" data-tauri-drag-region />
        {(error || update) && (
          <div className="banner-wrap">
            {error && (
              <div className="banner" role="alert">
                <p>{error.message}</p>
                {error.permission && (
                  <button type="button" className="btn" onClick={() => setPage("settings")}>
                    Open Settings
                  </button>
                )}
                <button type="button" className="btn ghost" onClick={() => setError(null)}>
                  Dismiss
                </button>
              </div>
            )}
            {update && (
              <div className="banner update" role="status" style={{ marginTop: error ? 8 : 0 }}>
                <p>Rec {update.version} is available.</p>
                <button type="button" className="btn primary" disabled={updating} onClick={installUpdate}>
                  {updating ? "Updating…" : "Restart to update"}
                </button>
              </div>
            )}
          </div>
        )}
        {page === "overview" && <Overview info={info} />}
        {page === "settings" && <Settings info={info} onInfo={setInfo} onToast={showToast} />}
        {page === "history" && <History items={history} platform={info.platform} onChanged={refreshHistory} onToast={showToast} />}
      </main>
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
