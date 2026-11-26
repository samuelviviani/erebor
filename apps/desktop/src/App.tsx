import { Suspense, useEffect, useRef } from "react";
import { Layout, Logo } from "./components/Layout";
import { Button } from "./components/ui";
import { alertText } from "./lib/alerts";
import { translateError, useT } from "./lib/i18n";
import { fleet } from "./lib/api";
import { useApp, useLocalMining, useResolvedTheme } from "./lib/store";
import Alerts, { eventText } from "./pages/Alerts";
import Coins from "./pages/Coins";
import Computer from "./pages/Computer";
import Dashboard from "./pages/Dashboard";
import DeviceDetail from "./pages/DeviceDetail";
import Devices from "./pages/Devices";
import Energy from "./pages/Energy";
import Groups from "./pages/Groups";
import Onboarding from "./pages/Onboarding";
import Profiles from "./pages/Profiles";
import Settings from "./pages/Settings";

const PAGES = {
  dashboard: Dashboard,
  coins: Coins,
  energy: Energy,
  computer: Computer,
  settings: Settings,
  devices: Devices,
  device: DeviceDetail,
  groups: Groups,
  profiles: Profiles,
  alerts: Alerts,
};
/** This computer's mining pages: they need its setup (onboarding) first. */
const LOCAL_PAGES = ["dashboard", "coins", "energy", "computer"];

export default function App() {
  const { init, refreshStatus, config, page, status, loadError, toast, setupLocal } =
    useApp();
  const lastAlert = useRef<number | null>(null);
  const theme = useResolvedTheme();

  useEffect(() => {
    init();
    const id = setInterval(refreshStatus, 2000);
    return () => clearInterval(id);
  }, [init, refreshStatus]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.lang = config?.language ?? "en";
  }, [theme, config?.language]);

  // Ambient light turns green while mining (index.css).
  const mining = status?.state === "running";
  useEffect(() => {
    document.documentElement.dataset.mining = mining ? "on" : "off";
  }, [mining]);

  // Backend alerts → toasts (only new ones; the first status sets the baseline).
  useEffect(() => {
    if (!status) return;
    const maxId = Math.max(0, ...status.alerts.map((a) => a.id));
    if (lastAlert.current != null) {
      const lang = config?.language ?? "en";
      for (const a of status.alerts.filter((a) => a.id > lastAlert.current!)) {
        const text = alertText(lang, a);
        if (text) toast(a.level, text);
      }
    }
    lastAlert.current = maxId;
  }, [status, config?.language, toast]);

  const localOn = useLocalMining();
  const setPage = useApp((s) => s.setPage);
  // This computer's pages aren't shown when only other devices are managed.
  useEffect(() => {
    if (!localOn && LOCAL_PAGES.includes(page)) setPage("devices");
  }, [localOn, page, setPage]);
  const fleetOn = useApp((s) => s.config?.defaultView === "devices" || (s.fleet?.devices.length ?? 0) > 1);
  useFleetAlerts(fleetOn);

  if (loadError) return <Unreachable error={loadError} />;
  if (!config) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="skeleton h-8 w-40" />
      </div>
    );
  }
  // Managing devices needs no mining setup here; this computer's pages ask for it.
  const devicesFirst = config.defaultView === "devices";
  if (!config.onboardingCompleted && (!devicesFirst || setupLocal)) return <Onboarding />;
  // Only other devices are managed: this computer's mining pages aren't there.
  const Page = !localOn && LOCAL_PAGES.includes(page) ? Devices : PAGES[page];
  return (
    <Layout>
      <Suspense fallback={<div className="skeleton h-8 w-40" />}>
        <Page />
      </Suspense>
    </Layout>
  );
}

/** The engine does not answer: say why and offer a retry. */
function Unreachable({ error }: { error: string }) {
  const t = useT();
  const init = useApp((s) => s.init);
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="w-full max-w-sm space-y-4 text-center" role="alert">
        <Logo className="mx-auto h-10 w-auto" />
        <p className="text-sm text-danger">{translateError("en", error)}</p>
        <div className="flex justify-center gap-2">
          <Button onClick={() => init()}>{t("computer.retry")}</Button>
        </div>
      </div>
    </div>
  );
}

/** New device warnings and errors as notifications (the first read sets the baseline). */
function useFleetAlerts(on: boolean) {
  const t = useT();
  const lang = useApp((s) => s.config?.language ?? "en");
  const toast = useApp((s) => s.toast);
  const last = useRef<number | null>(null);
  useEffect(() => {
    if (!on) return;
    const poll = async () => {
      try {
        const ev = await fleet.events(last.current ?? 0, 50);
        const max = Math.max(last.current ?? 0, ...ev.map((e) => e.id));
        if (last.current != null) {
          for (const e of ev.filter((e) => e.level !== "info" && e.id > last.current!).reverse()) {
            toast(e.level, `${e.deviceName ?? ""}: ${eventText(t, lang, e)}`);
          }
        }
        last.current = max;
      } catch {
        // Device management unavailable: nothing to report.
      }
    };
    poll();
    const id = setInterval(poll, 5000);
    return () => clearInterval(id);
  }, [on, lang]);
}
