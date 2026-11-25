import type { AppInfo } from "../lib/api";
import { tildify } from "../lib/format";
import { Keys } from "../components/Keys";

/** A short guide to how Rec works. */
export function Overview({ info }: { info: AppInfo }) {
  return (
    <div className="page">
      <h1 className="page-title">Overview</h1>
      <p className="page-sub">Record a part of your screen and paste it straight into your AI agent.</p>

      <div className="card hero">
        <div className="hero-text">
          <h2>Press your shortcut to start</h2>
          <p>It works from any app, as long as Rec is running. Closing this window doesn't quit Rec.</p>
        </div>
        <Keys shortcut={info.shortcut} platform={info.platform} large />
      </div>

      <div className="section-label">How it works</div>
      <ol className="card steps">
        <li className="step">
          <span className="step-num">1</span>
          <div>
            <p className="step-title">Press the shortcut</p>
            <p>A crosshair appears over your screen.</p>
          </div>
        </li>
        <li className="step">
          <span className="step-num">2</span>
          <div>
            <p className="step-title">Drag a box</p>
            <p>Recording starts the moment you let go. Press Esc, or click without dragging, to cancel.</p>
          </div>
        </li>
        <li className="step">
          <span className="step-num">3</span>
          <div>
            <p className="step-title">Click Stop</p>
            <p>The Stop button floats next to the area you're recording and isn't part of the video.</p>
          </div>
        </li>
        <li className="step">
          <span className="step-num">4</span>
          <div>
            <p className="step-title">Paste into your agent</p>
            <p>
              The video's path is on your clipboard. Paste it into Claude Code, Codex, Cursor or any other agent.
            </p>
          </div>
        </li>
      </ol>

      <div className="card tip">
        <strong>Tip:</strong> agents read the video with ffmpeg or any similar tool. Recordings are silent MP4 files
        saved to <span className="mono">{tildify(info.saveDir)}</span>, and they never leave your computer.
      </div>
    </div>
  );
}
