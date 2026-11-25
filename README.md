<p align="center">
  <img src="assets/icon.png" width="112" alt="Rec icon">
</p>

<h1 align="center">Rec</h1>

<p align="center"><b>Record a screen region, paste the file path into your AI agent.</b></p>

Taking a screenshot is one shortcut. Recording your screen should be too. Rec is a small, free desktop app for people building with AI: press a shortcut, drag a box, and the recording starts. Click **Stop** and the video's path is on your clipboard, ready to paste into Claude Code, Codex, Cursor or any other agent.

```
/Users/you/Downloads/rec-2026-11-25-14-03-12.mp4
```

Your agent reads the video with ffmpeg (or any tool it likes) and sees exactly what you saw.

## How it works

1. **Press your shortcut** (⌃⇧5 by default; change it in Settings). A crosshair appears.
2. **Drag a box.** Recording starts the moment you let go. Esc, or a click without dragging, cancels.
3. **Click Stop** on the small pill floating next to the region. The pill is never in the video.
4. **Paste.** The MP4's absolute path is on your clipboard.

Recordings are silent H.264 MP4s at full Retina sharpness and 30 fps, with the mouse pointer visible. They go to your Downloads folder, or a folder you pick.

## Download

Get `Rec_<version>_aarch64.dmg` from the [latest release](https://github.com/alexhemmingsen/rec/releases/latest). It needs macOS 14 (Sonoma) or newer on Apple Silicon.

**First launch:** Rec isn't notarized by Apple yet, so macOS says it "could not verify" the app. Open **System Settings → Privacy & Security**, scroll down and click **Open Anyway**. You only do this once; updates install from inside Rec.

The first run walks you through granting Screen Recording permission, choosing a shortcut, checking for ffmpeg and making a practice recording.

## Privacy

Everything stays on your computer. There's no account, no cloud and no analytics or crash reporting. The only network request Rec makes is the update check against this repository's GitHub Releases. It sends nothing about you or your recordings, and you can turn it off in Settings.

## The app

- **Overview:** a short guide.
- **Settings:** Screen Recording permission, the shortcut, the save folder, Open at login and the update check.
- **History:** every recording with a thumbnail, date and length. Copy the path again, open it, show it in Finder, or delete it (it goes to the Trash). Rec never deletes anything on its own.

Closing the window keeps Rec running, so the shortcut keeps working. Quit with ⌘Q.

## Platforms

Rec is built as one codebase for macOS, Windows and Linux ([Tauri 2](https://tauri.app) + Rust). It ships on macOS first; Windows (Windows Graphics Capture) and Linux (PipeWire through the ScreenCast portal) follow. On those platforms the app builds and runs today, but recording reports that it isn't available yet.

## Building from source

You need Rust (stable), Node.js 22+ and pnpm.

```sh
pnpm install
pnpm tauri dev          # run the app with hot reload
pnpm test               # frontend unit tests
cargo test --workspace  # Rust tests, including real H.264 encoding
pnpm tauri build        # Rec.app and a DMG in target/release/bundle
```

When you run from a terminal, macOS attributes screen recording to the terminal app, so grant Screen Recording to it (or test with the built `Rec.app`). `scripts/release-macos.sh` builds the signed release artifacts; see `AGENTS.md` for the layout of the code.

## License

[MIT](LICENSE)
