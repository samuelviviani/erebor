import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { Layout, Logo } from "./components/Layout";
import { Button, inputClass } from "./components/ui";
import { inApp, webToken } from "./lib/api";
import { translate, translateError, useT } from "./lib/i18n";
import { useApp, useResolvedTheme } from "./lib/store";
import Coins from "./pages/Coins";
import Dashboard from "./pages/Dashboard";
import Onboarding from "./pages/Onboarding";
import Settings from "./pages/Settings";
import WalletPage from "./pages/Wallet";

// Recharts is large: load the statistics page on demand.
const Stats = lazy(() => import("./pages/Stats"));

const PAGES = {
  dashboard: Dashboard,
  coins: Coins,
  wallet: WalletPage,
  stats: Stats,
  settings: Settings,
};

export default function App() {
  const { init, refreshStatus, config, page, status, loadError, toast } = useApp();
  const lastAlert = useRef<number | null>(null);
  const theme = useResolvedTheme();

  useEffect(() => {
    init();
    const id = setInterval(refreshStatus, 2000);
    return () => clearInterval(id);
  }, [init, refreshStatus]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.lang = config?.language ?? "it";
  }, [theme, config?.language]);

  // Backend alerts → toasts (only new ones; the first status sets the baseline).
  useEffect(() => {
    if (!status) return;
    const maxId = Math.max(0, ...status.alerts.map((a) => a.id));
    if (lastAlert.current != null) {
      const lang = config?.language ?? "it";
      for (const a of status.alerts.filter((a) => a.id > lastAlert.current!)) {
        const text = translate(lang, `alert.${a.code}`) + (a.detail ? ` · ${a.detail}` : "");
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

/**
 * The engine does not answer. In the app: retry. In a browser on the node's page: ask
 * for the access token.
 */
function Unreachable({ error }: { error: string }) {
  const t = useT();
  const init = useApp((s) => s.init);
  const [token, setToken] = useState("");
  const login = !inApp() && (error === "remote_unauthorized" || !webToken.get());
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="w-full max-w-sm space-y-4 text-center" role="alert">
        <Logo className="mx-auto h-10 w-auto" />
        {login ? (
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              webToken.set(token);
              init();
            }}
          >
            <p className="text-sm text-muted">{t("web.login")}</p>
            <input className={`${inputClass} font-mono text-xs`} value={token} onChange={(e) => setToken(e.target.value)} autoComplete="off" spellCheck={false} aria-label={t("remote.token")} />
            <Button type="submit" variant="primary" disabled={token.trim().length < 32} className="w-full justify-center">
              {t("web.connect")}
            </Button>
          </form>
        ) : (
          <>
            <p className="text-sm text-danger">{translateError("it", error)}</p>
            <div className="flex justify-center gap-2">
              <Button onClick={() => init()}>{t("computer.retry")}</Button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
