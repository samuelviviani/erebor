import {
  BarChart3,
  CircleCheck,
  CircleX,
  Coins,
  FlaskConical,
  Home,
  PanelLeftClose,
  PanelLeftOpen,
  Settings as SettingsIcon,
  TriangleAlert,
  X,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { useT } from "../lib/i18n";
import { useApp, useResolvedTheme, type Page } from "../lib/store";
import { Guide } from "./Guide";
import { Updater } from "./Updater";
import { StatusBar } from "./MinerStatus";
import { cx } from "./ui";

const NAV: { page: Page; icon: LucideIcon }[] = [
  { page: "dashboard", icon: Home },
  { page: "coins", icon: Coins },
  { page: "stats", icon: BarChart3 },
  { page: "settings", icon: SettingsIcon },
];
/** Height + gap of one nav item, for the sliding highlight. */
const STEP = 44;

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
  const idx = NAV.findIndex((n) => n.page === page);

  return (
    <nav
      aria-label="Erebor"
      className={cx(
        "relative z-10 flex shrink-0 flex-col border-r border-line bg-sidebar backdrop-blur-2xl transition-[width] duration-300 ease-[var(--ease)]",
        collapsed ? "w-[72px]" : "w-56",
      )}
    >
      <div
        className={cx(
          "flex h-[68px] items-center gap-2.5 px-4",
          collapsed && "justify-center px-0",
        )}
      >
        <Logo className="h-8 w-auto shrink-0" />
        {!collapsed && (
          <>
            <span className="min-w-0">
              <span className="block text-[15px] leading-tight font-semibold tracking-wide">
                Erebor
              </span>
              <span className="num block text-[10.5px] leading-tight text-faint">
                v{info?.version ?? "—"}
              </span>
            </span>
            <button
              type="button"
              onClick={toggle}
              aria-label={t("nav.collapse")}
              title={t("nav.collapse")}
              className="ml-auto rounded-lg p-1.5 text-faint transition-colors hover:bg-surface-2 hover:text-fg"
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
          className="mx-auto mb-2 rounded-lg p-1.5 text-faint transition-colors hover:bg-surface-2 hover:text-fg"
        >
          <PanelLeftOpen className="size-4" />
        </button>
      )}

      <ul className="relative mt-2 space-y-1 px-3">
        {idx >= 0 && (
          <li
            aria-hidden
            className="pointer-events-none absolute inset-x-3 top-0 h-10 rounded-xl border border-line bg-surface-2 shadow-[var(--highlight)] transition-transform duration-400 ease-[var(--ease)]"
            style={{ transform: `translateY(${idx * STEP}px)` }}
          />
        )}
        {NAV.map(({ page: p, icon: Icon }) => {
          const on = page === p;
          return (
            <li key={p} className="relative">
              <button
                type="button"
                onClick={() => setPage(p)}
                aria-current={on ? "page" : undefined}
                title={collapsed ? t(`nav.${p}`) : undefined}
                className={cx(
                  "flex h-10 w-full items-center gap-3 rounded-xl px-3 text-[13.5px] transition-colors duration-200",
                  collapsed && "justify-center px-0",
                  on ? "font-medium text-fg" : "text-muted hover:text-fg",
                )}
              >
                <Icon
                  className={cx(
                    "size-[18px] shrink-0 transition-colors duration-300",
                    on && "text-accent",
                  )}
                  aria-hidden
                />
                {!collapsed && <span className="truncate">{t(`nav.${p}`)}</span>}
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

const TOAST_ICON = {
  info: <CircleCheck className="size-4 text-accent" />,
  warning: <TriangleAlert className="size-4 text-warn" />,
  error: <CircleX className="size-4 text-danger" />,
};
const TOAST_BAR = { info: "bg-accent", warning: "bg-warn", error: "bg-danger" };

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
      className="fixed right-5 bottom-5 z-40 w-[340px] max-w-[calc(100vw-32px)]"
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
                      transform: `translateY(${-10 * Math.min(i, 2)}px) scale(${1 - 0.05 * Math.min(i, 2)})`,
                      transformOrigin: "top",
                      zIndex: -i,
                      opacity: i > 2 ? 0 : 1,
                    }
                  : undefined
              }
              className={cx(
                "toast-in glass relative flex items-start gap-3 overflow-hidden rounded-2xl py-3 pr-3 pl-4 text-[13px] leading-snug transition-[transform,opacity] duration-300 ease-[var(--ease)]",
                behind && "absolute inset-0",
              )}
            >
              <span
                aria-hidden
                className={cx("absolute inset-y-0 left-0 w-[3px]", TOAST_BAR[x.level])}
              />
              <span className={cx("mt-px shrink-0", behind && "invisible")}>
                {TOAST_ICON[x.level]}
              </span>
              <span className={cx("min-w-0 flex-1", behind && "invisible")}>
                {x.text}
                {x.count > 1 && (
                  <span className="num ml-1.5 rounded-full bg-surface-3 px-1.5 py-px text-[11px] text-muted">
                    ×{x.count}
                  </span>
                )}
              </span>
              {!open && i === 0 && list.length > 1 && (
                <span className="num shrink-0 rounded-full bg-surface-3 px-1.5 text-[11px] text-faint">
                  +{list.length - 1}
                </span>
              )}
              <button
                type="button"
                tabIndex={behind ? -1 : 0}
                aria-label={t("close")}
                onClick={() => dismissToast(x.id)}
                className={cx(
                  "-my-0.5 shrink-0 rounded-full p-1 text-faint transition-colors hover:bg-surface-3 hover:text-fg",
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
        <div className="sticky top-0 z-20 border-b border-line bg-bg/70 backdrop-blur-xl">
          <div className="mx-auto flex max-w-5xl items-center gap-3 px-6 md:px-10">
            <div className="min-w-0 flex-1">
              <StatusBar action={page !== "dashboard"} />
            </div>
            {demo && (
              <div
                className="flex shrink-0 items-center gap-1 rounded-full border border-warn/30 bg-warn/8 py-0.5 pr-0.5 pl-2.5 text-xs text-warn"
                role="status"
                title={t("demo.banner")}
              >
                <FlaskConical className="size-3.5" aria-hidden />
                <span className="hidden lg:inline">{t("demo.banner")}</span>
                <button
                  className="ml-1 rounded-full bg-warn/12 px-2.5 py-1 font-medium transition-colors hover:bg-warn/20"
                  onClick={exitDemo}
                >
                  {t("demo.exit")}
                </button>
              </div>
            )}
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
    <div className="mb-8 flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        <h1 className="font-display text-[28px] leading-tight">{children}</h1>
        {sub && (
          <p className="mt-2 max-w-xl text-sm leading-relaxed text-muted">{sub}</p>
        )}
      </div>
      {action}
    </div>
  );
}
