import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./index.css";

async function boot() {
  // Browser preview during development only (no Tauri backend): simulated IPC,
  // clearly flagged as demo. Never included in production builds.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installMockBackend } = await import("./dev/mockBackend");
    installMockBackend();
  }
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
boot();
