import { useEffect, useState } from "react";
import { create } from "zustand";
import { api, errorText, type AppConfig, type AppInfo, type Status } from "./api";

export type Page = "dashboard" | "coins" | "stats" | "settings";
export type SettingsTab = "general" | "when" | "energy" | "safety" | "notify" | "data" | "advanced";

/** One reading of the live sensors, for the Home computer charts. */
export interface Sample {
  ts: number;
  cpu: number;
  cpuTemp: number | null;
  mem: number;
  gpus: { util: number | null; temp: number | null; power: number | null; hash: number | null }[];
}
/** 2 s polling → 5 minutes of history. */
export const HISTORY = 150;

export interface Toast {
  id: number;
  level: "info" | "warning" | "error";
  text: string;
  /** Same message repeated: shown once with "×count". */
  count: number;
}

interface AppStore {
  page: Page;
  info: AppInfo | null;
  config: AppConfig | null;
  status: Status | null;
  loadError: string | null;
  toasts: Toast[];
  history: Sample[];
  /** Coin being switched to (the miner restarts), for the status line. */
  coinBusy: string | null;
  setCoinBusy: (id: string | null) => void;
  /** Until when a saved change is restarting the miner (ms epoch), for the status bar. */
  restartUntil: number;
  /** Quick guide (beginner tour) visibility. */
  guide: boolean;
  setGuide: (open: boolean) => void;
  setPage: (p: Page) => void;
  /** Section shown in Settings; other pages can open a given one. */
  settingsTab: SettingsTab;
  openSettings: (tab: SettingsTab) => void;
  init: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  /** Persist a modified config; returns the saved config or throws an error string. */
  saveConfig: (next: AppConfig) => Promise<AppConfig>;
  exitDemo: () => Promise<void>;
  setConfig: (c: AppConfig) => void;
  toast: (level: Toast["level"], text: string) => void;
  dismissToast: (id: number) => void;
  /** Pause auto-dismiss while the pointer is on the notifications. */
  holdToasts: (hold: boolean) => void;
}

let toastSeq = 0;
const TOAST_MS = { info: 3500, warning: 6000, error: 8000 } as const;
const toastTimers = new Map<number, ReturnType<typeof setTimeout>>();
function scheduleToast(id: number, ms: number) {
  clearTimeout(toastTimers.get(id));
  toastTimers.set(id, setTimeout(() => useApp.getState().dismissToast(id), ms));
}

export const useApp = create<AppStore>((set, get) => ({
  page: "dashboard",
  info: null,
  config: null,
  status: null,
  loadError: null,
  toasts: [],
  history: [],
  coinBusy: null,
  setCoinBusy: (coinBusy) => set({ coinBusy }),
  restartUntil: 0,
  guide: false,
  setGuide: (guide) => set({ guide }),
  setPage: (page) => set({ page }),
  settingsTab: "general",
  openSettings: (settingsTab) => set({ settingsTab, page: "settings" }),
  init: async () => {
    try {
      const [info, config] = await Promise.all([api.appInfo(), api.getConfig()]);
      set({ info, config, loadError: null });
      await get().refreshStatus();
    } catch (e) {
      set({ loadError: errorText(e) });
    }
  },
  refreshStatus: async () => {
    try {
      const prev = get().status;
      const status = await api.getStatus();
      const h = status.hardware;
      const sample: Sample = {
        ts: Date.now(),
        cpu: h.cpuUsage,
        cpuTemp: h.cpuTempC,
        mem: h.memTotalBytes ? (h.memUsedBytes / h.memTotalBytes) * 100 : 0,
        gpus: h.gpus.map((g, i) => ({ util: g.utilPercent, temp: g.tempC, power: g.powerW, hash: status.gpus[i]?.hashrate ?? null })),
      };
      set({ status, history: [...get().history, sample].slice(-HISTORY) });
      // Jobs (install, benchmark) change protected config fields in the backend.
      if (status.job?.done && prev?.job && !prev.job.done) set({ config: await api.getConfig() });
    } catch {
      // Keep the last status; the UI shows connection state from it.
    }
  },
  saveConfig: async (next) => {
    try {
      // Same rule as the engine: these changes restart a running miner.
      const prev = get().config;
      const active = get().status?.state === "running" || get().status?.state === "paused";
      const same = (k: keyof AppConfig) => JSON.stringify(prev?.[k]) === JSON.stringify(next[k]);
      if (active && prev?.demoMode === next.demoMode && !(["wallet", "coin", "pool", "performance"] as const).every(same)) set({ restartUntil: Date.now() + 30_000 });
      const saved = await api.updateConfig(next);
      set({ config: saved });
      await get().refreshStatus(); // status (e.g. start blockers) depends on config
      return saved;
    } catch (e) {
      throw errorText(e);
    }
  },
  // Back to real mode; without a real wallet + miner, redo the real onboarding.
  exitDemo: async () => {
    const c = get().config!;
    try {
      await get().saveConfig({ ...c, demoMode: false, onboardingCompleted: !!c.wallet && !!c.miner.path });
      set({ page: "dashboard" });
    } catch (e) {
      get().toast("error", String(e));
    }
  },
  setConfig: (config) => set({ config }),
  toast: (level, text) => {
    const list = get().toasts;
    const same = list.find((t) => t.level === level && t.text === text);
    const next = same ? { ...same, count: same.count + 1 } : { id: ++toastSeq, level, text, count: 1 };
    set({ toasts: [...list.filter((t) => t !== same), next].slice(-5) });
    scheduleToast(next.id, TOAST_MS[level]);
  },
  dismissToast: (id) => {
    clearTimeout(toastTimers.get(id));
    toastTimers.delete(id);
    set({ toasts: get().toasts.filter((t) => t.id !== id) });
  },
  holdToasts: (hold) => {
    for (const t of get().toasts) {
      if (hold) clearTimeout(toastTimers.get(t.id));
      else scheduleToast(t.id, 2500);
    }
  },
}));

/** The theme actually shown: "system" follows the operating system, live. */
export function useResolvedTheme(): "dark" | "light" {
  const theme = useApp((s) => s.config?.theme ?? "light");
  const [query] = useState(() => (typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: light)") : null));
  const [light, setLight] = useState(!!query?.matches);
  useEffect(() => {
    if (!query) return;
    const on = () => setLight(query.matches);
    query.addEventListener("change", on);
    return () => query.removeEventListener("change", on);
  }, [query]);
  return theme === "system" ? (light ? "light" : "dark") : theme;
}
