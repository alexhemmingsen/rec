import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api, errorText, type HistoryItem } from "../lib/api";
import { formatLength, formatSize, folderOf, fileName } from "../lib/format";
import type { Platform } from "../lib/shortcut";
import copyIcon from "../assets/icons/copy.svg";
import checkIcon from "../assets/icons/check.svg";

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
  const folderName = platform === "macos" ? "finder" : platform === "windows" ? "explorer" : "folder";

  return (
    <div className="page">
      <h1 className="page-title">History</h1>
      {items.length === 0 ? (
        <div className="group empty">
          <p className="empty-title">No recordings yet</p>
          <p className="empty-hint">Press your shortcut, drag a box and click Stop. Your recordings show up here.</p>
        </div>
      ) : (
        <div className="group">
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
  const act = (p: Promise<void>, done?: string) =>
    p.then(() => done && onToast(done)).catch((e) => onToast(errorText(e))).finally(onChanged);

  // After copying, the copy icon briefly becomes a checkmark.
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const id = window.setTimeout(() => setCopied(false), 1500);
    return () => window.clearTimeout(id);
  }, [copied]);
  const copyPath = () =>
    api
      .copyRecordingPath(item.id)
      .then(() => setCopied(true))
      .catch((e) => onToast(errorText(e)));

  return (
    <div className={`group-row history-row${item.missing ? " missing" : ""}`}>
      <div className="row-label">
        <div className="thumb">
          {item.thumbnail && <img src={convertFileSrc(item.thumbnail)} alt="" draggable={false} />}
        </div>
        <div className="history-meta">
          <p className="history-name">{fileName(item.path)}</p>
          <div className="history-path">
            <span className="truncate" title={item.path}>
              {folderOf(item.path)}
            </span>
            <button
              type="button"
              className="icon-button"
              title={copied ? "Copied" : "Copy path"}
              aria-label={copied ? "Path copied" : "Copy path"}
              onClick={copyPath}
            >
              {copied ? (
                <img src={checkIcon} className="icon" width={16} height={16} alt="" draggable={false} />
              ) : (
                <img src={copyIcon} width={16} height={16} alt="" draggable={false} />
              )}
            </button>
          </div>
          <div className="history-facts">
            {item.missing ? (
              <span>Moved or deleted outside Rec</span>
            ) : (
              <>
                <span>{formatLength(item.durationSecs)}</span>
                <span className="vrule short" />
                <span>{formatSize(item.sizeBytes)}</span>
                <span className="vrule short" />
                <span>
                  {item.width}×{item.height}
                </span>
              </>
            )}
          </div>
        </div>
      </div>
      <div className="row-controls tight">
        <button type="button" className="button" onClick={() => act(api.revealRecording(item.id))}>
          Show in {folderName}
        </button>
        <button type="button" className="button" disabled={item.missing} onClick={() => act(api.openRecording(item.id))}>
          Watch
        </button>
        <span className="vrule" />
        <button
          type="button"
          className="button destructive"
          onClick={() => act(api.deleteRecording(item.id), item.missing ? "Removed from History" : "Moved to Trash")}
        >
          Delete
        </button>
      </div>
    </div>
  );
}
