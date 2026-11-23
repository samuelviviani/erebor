import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { webToken } from "./lib/api";
import "./index.css";

async function boot() {
  // Browser preview during development only (no Tauri backend): simulated IPC,
  // clearly flagged as demo. Never included in production builds.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installMockBackend } = await import("./dev/mockBackend");
    installMockBackend();
  }
  // Phone/browser link from a node ("…/#token=…"): keep the token, hide it from the URL.
  // Also when the link opens over an already loaded page (only the hash changes).
  const takeToken = () => {
    const token = new URLSearchParams(location.hash.slice(1)).get("token");
    if (!token) return false;
    webToken.set(token);
    history.replaceState(null, "", location.pathname);
    return true;
  };
  takeToken();
  window.addEventListener("hashchange", () => takeToken() && location.reload());
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
boot();
