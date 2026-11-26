import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./index.css";

async function boot() {
  // Browser preview during development only (no Tauri backend): simulated IPC,
  // clearly flagged as demo. Never included in production builds.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installMockBackend } = await import("./dev/mockBackend");
    // ?demo opens straight on the app with simulated data (screenshots, design work).
    const q = new URLSearchParams(location.search);
    const demo = q.has("demo");
    // ?cfg={...} adds config fields (whole objects), e.g. to show options switched on.
    const cfg = q.get("cfg");
    installMockBackend(
      demo ? { onboardingCompleted: true, demoMode: true, ...(cfg ? JSON.parse(cfg) : {}) } : undefined,
      // Simulated remote devices and ASICs in the device list: in demo mode, or ?devices.
      { devices: demo || q.has("devices") },
    );
    // ?page=energy opens a section directly (no tour), for screenshots.
    const page = q.get("page");
    if (page) {
      localStorage.setItem("erebor.guideSeen", "1");
      const { useApp } = await import("./lib/store");
      useApp.setState({ page: page as never });
    }
  }
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
boot();
