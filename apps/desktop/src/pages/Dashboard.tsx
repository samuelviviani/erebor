import { Power } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { usePresetChange } from "../components/CoinChange";
import { CoinAdvice } from "../components/Advice";
import { RealBalanceLink } from "../components/shared";
import { ComputerMonitor } from "../components/ComputerMonitor";
import { ModeSelector } from "../components/ModeSelector";
import { payoutMode } from "../components/PayoutChoice";
import { cx, Explain, Slider, type HelpTerm } from "../components/ui";
import { api, errorText } from "../lib/api";
import { btcFull, coinAmount, hashrate, money } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

const MONTH_DAYS = 30;
/**
 * Home: what people want to know (how much they earn per month and per day) and what
 * they change often (start/stop, coin, performance mode). Details stay plain text.
 */
/**
 * Start/stop as a power dial: a round key with a power glyph and the action beside it.
 * Idle: a quiet ring that lights up on hover. Mining: the ring turns (motion-safe) and the
 * line under the action shows the live speed; hovering the dial turns it to "stop".
 */
function PowerDial({
  state,
  busy,
  disabled,
  hashrate: h,
  onClick,
}: {
  state: string;
  busy: boolean;
  disabled: boolean;
  hashrate: number | null;
  onClick: () => void;
}) {
  const t = useT();
  const running = state === "running";
  const paused = state === "paused";
  const action = running
    ? t("dash.stop.long")
    : paused
      ? t("dash.resume")
      : t("dash.start.long");
  const sub = running
    ? h
      ? `${t("dash.running")} · ${hashrate(h)}`
      : t("dash.running")
    : paused
      ? t("dash.paused")
      : t("dash.idle");
  return (
    <button
      type="button"
      aria-label={action}
      onClick={onClick}
      disabled={busy || disabled}
      className="group flex items-center gap-4 rounded-full pr-2 text-left disabled:cursor-not-allowed disabled:opacity-50"
    >
      <span className="relative flex size-16 shrink-0 items-center justify-center rounded-full transition-transform duration-150 group-active:scale-95">
        {/* The ring: a turning arc while mining, a still outline otherwise. */}
        <span
          aria-hidden
          className={cx(
            "absolute inset-0 rounded-full",
            running
              ? "bg-[conic-gradient(from_0deg,transparent_0deg,var(--go)_280deg,transparent_360deg)] motion-safe:animate-[spin_2.4s_linear_infinite] group-hover:bg-[conic-gradient(var(--stop),var(--stop))]"
              : busy
                ? "bg-[conic-gradient(from_0deg,transparent_0deg,var(--fg)_200deg,transparent_360deg)] opacity-40 motion-safe:animate-spin"
                : paused
                  ? "bg-warn/70"
                  : "bg-line group-hover:bg-go group-disabled:bg-line",
          )}
        />
        <span
          className={cx(
            "absolute inset-[3px] flex items-center justify-center rounded-full bg-surface transition-colors",
            running
              ? "text-go shadow-[0_0_28px_-6px_var(--go)] group-hover:text-stop group-hover:shadow-[0_0_28px_-6px_var(--stop)]"
              : paused
                ? "text-warn"
                : "text-muted group-hover:text-go group-disabled:text-faint",
          )}
        >
          <Power className="size-6" strokeWidth={2.25} />
        </span>
      </span>
      <span className="min-w-0">
        <span className="block text-[15px] font-semibold">{action}</span>
        <span className="num mt-0.5 block text-xs text-faint">{sub}</span>
      </span>
    </button>
  );
}

