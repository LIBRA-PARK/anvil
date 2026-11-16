import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 개발 서버 설정 (https://v2.tauri.app/start/frontend/vite/)
export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2021",
  },
});
