import {
  CircleCheck,
  CircleX,
  Coins,
  Cpu,
  FlaskConical,
  Home,
  PanelLeftClose,
  PanelLeftOpen,
  Settings as SettingsIcon,
  TriangleAlert,
  X,
  Zap,
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
  { page: "energy", icon: Zap },
  { page: "computer", icon: Cpu },
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

/** macOS: the window has no title bar, the traffic lights sit in the sidebar. */
export const IS_MAC =
  typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);
const MOD = IS_MAC ? "⌘" : "Ctrl+";

function NavButton({
  p,
  icon: Icon,
  on,
  collapsed,
  hint,
  onClick,
}: {
  p: Page;
  icon: LucideIcon;
  on: boolean;
  collapsed: boolean;
  hint: string;
  onClick: () => void;
}) {
  const t = useT();
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={on ? "page" : undefined}
      aria-keyshortcuts={hint.replace("⌘", "Meta+")}
      title={collapsed ? `${t(`nav.${p}`)}  ${hint}` : hint}
      className={cx(
        "press group flex h-10 items-center gap-3 rounded-xl px-3 text-sm transition-colors duration-200",
        // Collapsed: a square icon button, not a stretched bar.
        collapsed ? "mx-auto w-10 justify-center px-0" : "w-full",
        on ? "font-medium text-fg" : "text-muted hover:bg-nav-active/50 hover:text-fg",
      )}
    >
      <Icon
        className={cx(
          "size-[18px] shrink-0 transition-colors duration-300",
          on ? "text-accent" : "text-faint group-hover:text-muted",
        )}
        aria-hidden
      />
      {!collapsed && <span className="truncate">{t(`nav.${p}`)}</span>}
    </button>
  );
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
        "relative z-10 flex shrink-0 flex-col border-r border-line-strong bg-sidebar transition-[width] duration-300 ease-[var(--ease)]",
        // macOS: the three window buttons (x 12 in tauri.macos.conf.json, ~61px wide)
        // sit centred, ~12px each side.
        collapsed ? (IS_MAC ? "w-[84px]" : "w-[72px]") : "w-56",
      )}
    >
      {/* Drag strip: room for the traffic lights on macOS, a handle to move the window. */}
      {IS_MAC && <div data-tauri-drag-region className="h-9 shrink-0" />}
      <div
        data-tauri-drag-region
        className={cx(
          "flex h-[60px] items-center gap-2.5 px-4",
          collapsed && "justify-center px-0",
        )}
      >
        <Logo className="pointer-events-none h-7 w-auto shrink-0" />
        {!collapsed && (
          <>
            <span className="pointer-events-none min-w-0">
              <span className="block text-base leading-tight font-semibold tracking-tight">
                Erebor
              </span>
              <span className="num block text-2xs leading-tight text-faint">
                v{info?.version ?? "—"}
              </span>
            </span>
          </>
        )}
      </div>

      <ul className="relative mt-2 space-y-1 px-3">
        {idx >= 0 && (
          <li
            aria-hidden
            className={cx(
              "pointer-events-none absolute top-0 h-10 rounded-xl border border-line bg-nav-active shadow-[0_1px_2px_rgb(0_0_0/0.08)] transition-transform duration-400 ease-[var(--ease)]",
              collapsed ? "left-1/2 -ml-5 w-10" : "inset-x-3",
            )}
            style={{ transform: `translateY(${idx * STEP}px)` }}
          />
        )}
        {NAV.map(({ page: p, icon }, i) => (
          <li key={p} className="relative">
            <NavButton
              p={p}
              icon={icon}
              on={page === p}
              collapsed={collapsed}
              hint={`${MOD}${i + 1}`}
              onClick={() => setPage(p)}
            />
          </li>
        ))}
      </ul>

      {/* Bottom: settings, and beside it the toggle, so collapsing never moves the sections. */}
      <div
        className={cx(
          "mt-auto flex gap-1 px-3 pb-3",
          collapsed && "flex-col-reverse",
        )}
      >
        <div
          className={cx(
            "min-w-0 rounded-xl border",
            collapsed ? "mx-auto w-10" : "flex-1",
            page === "settings"
              ? "border-line bg-nav-active shadow-[0_1px_2px_rgb(0_0_0/0.08)]"
              : "border-transparent",
          )}
        >
          <NavButton
            p="settings"
            icon={SettingsIcon}
            on={page === "settings"}
            collapsed={collapsed}
            hint={`${MOD},`}
            onClick={() => setPage("settings")}
          />
        </div>
        <button
          type="button"
          onClick={toggle}
          aria-label={t(collapsed ? "nav.expand" : "nav.collapse")}
          title={t(collapsed ? "nav.expand" : "nav.collapse")}
          className="press flex size-10 shrink-0 items-center justify-center self-center rounded-xl text-faint transition-colors hover:bg-nav-active/50 hover:text-fg"
        >
          {collapsed ? <PanelLeftOpen className="size-[18px]" /> : <PanelLeftClose className="size-[18px]" />}
        </button>
      </div>
    </nav>
  );
}

