/** 0:07, 1:05, 1:02:03 */
export function formatDuration(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const ss = String(sec).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** 812 KB, 4.2 MB */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes)) return "";
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${Math.round(kb)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

/** "Today 14:03", "Yesterday 09:12", "25 Nov 2026 14:03" */
export function formatWhen(iso: string, now: Date = new Date()): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const time = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const day = 24 * 60 * 60 * 1000;
  if (d.getTime() >= startOfToday) return `Today ${time}`;
  if (d.getTime() >= startOfToday - day) return `Yesterday ${time}`;
  const date = d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  return `${date} ${time}`;
}

/** Shorten a home-folder path for display: /Users/me/Downloads → ~/Downloads */
export function tildify(path: string): string {
  const m = /^(\/Users\/[^/]+|\/home\/[^/]+)(\/.*)?$/.exec(path);
  return m ? `~${m[2] ?? ""}` : path;
}
