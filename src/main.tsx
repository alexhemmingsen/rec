import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./styles.css";
import { Main } from "./windows/Main";
import { Overlay } from "./windows/Overlay";
import { Pill } from "./windows/Pill";
import { displayIdFromLabel } from "./lib/selection";

// One bundle serves all windows; the window label decides what to render.
const label = getCurrentWebviewWindow().label;
const overlayDisplay = displayIdFromLabel(label);
const kind = overlayDisplay !== null ? "overlay" : label === "pill" ? "pill" : "main";
document.documentElement.dataset.window = kind;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {kind === "overlay" ? <Overlay displayId={overlayDisplay!} /> : kind === "pill" ? <Pill /> : <Main />}
  </StrictMode>,
);
