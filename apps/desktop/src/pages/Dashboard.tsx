import { Clock, Power } from "lucide-react";
import { useState, type ReactNode } from "react";
import { usePresetChange } from "../components/CoinChange";
import { CoinAdvice } from "../components/Advice";
import { RealBalanceLink } from "../components/shared";
import { ComputerMonitor } from "../components/ComputerMonitor";
import { ModeSelector } from "../components/ModeSelector";
import { payoutMode } from "../components/PayoutChoice";
import { cx, Explain, Ticker, type HelpTerm } from "../components/ui";
import { api, errorText } from "../lib/api";
import { coinAmount, hashrate } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

const MONTH_DAYS = 30;

/**
 * Start/stop as a power key: a round button with a power glyph and the action beside it.
 * Idle: still, a quiet ring that turns green on hover. Mining: a slow arc turns around it
 * (motion-safe) and the line under the action shows the live speed; hovering turns it red.
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
  // On but waiting (PC in use, or not paying now): looks paused, the action is stop.
  const held = state === "held";
  const paused = state === "paused" || held;
  const action =
    running || held
      ? t("dash.stop.long")
      : paused
        ? t("dash.resume")
        : t("dash.start.long");
  const sub = running
    ? h
      ? `${t("dash.running")} · ${hashrate(h)}`
      : t("dash.running")
    : held
      ? t("dash.waiting")
      : paused
        ? t("dash.paused")
        : t("dash.idle");
  return (
    <button
      type="button"
      aria-label={action}
      onClick={onClick}
      disabled={busy || disabled}
      className="group flex items-center gap-5 rounded-full pr-3 text-left disabled:cursor-not-allowed disabled:opacity-50"
    >
      <span className="relative flex size-[84px] shrink-0 items-center justify-center rounded-full">
        {/* The ring: a turning arc while mining, a still outline otherwise. */}
        <span
          aria-hidden
          className={cx(
            "absolute inset-0 rounded-full transition-colors duration-300",
            running
              ? "bg-[conic-gradient(from_0deg,transparent_0deg,var(--go)_300deg,transparent_360deg)] motion-safe:animate-[spin_4s_linear_infinite] group-hover:bg-[conic-gradient(var(--stop),var(--stop))]"
              : busy
                ? "bg-[conic-gradient(from_0deg,transparent_0deg,var(--fg)_200deg,transparent_360deg)] opacity-40 motion-safe:animate-spin"
                : paused
                  ? "bg-warn/70"
                  : "bg-line-strong group-hover:bg-go group-disabled:bg-line",
          )}
        />
        <span
          className={cx(
            "absolute inset-[3px] flex items-center justify-center rounded-full bg-gradient-to-b from-surface-3 to-surface shadow-[var(--highlight),inset_0_-6px_14px_rgb(0_0_0/0.25)] transition-colors duration-200",
            running
              ? "text-go group-hover:text-stop"
              : paused
                ? "text-warn"
                : "text-muted group-hover:text-go group-disabled:text-faint",
          )}
        >
          <Power className="size-8" strokeWidth={2.25} />
        </span>
      </span>
      <span className="min-w-0">
        <span className="block text-[17px] font-semibold tracking-tight">{action}</span>
        <span className="num mt-1 block text-[13px] text-faint">{sub}</span>
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
  const held = status?.held ?? null;
  const active = state === "running" || state === "paused" || !!held;
  const coin = status?.coin;
  const p = status?.projection;
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
  const mined = status?.mined ?? [];

  const setCustom = (v: number) =>
    config &&
    saveConfig({
      ...config,
      performance: {
        ...config.performance,
        preset: "custom",
        customThreadsPercent: v,
      },
    }).catch((e) => toast("error", translateError(lang, String(e))));

  return (
    <div className="space-y-10">
      {/* Headline, the power key right under it, then the two figures (no boxes) */}
      <section>
        <h1 className="font-display text-[32px] leading-tight" aria-live="polite">
          {held
            ? t("home.title.held")
            : t(`home.title.${state}`, { c: coin?.name ?? "" })}
        </h1>
        <p className="mt-2.5 max-w-lg text-sm leading-relaxed text-muted">
          {held
            ? t(`home.sub.held.${held}`)
            : state === "error" && status?.error
              ? status.error
              : t(`home.sub.${state === "error" ? "stopped" : state}`)}
        </p>

        {/* One control: the power key. No pause: stopping is just as quick (a run
            paused from the terminal shows "Resume", with stop beside it). */}
        <div className="mt-7 flex flex-wrap items-center gap-x-5 gap-y-3">
          <PowerDial
            state={held ? "held" : state}
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
              className="text-sm font-medium text-stop hover:underline disabled:opacity-50"
            >
              {t("dash.stop")}
            </button>
          )}
          {state === "running" && (
            <button
              type="button"
              disabled={busy}
              onClick={() => act(() => api.stopFor(60))}
              className="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[13px] text-muted transition-colors hover:bg-surface-2 hover:text-fg disabled:opacity-50"
            >
              <Clock className="size-3.5" />
              {t("dash.stopFor")}
            </button>
          )}
          {status?.resumeInSecs != null && (
            <p className="text-xs text-faint" role="status">
              {t("dash.resumeIn", {
                m: Math.max(1, Math.ceil(status.resumeInSecs / 60)),
              })}
            </p>
          )}
          {!active && status?.startBlocker && (
            <p className="max-w-xs text-xs text-warn" role="status">
              {translateError(lang, status.startBlocker)}
            </p>
          )}
        </div>

        {/* Two figures, nothing else: what it will earn, and what it has earned. */}
        <dl className="mt-9 flex flex-wrap gap-x-14 gap-y-6">
          <Figure
            label={t("home.month.short")}
            help="estimate"
            sub={
              perDay != null ? (
                <>
                  ≈ <Ticker value={perDay} locale={locale} currency={currency} />{" "}
                  {t("home.perDay")}
                  {p?.energyCostPerDay != null &&
                    ` · ${t("home.afterPower.short")}`}
                </>
              ) : (
                t(
                  active
                    ? "home.noEstimate.running.short"
                    : "home.noEstimate.short",
                )
              )
            }
          >
            {perDay != null ? (
              <span>
                ≈{" "}
                <Ticker
                  value={perDay * MONTH_DAYS}
                  locale={locale}
                  currency={currency}
                />
              </span>
            ) : (
              "—"
            )}
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
                status?.minedBtc != null && btcPrice != null ? (
                  <>
                    ≈{" "}
                    <Ticker
                      value={status.minedBtc * btcPrice}
                      locale={locale}
                      currency={currency}
                    />
                  </>
                ) : undefined
              }
              extra={payout === "kryptex" && <RealBalanceLink />}
            >
              <Ticker
                value={status?.minedBtc ?? 0}
                locale={locale}
                decimals={8}
                suffix=" BTC"
              />
            </Figure>
          ) : (
            <Figure label={t("home.mined")} help="mined">
              {mined.length
                ? mined.map((m) => coinAmount(m.amount, m.coin)).join(" · ")
                : "—"}
            </Figure>
          )}
        </dl>
      </section>

      {/* Quick controls: coin and performance mode */}
      <div className="grid gap-8 border-t border-line pt-8 lg:grid-cols-2">
        <section aria-labelledby="q-coin">
          <h2 id="q-coin" className="eyebrow mb-3 flex items-center gap-1.5 px-1">
            {t("home.coin")}
            <Explain term={status?.autoCoin ? "auto" : "coin"} align="left" />
          </h2>
          <CoinAdvice onChange={() => setPage("coins")} />
        </section>

        <section aria-labelledby="q-mode">
          <h2 id="q-mode" className="eyebrow mb-3 flex items-center gap-1.5 px-1">
            {t("home.mode")}
            <Explain term="mode" align="left" />
          </h2>
          {coin?.device === "gpu" ? (
            // GPU miners run with the CPU disabled and no preset: a CPU share would be a lie.
            <p className="panel px-5 py-4 text-[13px] leading-relaxed text-muted">
              {t("home.mode.gpuNote")}
            </p>
          ) : (
            <>
              <ModeSelector
                labelledBy="q-mode"
                withCustom
                value={preset}
                customPercent={config?.performance.customThreadsPercent ?? 50}
                onChange={setPreset}
                onCustom={setCustom}
              />
              <p className="mt-3 px-1 text-xs leading-relaxed text-faint">
                {preset === "custom"
                  ? t("hw.mode.custom.desc")
                  : t(`home.mode.${preset}.desc`)}
              </p>
            </>
          )}
        </section>
      </div>

      {/* The whole computer, live */}
      <section aria-labelledby="q-pc" className="border-t border-line pt-8">
        <h2 id="q-pc" className="eyebrow mb-1 px-1">
          {t("home.computer")}
        </h2>
        <p className="mb-4 px-1 text-xs text-faint">{t("hw.live.sub")}</p>
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
  sub?: ReactNode;
  extra?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="min-w-0">
      <dt className="flex items-center gap-1.5 text-[13px] text-muted">
        {label}
        <Explain term={help} align="left" />
      </dt>
      <dd className="num mt-1.5 text-[40px] leading-none font-bold tracking-tight">
        {children}
      </dd>
      {sub && <dd className="num mt-2.5 text-[13px] text-faint">{sub}</dd>}
      {extra && <dd className="mt-1.5">{extra}</dd>}
    </div>
  );
}
