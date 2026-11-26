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

- `pnpm tauri dev`: run from source with hot reload (UI instantly, Rust in
  ~20 s). Recording in this mode counts against the terminal, which then
  needs Screen Recording too.
- `./scripts/dev-mac.sh`: build Rec.app (debug, ~30 s), sign it with Rec's
  certificate so the Screen Recording permission survives rebuilds, and
  relaunch it. Use this to try recording changes.
- `./scripts/e2e-mac.sh`: the same build, then the end-to-end driver
  operates the app on this Mac (~2 min; takes over the mouse; needs
  Accessibility for the terminal). Removes its test recordings afterwards.
- Pushes run only the quick CI (`.github/workflows/ci.yml`). The live
  capture and end-to-end tests on all three platforms
  (`.github/workflows/e2e.yml`) run on demand: `gh workflow run e2e.yml`.
  Run them before a release. GitHub's macOS VMs drop the mouse on its way
  to Rec, so there CI runs only the keyboard steps; before a release, also
  run `./scripts/e2e-mac.sh` on a real Mac for the rest.
- Releases are automatic: a push to main that carries a new version
  (see below) builds, signs, notarizes and publishes it. Bump the version
  only when asked to release; other pushes just run CI.

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
- Signing keys live outside the repo, in `~/.tauri/`:
  - `rec-developer-id/`: the Apple Developer ID (`.p12` + `p12-password.txt`)
    and the App Store Connect key for notarization (`AuthKey_*.p8`,
    `notary.json`).
  - `rec-codesign/`: the old self-signed certificate (0.1.0 and 0.2.0).
  - `rec-updater.key`: the updater key.

  The same keys are in GitHub secrets: `REC_SIGN_P12_BASE64`,
  `REC_SIGN_P12_PASSWORD`, `REC_NOTARY_API_KEY_JSON` and
  `TAURI_SIGNING_PRIVATE_KEY`. Never commit them.

  Releases are signed with Rec's Apple Developer ID. macOS ties the Screen
  Recording permission to its team, so a renewed certificate is fine.
  Signing with another team's certificate makes users grant the permission
  again.
- To release: bump the version in `src-tauri/tauri.conf.json`, the root
  `Cargo.toml` (`[workspace.package]`) and `package.json` (all three the
  same, or the release stops), and push to main. `release.yml` sees that
  `vX.Y.Z` doesn't exist yet, releases it and creates the tag; pushes
  that keep the version stop after its first job.
