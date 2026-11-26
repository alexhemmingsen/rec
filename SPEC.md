# Rec: product spec

The decisions behind Rec, agreed before it was built. When the code and this
file disagree, fix one of them on purpose.

## What it is

A free, open-source (MIT) desktop app for people building with AI. Press a
shortcut, drag a box, the recording starts. Click Stop and the video's file
path is on your clipboard, ready to paste into Claude Code, Codex, Cursor or
any other agent. The agent reads the video with ffmpeg or any tool it likes.

## The recording flow

- Press the global shortcut. You choose it; the default is ⌃⇧5 (Ctrl+Shift+5
  on Windows and Linux). A crosshair overlay covers the screen.
- Drag a region on one screen. Esc, or a click without dragging, cancels.
- Recording starts the moment you let go. No countdown.
- Recordings are silent, with the mouse pointer visible.
- A floating **■ Stop 0:12** pill sits just outside a corner of the region,
  so it isn't in the video. If the region fills the screen, the pill sits
  inside it and is still left out of the video. The pill is the only way to
  stop.
- Beside the pill, a pen button showing its keys, ⌘⇧D (Ctrl+Shift+D on
  Windows and Linux): click it or press the keys to draw on the region, and
  the lines are in the video. It turns red while on; again turns it off.
  Rec only claims those keys while recording.
- Stop saves `rec-YYYY-MM-DD-HH-MM-SS.mp4` (H.264, 30 fps, full Retina
  sharpness, no time limit) to Downloads or the folder chosen in Settings,
  and puts the **absolute path as plain text** on the clipboard.
- The pill then shows "✓ Path copied" briefly and disappears.
- Pressing the shortcut while selecting or recording does nothing.

## The app

- A normal desktop app with a window and a Dock/taskbar icon. Closing the
  window doesn't quit it (macOS: it stays in the Dock; Windows/Linux: it
  minimizes), so the shortcut keeps working.
- Three sections:
  - **Overview:** a how-to guide.
  - **Settings:** permissions, shortcut, save folder, open at login, update
    check.
  - **History:** thumbnail, date, length; Copy path / Open / Show in folder /
    Delete; shown as "missing" when the file was moved; nothing is ever
    deleted automatically.
- **Onboarding**, with no account: welcome → Screen Recording permission →
  shortcut + "Open at login" (ticked by default) → ffmpeg check (a
  suggestion, skippable; "or use any tool you like") → practice recording.

## Principles

- Everything stays on your computer: no account, no cloud, no analytics, no
  crash reports. The only network call is an update check against GitHub
  Releases, and it can be turned off.
- Not in v1: editing or trimming, audio, GIF, webcam, uploads.

## Tech and release

- Tauri 2 with the capture code in Rust: ScreenCaptureKit on macOS, Windows
  Graphics Capture on Windows, PipeWire on Linux. One codebase.
- Releases go macOS → Windows → Linux. v1 is macOS 14+ on Apple Silicon only.
  Wayland's limits get handled when Linux comes up.
- Signing: a free self-made certificate while building (it keeps the Screen
  Recording permission across updates), and an Apple Developer ID before the
  public launch: the owner's own (Alexander Hemmingsen, team 648JZG624Q), not
  a company's. Windows signing is decided at the Windows release.
- Name: **Rec**, bundle ID `com.alexhem.rec`. Downloads come from GitHub
  Releases. No App Store. The website (rec.alexhem.com) is parked for later.

## Status

| Area | Status |
| --- | --- |
| Recording flow, app, onboarding, History | Done on macOS, Windows and Linux |
| Capture | macOS: ScreenCaptureKit + AVFoundation. Windows: Windows Graphics Capture + Media Foundation. Linux: GStreamer (X11 directly; Wayland through the ScreenCast portal) |
| End-to-end tests | CI drives the real app on Windows and Linux; on macOS CI covers shortcut, overlay, Esc and live capture (its VMs drop scripted drags), and the full flow was verified on a real Mac (2026-11-25) |
| Update check and in-app update | Done (GitHub Releases, signed updater artifacts for all three platforms) |
| Signing while building (free, self-made) | Done, and replaced: 0.1.0 and 0.2.0 were signed with Rec's self-signed certificate |
| Apple Developer ID + notarization | Done from the release after 0.2.0 (set up 2026-11-26): `scripts/release-macos.sh` signs with the Developer ID (hardened runtime), notarizes and staples both Rec.app and the DMG. macOS ties Screen Recording to the team ID, so it survives updates and certificate renewals; people on 0.1.0/0.2.0 grant it once more |
| Windows code signing | Decided: unsigned until a certificate is bought (SmartScreen warns once; documented). The release signs automatically once `REC_WINDOWS_PFX_BASE64` / `REC_WINDOWS_PFX_PASSWORD` are set |
| Stop pill out of full-screen recordings | macOS and Windows: the OS leaves Rec's windows out of the capture. Linux (no per-window exclusion): Rec covers the pill's area with what was there when recording started |
| Website | Parked |
