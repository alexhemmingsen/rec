# Working on Rec

Rec records a screen region to MP4 and puts the file path on the clipboard.
Read `SPEC.md` for what the product must do; it is the source of truth for
behaviour.

## Layout

- `crates/capture/`: the `rec-capture` crate. Records one region of one
  display to an H.264 MP4.
  - `src/lib.rs`: platform-independent types and math (output size, bitrate,
    thumbnail conversion), with unit tests.
  - `src/macos/`: ScreenCaptureKit (`mod.rs`) feeding AVAssetWriter
    (`writer.rs`). Rec's own windows are excluded from the capture.
  - `src/windows.rs`: Windows Graphics Capture + Media Foundation (through
    `windows-capture`), written at a constant 30 fps.
  - `src/linux.rs` + `src/gst_pipeline.rs`: GStreamer; `ximagesrc` on X11,
    ScreenCast portal + `pipewiresrc` on Wayland.
- `src-tauri/`: the Tauri app.
  - `recorder.rs`: the flow (shortcut → selection → recording → Stop →
    clipboard) and `AppState`.
  - `windowing.rs`: main window, one selection overlay per display, the Stop
    pill. `macos.rs` holds the AppKit details (window levels, frames,
    handing focus back).
  - `commands.rs`: everything the web UI calls.
  - `settings.rs`, `history.rs`, `naming.rs`, `layout.rs`, `shortcut.rs`,
    `ffmpeg.rs`, `updater.rs`: small modules with unit tests.
- `src/`: React UI. `main.tsx` picks the view from the window label (`main`,
  `overlay-<displayId>`, `pill`). Pure helpers in `src/lib/` have Vitest tests.
- `scripts/release-macos.sh`: signed DMG + updater archive;
  `scripts/latest-json.mjs` merges every platform's signed artifacts into
  the updater's `latest.json`.
- `e2e/`: end-to-end drivers that operate the real app (macOS: Swift with
  CGEvents; Windows: PowerShell with SendInput; Linux: xdotool under Xvfb).
  CI runs them; on macOS CI grants permissions with
  `e2e/macos/grant-permissions.sh` (CI runners only).

## Commands

```sh
pnpm install
pnpm tauri dev                 # run with hot reload
pnpm test                      # Vitest
pnpm build                     # tsc -b + vite build (what CI runs)
cargo test --workspace         # Rust tests, incl. real H.264 encoding
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
pnpm tauri build --bundles app # Rec.app
```

Run the full build and test suite before every commit.

### Iterating

- Iterate locally: `pnpm tauri build --debug --bundles app`, then
  `swiftc -O -o /tmp/rec-e2e e2e/macos/RecE2E.swift && /tmp/rec-e2e
  "$PWD/target/debug/bundle/macos/Rec.app"` drives the real app on this Mac
  (needs Accessibility for the terminal and Screen Recording for Rec). It
  takes over the mouse for about a minute.
- Pushes run only the quick CI (`.github/workflows/ci.yml`). The live
  capture and end-to-end tests on all three platforms
  (`.github/workflows/e2e.yml`) run on demand: `gh workflow run e2e.yml`.
  Run them before a release.
- Release only when asked: push a `vX.Y.Z` tag.

## Gotchas

- Screen capture needs Screen Recording permission for the *responsible*
  process: the terminal when you run from a shell, Rec.app when launched from
  Finder. The live capture test is `#[ignore]`d for that reason:
  `cargo test -p rec-capture --test live -- --ignored`.
- Sleeping displays aren't "active"; `displays()` falls back to the online
  list.
- AppKit calls must run on the main thread: use `app.run_on_main_thread`.
- H.264 needs even dimensions and at most 4096×2304 pixels; see
  `output_dimensions`.
- Signing keys live outside the repo (`~/.tauri/`) and in GitHub secrets
  (`REC_SIGN_P12_BASE64`, `REC_SIGN_P12_PASSWORD`, `TAURI_SIGNING_PRIVATE_KEY`).
  Never commit them. Every release must be signed with the same certificate,
  or users lose the Screen Recording permission on update.
- To release: bump the version in `src-tauri/tauri.conf.json`, the root
  `Cargo.toml` (`[workspace.package]`) and `package.json`, then push a
  `vX.Y.Z` tag.