export default function Dashboard() {
  const t = useT();
  const { status, config, toast, refreshStatus, saveConfig, setPage } =
    useApp();
  const [busy, setBusy] = useState(false);
  const setPreset = usePresetChange();
  const lang = config?.language ?? "en";
  const locale = lang === "it" ? "it-IT" : "en-US";
  const currency = config?.energy.currency ?? "EUR";
  const state = status?.state ?? "stopped";
  const active = state === "running" || state === "paused";
  const coin = status?.coin;
  const p = status?.projection;
  const fmt = (v: number) => money(v, currency, locale);
  const payout = payoutMode(config?.wallet ?? "");

  const act = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    } finally {
      await refreshStatus();
      setBusy(false);
    }
  };

  // Kryptex and unMineable pay out in Bitcoin: show the balance in BTC, plainly.
  const paysBtc =
    !!config?.wallet && (payout === "kryptex" || payout === "bitcoin");
  const btcPrice =
    status?.market.btc?.[currency === "USD" ? "usd" : "eur"] ?? null;
  const perDay = p?.netPerDay ?? p?.valuePerDay ?? null;
  const preset = config?.performance.preset ?? "balanced";
  const month = useCountUp(perDay != null ? perDay * MONTH_DAYS : null);
  const dayValue = useCountUp(perDay);

  return (
    <div className="space-y-10">
      {/* Headline, the controls right under it, then the two figures (no boxes) */}
      <div>
        <div className="min-w-0">
          <h1
            className="font-display text-3xl tracking-tight"
            aria-live="polite"
          >
            {t(`home.title.${state}`, { c: coin?.name ?? "" })}
          </h1>
          <p className="mt-3 max-w-lg text-sm leading-relaxed text-muted">
            {state === "error" && status?.error
              ? status.error
              : t(`home.sub.${state === "error" ? "stopped" : state}`)}
          </p>

          {/* One control: a power dial. Idle it waits; while mining its ring turns
              like an engine. No pause: stopping is just as quick (a run paused from
              the terminal shows "Resume", with stop beside it). */}
          <div className="mt-6 flex flex-wrap items-center gap-x-5 gap-y-3">
            <PowerDial
              state={state}
              busy={busy}
              disabled={!status || (!active && !!status?.startBlocker)}
              hashrate={status?.stats?.hashrate10s ?? null}
              onClick={() =>
                act(
                  state === "paused"
                    ? api.resumeMining
                    : active
                      ? api.stopMining
                      : api.startMining,
                )
              }
            />
            {state === "paused" && (
              <button
                type="button"
                disabled={busy}
                onClick={() => act(api.stopMining)}
                className="text-sm text-stop hover:underline disabled:opacity-50"
              >
                {t("dash.stop")}
              </button>
            )}
            {!active && status?.startBlocker && (
              <p className="max-w-xs text-xs text-faint" role="status">
                {translateError(lang, status.startBlocker)}
              </p>
            )}
          </div>

          {/* Two figures, nothing else: what it will earn, and what it has earned. */}
          <dl className="mt-8 flex flex-wrap gap-x-12 gap-y-6">
            <Figure
              label={t("home.month.short")}
              help="estimate"
              sub={
                perDay != null
                  ? `≈ ${fmt(dayValue ?? perDay)} ${t("home.perDay")}${p?.energyCostPerDay != null ? ` · ${t("home.afterPower.short")}` : ""}`
                  : t(
                      active
                        ? "home.noEstimate.running.short"
                        : "home.noEstimate.short",
                    )
              }
            >
              {perDay != null ? `≈ ${fmt(month ?? perDay * MONTH_DAYS)}` : "—"}
            </Figure>
            {paysBtc ? (
              <Figure
                label={t(
                  payout === "kryptex"
                    ? "home.btc.kryptex.short"
                    : "home.btc.unmineable.short",
                )}
                help="btc"
                sub={
                  status?.minedBtc != null && btcPrice != null
                    ? `≈ ${fmt(status.minedBtc * btcPrice)}`
                    : undefined
                }
                extra={payout === "kryptex" && <RealBalanceLink />}
              >
                {btcFull(status?.minedBtc ?? 0, locale)}
              </Figure>
            ) : (
              <Figure label={t("home.mined")} help="mined">
                {status?.mined.length
                  ? status.mined
                      .map((m) => coinAmount(m.amount, m.coin))
                      .join(" · ")
                  : "—"}
              </Figure>
            )}
          </dl>
        </div>
      </div>

      {/* Quick controls: coin and performance mode */}
      <div className="grid gap-8 border-t border-line pt-8 lg:grid-cols-2">
        <section aria-labelledby="q-coin">
          <h2
            id="q-coin"
            className="mb-3 flex items-center gap-1.5 text-sm font-medium"
          >
            {t("home.coin")}
            <Explain term={status?.autoCoin ? "auto" : "coin"} align="left" />
          </h2>
          <CoinAdvice onChange={() => setPage("coins")} />
        </section>

        <section aria-labelledby="q-mode">
          <h2
            id="q-mode"
            className="mb-3 flex items-center gap-1.5 text-sm font-medium"
          >
            {t("home.mode")}
            <Explain term="mode" align="left" />
          </h2>
          <ModeSelector
            labelledBy="q-mode"
            withCustom
            value={preset}
            customPercent={config?.performance.customThreadsPercent ?? 50}
            onChange={setPreset}
          />
          <p className="mt-2.5 text-xs leading-relaxed text-faint">
            {preset === "custom"
              ? t("hw.mode.custom.desc")
              : t(`home.mode.${preset}.desc`)}
            {coin?.device === "gpu" && ` ${t("home.mode.gpuNote")}`}
          </p>
          {preset === "custom" && config && (
            <div className="mt-4">
              <Slider
                label={t("hw.threads")}
                value={config.performance.customThreadsPercent}
                min={5}
                max={100}
                step={5}
                unit="%"
                onCommit={(v) =>
                  saveConfig({
                    ...config,
                    performance: {
                      ...config.performance,
                      customThreadsPercent: v,
                    },
                  }).catch((e) =>
                    toast("error", translateError(lang, String(e))),
                  )
                }
              />
            </div>
          )}
        </section>
      </div>

      {/* The whole computer, live */}
      <section aria-labelledby="q-pc" className="border-t border-line pt-8">
        <h2 id="q-pc" className="mb-1 text-sm font-medium">
          {t("home.computer")}
        </h2>
        <p className="mb-4 text-xs text-faint">{t("hw.live.sub")}</p>
        <ComputerMonitor />
      </section>
    </div>
  );
}

function Figure({
  label,
  help,
  sub,
  extra,
  children,
}: {
  label: string;
  help: HelpTerm;
  sub?: string;
  extra?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="min-w-0">
      <dt className="flex items-center gap-1.5 text-sm text-muted">
        {label}
        <Explain term={help} align="left" />
      </dt>
      <dd className="num mt-1 text-4xl font-semibold tracking-tight">
        {children}
      </dd>
      {sub && <dd className="num mt-1.5 text-sm text-faint">{sub}</dd>}
      {extra && <dd className="mt-1">{extra}</dd>}
    </div>
  );
}

/** Smoothly animates a number towards its new value (respects reduced motion). */
function useCountUp(target: number | null, ms = 700): number | null {
  const [value, setValue] = useState(target);
  const from = useRef(target);
  useEffect(() => {
    if (target == null) {
      setValue(null);
      from.current = null;
      return;
    }
    const start = from.current ?? target;
    const reduce =
      typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (reduce || start === target) {
      setValue(target);
      from.current = target;
      return;
    }
    let raf = 0;
    const t0 = performance.now();
    const step = (now: number) => {
      const k = Math.min(1, (now - t0) / ms);
      const eased = 1 - Math.pow(1 - k, 3);
      setValue(start + (target - start) * eased);
      if (k < 1) raf = requestAnimationFrame(step);
      else from.current = target;
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [target, ms]);
  return value;
}
