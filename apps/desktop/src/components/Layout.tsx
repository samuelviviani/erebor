import {
  BarChart3,
  CircleX,
  Coins,
  Home,
  Info,
  PanelLeftClose,
  PanelLeftOpen,
  Settings as SettingsIcon,
  TriangleAlert,
  Wallet,
  X,
  type LucideIcon,
} from "lucide-react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useState, type ReactNode } from "react";
import { inApp } from "../lib/api";
import { useT } from "../lib/i18n";
import { useApp, useResolvedTheme, type Page } from "../lib/store";
import { Guide } from "./Guide";
import { StatusBar } from "./MinerStatus";
import { cx } from "./ui";

const MAIN: { page: Page; icon: LucideIcon }[] = [
  { page: "dashboard", icon: Home },
  { page: "coins", icon: Coins },
  { page: "stats", icon: BarChart3 },
  { page: "wallet", icon: Wallet },
];

export function Logo({ className }: { className?: string }) {
  // Cream ink on the dark theme, navy ink on the light one (brand/erebor-mark-*.svg).
  const theme = useResolvedTheme();
  return (
    <img src={`/logo-${theme}.svg`} alt="" aria-hidden className={className} />
  );
}

/** Fixed width; the only choice is labels or icons only (remembered per viewer). */
function readCollapsed() {
  try {
    const saved = localStorage.getItem("erebor.sidebar");
    // Phones: start compact so the page keeps most of the width.
    return saved ? saved === "collapsed" : window.innerWidth < 640;
  } catch {
    return window.innerWidth < 640;
  }
}

function Sidebar() {
  const t = useT();
  const { page, setPage, info } = useApp();
  const [collapsed, setCollapsed] = useState(readCollapsed);
  const toggle = () => {
    setCollapsed(!collapsed);
    try {
      localStorage.setItem("erebor.sidebar", collapsed ? "open" : "collapsed");
    } catch {
      /* per-viewer convenience only */
    }
  };

  const item = ({ page: p, icon: Icon }: { page: Page; icon: LucideIcon }) => (
    <li key={p}>
      <button
        type="button"
        onClick={() => setPage(p)}
        aria-current={page === p ? "page" : undefined}
        title={collapsed ? t(`nav.${p}`) : undefined}
        className={cx(
          "relative flex h-9 w-full items-center gap-3 rounded-xl px-3 text-[13.5px] transition-colors duration-200",
          collapsed && "justify-center px-0",
          page === p
            ? "bg-surface-2 font-medium text-fg"
            : "text-muted hover:bg-surface-2/60 hover:text-fg",
        )}
      >
        {page === p && (
          <span
            aria-hidden
            className="pop-in absolute top-2 bottom-2 left-0 w-[3px] rounded-full bg-accent"
          />
        )}
        <Icon
          className={cx(
            "size-[18px] shrink-0 transition-colors",
            page === p && "text-accent",
          )}
          aria-hidden
        />
        {!collapsed && <span className="truncate">{t(`nav.${p}`)}</span>}
      </button>
    </li>
  );

  return (
    <nav
      aria-label="Erebor"
      className={cx(
        "flex shrink-0 flex-col overflow-hidden border-r border-line bg-sidebar transition-[width] duration-200",
        collapsed ? "w-[68px]" : "w-52",
      )}
    >
      <div
        className={cx(
          "flex h-16 items-center gap-2.5 px-4",
          collapsed && "justify-center px-0",
        )}
      >
        <Logo className="h-7 w-auto shrink-0" />
        {!collapsed && (
          <>
            <span className="text-[15px] font-semibold tracking-wide">
              Erebor
            </span>
            <span className="num text-[11px] text-faint">
              v{info?.version ?? "—"}
            </span>
            <button
              type="button"
              onClick={toggle}
              aria-label={t("nav.collapse")}
              title={t("nav.collapse")}
              className="ml-auto rounded-lg p-1.5 text-faint hover:bg-surface-2 hover:text-fg"
            >
              <PanelLeftClose className="size-4" />
            </button>
          </>
        )}
      </div>
      {collapsed && (
        <button
          type="button"
          onClick={toggle}
          aria-label={t("nav.expand")}
          title={t("nav.expand")}
          className="mx-auto mb-2 rounded-lg p-1.5 text-faint hover:bg-surface-2 hover:text-fg"
        >
          <PanelLeftOpen className="size-4" />
        </button>
      )}

      <div className="flex-1 overflow-y-auto px-3">
        <ul className="space-y-0.5">{MAIN.map(item)}</ul>
      </div>

      <ul className="p-3">{item({ page: "settings", icon: SettingsIcon })}</ul>
    </nav>
  );
}

const TOAST_ICON = {
  info: <Info className="size-4 text-accent" />,
  warning: <TriangleAlert className="size-4 text-warn" />,
  error: <CircleX className="size-4 text-danger" />,
};

/**
 * Notifications as one quiet stack: the newest in front, older ones tucked behind it.
 * Pointer or focus on the stack fans it out and pauses auto-dismiss.
 */
