import { lazy, Suspense, useEffect, useRef } from "react";
import { Layout, Logo } from "./components/Layout";
import { Button } from "./components/ui";
import { translate, translateError, useT } from "./lib/i18n";
import { useApp, useResolvedTheme } from "./lib/store";
import Coins from "./pages/Coins";
import Dashboard from "./pages/Dashboard";
import Onboarding from "./pages/Onboarding";
import Settings from "./pages/Settings";

// Recharts is large: load the statistics page on demand.
const Stats = lazy(() => import("./pages/Stats"));

const PAGES = {
  dashboard: Dashboard,
  coins: Coins,
  stats: Stats,
  settings: Settings,
};

export default function App() {
  const { init, refreshStatus, config, page, status, loadError, toast } =
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

  // Backend alerts → toasts (only new ones; the first status sets the baseline).
  useEffect(() => {
    if (!status) return;
    const maxId = Math.max(0, ...status.alerts.map((a) => a.id));
    if (lastAlert.current != null) {
      const lang = config?.language ?? "en";
      for (const a of status.alerts.filter((a) => a.id > lastAlert.current!)) {
        const text =
          translate(lang, `alert.${a.code}`) +
          (a.detail ? ` · ${a.detail}` : "");
        toast(a.level, text);
      }
    }
    lastAlert.current = maxId;
  }, [status, config?.language, toast]);

  if (loadError) return <Unreachable error={loadError} />;
  if (!config) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="skeleton h-8 w-40" />
      </div>
    );
  }
  if (!config.onboardingCompleted) return <Onboarding />;
  const Page = PAGES[page];
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
