import { Pause, Play, Square } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { usePresetChange } from "../components/CoinChange";
import { CoinAdvice } from "../components/Advice";
import { ComputerMonitor } from "../components/ComputerMonitor";
import { ModeSelector } from "../components/ModeSelector";
import { payoutMode } from "../components/PayoutChoice";
import { Explain, Slider, type HelpTerm } from "../components/ui";
import { api, errorText } from "../lib/api";
import { btcFull, coinAmount, money } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

const MONTH_DAYS = 30;
/**
 * Home: what people want to know (how much they earn per month and per day) and what
 * they change often (start/stop, coin, performance mode). Details stay plain text.
 */
export default function Dashboard() {
  const t = useT();
  const { status, config, toast, refreshStatus, saveConfig, setPage } =
    useApp();
  const [busy, setBusy] = useState(false);
  const setPreset = usePresetChange();
  const lang = config?.language ?? "it";
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

          {/* One obvious action: start; while mining, stop and pause side by side. */}
          <div className="mt-6 flex flex-wrap items-center gap-3">
            {!active ? (
              <button
                type="button"
                disabled={busy || !status || !!status?.startBlocker}
                onClick={() => act(api.startMining)}
                className="inline-flex h-12 items-center gap-2.5 rounded-full bg-go/12 px-6 text-[15px] font-semibold text-go transition-colors hover:bg-go/20 active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-50"
              >
                <Play className="size-[18px] fill-current" />
                {t("dash.start.long")}
              </button>
            ) : (
              <>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => act(api.stopMining)}
                  className="inline-flex h-12 items-center gap-2.5 rounded-full bg-stop/10 px-6 text-[15px] font-semibold text-stop transition-colors hover:bg-stop/15 active:scale-[0.98] disabled:opacity-50"
                >
                  <Square className="size-4 fill-current" />
                  {t("dash.stop")}
                </button>
                {status?.miner?.supportsPause !== false && (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      act(
                        state === "paused" ? api.resumeMining : api.pauseMining,
                      )
                    }
                    className={
                      state === "paused"
                        ? "inline-flex h-12 items-center gap-2.5 rounded-full bg-go/12 px-6 text-[15px] font-semibold text-go transition-colors hover:bg-go/20 active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-50"
                        : "inline-flex h-12 items-center gap-2.5 rounded-full bg-surface-2 px-6 text-[15px] font-semibold text-fg transition-colors hover:bg-line active:scale-[0.98] disabled:opacity-50"
                    }
                  >
                    {state === "paused" ? (
                      <Play className="size-4 fill-current" />
                    ) : (
                      <Pause className="size-4 fill-current" />
                    )}
                    {state === "paused" ? t("dash.resume") : t("dash.pause")}
                  </button>
                )}
              </>
            )}
            {!active && status?.startBlocker && (
              <p className="text-xs text-faint" role="status">
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
  children,
}: {
  label: string;
  help: HelpTerm;
  sub?: string;
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