function Toasts() {
  const t = useT();
  const { toasts, dismissToast, holdToasts } = useApp();
  const [open, setOpen] = useState(false);
  const list = [...toasts].reverse(); // newest first
  const expand = (on: boolean) => {
    setOpen(on);
    holdToasts(on);
  };
  useEffect(() => {
    if (!toasts.length) setOpen(false);
  }, [toasts.length]);
  return (
    <section
      aria-label={t("notifications")}
      aria-live="polite"
      onMouseEnter={() => expand(true)}
      onMouseLeave={() => expand(false)}
      onFocus={() => expand(true)}
      onBlur={(e) =>
        !e.currentTarget.contains(e.relatedTarget) && expand(false)
      }
      className="fixed right-4 bottom-4 z-40 w-80 max-w-[calc(100vw-32px)]"
    >
      <div className={open ? "flex flex-col-reverse gap-2" : "relative"}>
        {list.map((x, i) => {
          const behind = !open && i > 0;
          return (
            <div
              key={x.id}
              role={x.level === "error" ? "alert" : undefined}
              aria-hidden={behind || undefined}
              style={
                behind
                  ? {
                      transform: `translateY(${-8 * Math.min(i, 2)}px) scale(${1 - 0.05 * Math.min(i, 2)})`,
                      transformOrigin: "top",
                      zIndex: -i,
                      opacity: i > 2 ? 0 : 1,
                    }
                  : undefined
              }
              className={cx(
                "pop-in shadow-soft flex items-start gap-2.5 rounded-xl border border-line bg-surface px-3.5 py-2.5 text-[13px] leading-snug transition-[transform,opacity] duration-200",
                behind && "absolute inset-0 overflow-hidden",
              )}
            >
              <span className={cx("mt-px shrink-0", behind && "invisible")}>
                {TOAST_ICON[x.level]}
              </span>
              <span className={cx("min-w-0 flex-1", behind && "invisible")}>
                {x.text}
                {x.count > 1 && (
                  <span className="num ml-1.5 rounded-md bg-surface-2 px-1.5 py-px text-[11px] text-muted">
                    ×{x.count}
                  </span>
                )}
              </span>
              {!open && i === 0 && list.length > 1 && (
                <span className="num shrink-0 text-[11px] text-faint">
                  +{list.length - 1}
                </span>
              )}
              <button
                type="button"
                tabIndex={behind ? -1 : 0}
                aria-label={t("close")}
                onClick={() => dismissToast(x.id)}
                className={cx(
                  "-mr-1 shrink-0 rounded p-0.5 text-faint hover:text-fg",
                  behind && "invisible",
                )}
              >
                <X className="size-3.5" />
              </button>
            </div>
          );
        })}
      </div>
    </section>
  );
}

/**
 * Self-update from signed GitHub releases: checks at launch and every hour, installs
 * on its own and restarts. If that fails, a banner offers to retry by hand.
 */
function Updater() {
  const t = useT();
  const [state, setState] = useState<"idle" | "installing" | "failed">("idle");
  const [version, setVersion] = useState("");
  const run = async () => {
    try {
      const update = await check();
      if (!update) return;
      setVersion(update.version);
      setState("installing");
      await update.downloadAndInstall();
      await relaunch();
    } catch {
      setState((s) => (s === "installing" ? "failed" : s)); // offline check: retry later, quietly
    }
  };
  useEffect(() => {
    if (!inApp()) return;
    void run();
    const id = setInterval(run, 60 * 60 * 1000);
    return () => clearInterval(id);
  }, []);
  if (state === "idle") return null;
  return (
    <div
      className="flex items-center justify-between gap-3 border-b border-accent/30 bg-accent/10 px-6 py-2 text-xs text-accent"
      role="status"
    >
      <span>
        {t(state === "installing" ? "update.installing" : "update.failed")} (v
        {version})
      </span>
      {state === "failed" && (
        <button
          className="shrink-0 rounded-lg border border-accent/40 px-2.5 py-1 font-medium hover:bg-accent/15"
          onClick={() => void run()}
        >
          {t("update.button")}
        </button>
      )}
    </div>
  );
}

export function Layout({ children }: { children: ReactNode }) {
  const t = useT();
  const demo = useApp((s) => s.status?.demo);
  const exitDemo = useApp((s) => s.exitDemo);
  const page = useApp((s) => s.page);
  return (
    <div className="flex h-full overflow-hidden">
      <Sidebar />
      <main className="min-w-0 flex-1 overflow-y-auto">
        <Updater />
        {demo && (
          <div
            className="flex items-center justify-between gap-3 border-b border-warn/30 bg-warn/10 px-6 py-2 text-xs text-warn"
            role="status"
          >
            <span>{t("demo.banner")}</span>
            <button
              className="shrink-0 rounded-lg border border-warn/40 px-2.5 py-1 font-medium hover:bg-warn/15"
              onClick={exitDemo}
            >
              {t("demo.exit")}
            </button>
          </div>
        )}
        <div className="sticky top-0 z-20 border-b border-line bg-bg">
          <div className="mx-auto max-w-5xl px-6 md:px-10">
            <StatusBar action={page !== "dashboard"} />
          </div>
        </div>
        <div
          key={page}
          className="page-in mx-auto max-w-5xl px-6 py-8 md:px-10 md:py-10"
        >
          {children}
        </div>
      </main>
      <Toasts />
      <Guide />
    </div>
  );
}

export function PageTitle({
  children,
  action,
  sub,
}: {
  children: ReactNode;
  action?: ReactNode;
  sub?: ReactNode;
}) {
  return (
    <div className="mb-8 flex flex-wrap items-end justify-between gap-3">
      <div className="min-w-0">
        <h1 className="font-display text-2xl tracking-tight">{children}</h1>
        {sub && (
          <p className="mt-1.5 text-sm leading-relaxed text-muted">{sub}</p>
        )}
      </div>
      {action}
    </div>
  );
}
