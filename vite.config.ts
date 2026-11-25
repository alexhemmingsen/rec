/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and must not have the dev server clear its output.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/crates/**"] } },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: "safari16", outDir: "dist", emptyOutDir: true },
  // Generous timeout: the first Intl call on Windows CI runners is slow.
  test: { environment: "node", include: ["src/**/*.test.ts"], testTimeout: 30000 },
});