/** ⌘1…5 open the sections, ⌘, the settings (Ctrl on Windows and Linux). */
function useShortcuts() {
  const setPage = useApp((s) => s.setPage);
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (!(IS_MAC ? e.metaKey : e.ctrlKey) || e.altKey || e.shiftKey) return;
      const n = Number(e.key);
      const target =
        e.key === "," ? "settings" : n >= 1 && n <= NAV.length ? NAV[n - 1].page : null;
      if (!target) return;
      e.preventDefault();
      setPage(target);
    };
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, [setPage]);
}

const TOAST_ICON = {
  info: <CircleCheck className="size-4 text-accent" />,
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
                "toast-in float relative flex items-start gap-3 overflow-hidden rounded-2xl py-3 pr-3 pl-4 text-sm leading-snug transition-[transform,opacity] duration-300 ease-[var(--ease)]",
                behind && "absolute inset-0",
              )}
            >
              <span className={cx("mt-px shrink-0", behind && "invisible")}>
                {TOAST_ICON[x.level]}
              </span>
              <span className={cx("min-w-0 flex-1", behind && "invisible")}>
                {x.text}
                {x.count > 1 && (
                  <span className="num ml-1.5 rounded-full bg-surface-3 px-1.5 py-px text-2xs text-muted">
                    ×{x.count}
                  </span>
                )}
              </span>
              {!open && i === 0 && list.length > 1 && (
                <span className="num shrink-0 rounded-full bg-surface-3 px-1.5 text-2xs text-faint">
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
  useShortcuts();
  return (
    <div className="shell flex h-full overflow-hidden">
      <Sidebar />
      <main className="canvas min-w-0 flex-1 overflow-y-auto">
        <Updater />
        <div
          data-tauri-drag-region
          className="sticky top-0 z-20 border-b border-line bg-bg"
        >
          <div
            data-tauri-drag-region
            className="mx-auto flex max-w-5xl items-center gap-3 px-6 md:px-10"
          >
            <div className="min-w-0 flex-1">
              <StatusBar control={page !== "dashboard"} />
            </div>
            {demo && (
              // Short line; the full sentence is the tooltip and the accessible name.
              <div
                className="flex shrink-0 items-center gap-1 rounded-full border border-warn/30 bg-warn/8 py-0.5 pr-0.5 pl-2.5 text-xs font-medium text-warn"
                role="status"
                aria-label={t("demo.banner")}
                title={t("demo.banner")}
              >
                <FlaskConical className="size-3.5" aria-hidden />
                <span aria-hidden>{t("demo.short")}</span>
                <button
                  className="ml-1 rounded-full bg-warn/12 px-2.5 py-1 transition-colors hover:bg-warn/20"
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
        <h1 className="font-display text-2xl leading-tight">{children}</h1>
        {sub && (
          <p className="mt-2 max-w-xl text-sm leading-relaxed text-muted">{sub}</p>
        )}
      </div>
      {action}
    </div>
  );
}
