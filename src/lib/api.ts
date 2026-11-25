// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.

import { invoke } from "@tauri-apps/api/core";
import type { Platform } from "./shortcut";

export type Permission = "granted" | "notGranted" | "notRequired";
export type Phase = "idle" | "selecting" | "recording" | "finishing";

export interface AppInfo {
  platform: Platform;
  version: string;
  shortcut: string;
  saveDir: string;
  saveDirIsDefault: boolean;
  checkForUpdates: boolean;
  onboardingComplete: boolean;
  openAtLogin: boolean;
  permission: Permission;
  recordingSupported: boolean;
  phase: Phase;
}

export interface HistoryItem {
  id: string;
  path: string;
  createdAt: string;
  durationSecs: number;
  width: number;
  height: number;
  thumbnail: string | null;
  missing: boolean;
  sizeBytes: number | null;
}

export interface FfmpegStatus {
  found: boolean;
  path: string | null;
  installCommand: string;
}

export interface UpdateInfo {
  version: string;
  notes: string | null;
}

export interface RecordingError {
  message: string;
  permission: boolean;
}

export const api = {
  appInfo: () => invoke<AppInfo>("get_app_info"),
  setShortcut: (accelerator: string) => invoke<AppInfo>("set_shortcut", { accelerator }),
  setShortcutSuspended: (suspended: boolean) => invoke<void>("set_shortcut_suspended", { suspended }),
  chooseSaveDir: () => invoke<AppInfo>("choose_save_dir"),
  resetSaveDir: () => invoke<AppInfo>("reset_save_dir"),
  setOpenAtLogin: (enabled: boolean) => invoke<AppInfo>("set_open_at_login", { enabled }),
  setCheckForUpdates: (enabled: boolean) => invoke<AppInfo>("set_check_for_updates", { enabled }),
  completeOnboarding: () => invoke<AppInfo>("complete_onboarding"),
  permissionStatus: () => invoke<Permission>("permission_status"),
  requestScreenPermission: () => invoke<Permission>("request_screen_permission"),
  openPermissionSettings: () => invoke<void>("open_permission_settings"),
  relaunch: () => invoke<void>("relaunch"),
  checkFfmpeg: () => invoke<FfmpegStatus>("check_ffmpeg"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  listHistory: () => invoke<HistoryItem[]>("list_history"),
  copyRecordingPath: (id: string) => invoke<void>("copy_recording_path", { id }),
  openRecording: (id: string) => invoke<void>("open_recording", { id }),
  revealRecording: (id: string) => invoke<void>("reveal_recording", { id }),
  deleteRecording: (id: string) => invoke<void>("delete_recording", { id }),
  startRecording: (displayId: number, r: { x: number; y: number; width: number; height: number }) =>
    invoke<void>("start_recording", { displayId, ...r }),
  cancelSelection: () => invoke<void>("cancel_selection"),
  uiLog: (message: string) => invoke<void>("ui_log", { message }).catch(() => {}),
  stopRecording: () => invoke<HistoryItem>("stop_recording"),
  pillDone: () => invoke<void>("pill_done"),
  recordingStartedAt: () => invoke<number | null>("recording_started_at"),
  checkForUpdate: () => invoke<UpdateInfo | null>("check_for_update"),
  installUpdate: () => invoke<void>("install_update"),
};

/** Error text from a rejected command. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
