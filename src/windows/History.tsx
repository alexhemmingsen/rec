import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api, errorText, type HistoryItem } from "../lib/api";
import { formatBytes, formatDuration, formatWhen } from "../lib/format";
import type { Platform } from "../lib/shortcut";

/** Recordings Rec has made, newest first. Nothing is deleted automatically. */
export function History({
  items,
  platform,
  onChanged,
  onToast,
}: {
  items: HistoryItem[];
  platform: Platform;
  onChanged: () => void;
  onToast: (text: string) => void;
}) {
  const folderName = platform === "macos" ? "Finder" : platform === "windows" ? "Explorer" : "folder";

  return (
    <div className="page">
      <h1 className="page-title">History</h1>
      <p className="page-sub">Copy a path again if your clipboard got overwritten, or hand the same clip to another agent.</p>

      {items.length === 0 ? (
        <div className="card empty">
          <h3>No recordings yet</h3>
          <p>Press your shortcut, drag a box and click Stop. Your recordings show up here.</p>
        </div>
      ) : (
        <div className="history-list">
          {items.map((item) => (
            <Row key={item.id} item={item} folderName={folderName} onChanged={onChanged} onToast={onToast} />
          ))}
        </div>
      )}
    </div>
  );
}

function Row({
  item,
  folderName,
  onChanged,
  onToast,
}: {
  item: HistoryItem;
  folderName: string;
  onChanged: () => void;
  onToast: (text: string) => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const act = (p: Promise<void>, done?: string) =>
    p.then(() => done && onToast(done)).catch((e) => onToast(errorText(e))).finally(onChanged);

  const facts = [formatDuration(item.durationSecs), `${item.width}×${item.height}`, formatBytes(item.sizeBytes)]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className={`card rec-row${item.missing ? " missing" : ""}`}>
      <div className="thumb">
        {item.thumbnail ? <img src={convertFileSrc(item.thumbnail)} alt="" draggable={false} /> : "No preview"}
      </div>
      <div className="rec-meta">
        <p className="rec-when">
          {formatWhen(item.createdAt)}
          {item.missing && <span className="badge">Missing</span>}
        </p>
        <p className="rec-facts">{item.missing ? "The file was moved or deleted outside Rec." : facts}</p>
        <p className="path" title={item.path}>
          {item.path}
        </p>
        <div className="rec-actions">
          <button type="button" className="btn" onClick={() => act(api.copyRecordingPath(item.id), "Path copied")}>
            Copy path
          </button>
          <button type="button" className="btn" disabled={item.missing} onClick={() => act(api.openRecording(item.id))}>
            Open
          </button>
          <button type="button" className="btn" onClick={() => act(api.revealRecording(item.id))}>
            Show in {folderName}
          </button>
          {confirming ? (
            <>
              <button
                type="button"
                className="btn primary"
                onClick={() => act(api.deleteRecording(item.id), item.missing ? "Removed" : "Moved to Trash")}
              >
                {item.missing ? "Remove from History" : "Move to Trash"}
              </button>
              <button type="button" className="btn ghost" onClick={() => setConfirming(false)}>
                Cancel
              </button>
            </>
          ) : (
            <button type="button" className="btn ghost danger" onClick={() => setConfirming(true)}>
              Delete
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
