import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText, type AppInfo, type HistoryItem, type RecordingError, type UpdateInfo } from "../lib/api";
import { Onboarding } from "./Onboarding";
import { General } from "./General";
import { History } from "./History";
import generalIcon from "../assets/icons/general.svg";
import historyIcon from "../assets/icons/history.svg";

type Page = "general" | "history";

const PAGES: { id: Page; label: string; icon: string }[] = [
  { id: "general", label: "General", icon: generalIcon },
  { id: "history", label: "History", icon: historyIcon },
];

export function Main() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [page, setPage] = useState<Page>("general");
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
        <div className="sidebar-nav">
          {PAGES.map((p) => (
            <button
              key={p.id}
              type="button"
              className="nav-item"
              aria-current={page === p.id ? "page" : undefined}
              onClick={() => setPage(p.id)}
            >
              <img src={p.icon} className="icon" width={16} height={16} alt="" draggable={false} />
              {p.label}
            </button>
          ))}
        </div>
        <div className="sidebar-foot">Version {info.version}</div>
      </nav>
      <main className="content">
        <div className="drag-strip" data-tauri-drag-region />
        {(error || update) && (
          <div className="banners">
            {error && (
              <div className="banner" role="alert">
                <p>{error.message}</p>
                {error.permission && page !== "general" && (
                  <button type="button" className="button" onClick={() => setPage("general")}>
                    Open General
                  </button>
                )}
                <button type="button" className="button" onClick={() => setError(null)}>
                  Dismiss
                </button>
              </div>
            )}
            {update && (
              <div className="banner update" role="status">
                <p>Rec {update.version} is available.</p>
                <button type="button" className="button primary" disabled={updating} onClick={installUpdate}>
                  {updating ? "Updating…" : "Restart to update"}
                </button>
              </div>
            )}
          </div>
        )}
        {page === "general" && <General info={info} onInfo={setInfo} onToast={showToast} />}
        {page === "history" && <History items={history} platform={info.platform} onChanged={refreshHistory} onToast={showToast} />}
      </main>
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
