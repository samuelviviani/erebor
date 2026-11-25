import { Activity, ChevronRight, Clock, Gauge, Thermometer, Zap } from "lucide-react";
import { useState, type ReactNode } from "react";
import { usePresetChange } from "../components/CoinChange";
import { CoinAdvice } from "../components/Advice";
import { RealBalanceLink } from "../components/shared";
import { CoinPicker } from "../components/CoinPicker";
import { Meter, useSeries } from "../components/Spark";
import { ModeSelector } from "../components/ModeSelector";
import { PowerKnob } from "../components/MinerStatus";
import { payoutMode } from "../components/PayoutChoice";
import { Explain, Ticker, type HelpTerm } from "../components/ui";
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
      <PowerKnob running={running} paused={paused} busy={busy} />
      <span className="min-w-0">
        <span className="block text-lg font-semibold tracking-tight">{action}</span>
        <span className="num mt-1 block text-sm text-faint">{sub}</span>
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
      {/* Headline (the coin in it is a menu), then the power key and the two figures on
          the left, the mode on the right: everything used daily in the first screen. */}
      <section>
        <h1 className="font-display flex flex-wrap items-center gap-x-2.5 text-2xl leading-tight">
          {held ? (
            t("home.title.held")
          ) : state === "running" || state === "stopped" ? (
            <>
              <span aria-live="polite">{t(`home.title.${state}.pre`)}</span>
              <CoinPicker />
            </>
          ) : (
            t(`home.title.${state}`, { c: coin?.name ?? "" })
          )}
        </h1>
        <p className="mt-2.5 max-w-lg text-sm leading-relaxed text-muted">
          {held
            ? t(`home.sub.held.${held}`)
            : state === "error" && status?.error
              ? status.error
              : t(`home.sub.${state === "error" ? "stopped" : state}`)}
        </p>

        <div className="mt-7 grid gap-x-12 gap-y-10 lg:grid-cols-[minmax(0,1fr)_minmax(0,24rem)]">
        <div className="min-w-0">
        {/* One control: the power key. No pause: stopping is just as quick (a run
            paused from the terminal shows "Resume", with stop beside it). */}
        <div className="flex flex-wrap items-center gap-x-5 gap-y-3">
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
              className="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-sm text-muted transition-colors hover:bg-surface-2 hover:text-fg disabled:opacity-50"
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
        {/* Two fixed columns: the balance always sits on the right, whatever its digits. */}
        <dl className="mt-9 grid grid-cols-2 gap-x-8">
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
        {/* Only when there is something to fix: a manual coin, a better payout. */}
        <div className="mt-6 max-w-xl empty:hidden">
          <CoinAdvice card={false} warnOnly />
        </div>
        </div>

        <section aria-labelledby="q-mode">
          <h2 id="q-mode" className="mb-3 flex items-center gap-1.5 px-1 text-sm font-semibold">
            {t("home.mode")}
            <Explain term="mode" align="left" />
          </h2>
          {coin?.device === "gpu" ? (
            // GPU miners run with the CPU disabled and no preset: a CPU share would be a lie.
            <p className="panel px-5 py-4 text-sm leading-relaxed text-muted">
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
      </section>

      {/* The machine in four numbers; the charts and the miners are one click away. */}
      <section aria-labelledby="q-pc" className="border-t border-line pt-8">
        <div className="mb-3 flex items-baseline justify-between gap-3 px-1">
          <h2 id="q-pc" className="text-sm font-semibold">
            {t("home.computer")}
          </h2>
          <button
            type="button"
            onClick={() => setPage("computer")}
            className="inline-flex items-center gap-0.5 text-xs text-accent hover:underline"
          >
            {t("nav.computer")}
            <ChevronRight className="size-3.5" aria-hidden />
          </button>
        </div>
        <MachineStrip onOpen={() => setPage("computer")} />
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
    <div className="@container min-w-0">
      <dt className="flex items-center gap-1.5 text-sm text-muted">
        {label}
        <Explain term={help} align="left" />
      </dt>
      {/* One line always: smaller digits in a narrow column instead of wrapping. */}
      <dd className="num mt-1.5 text-xl leading-none font-bold tracking-tight whitespace-nowrap overflow-hidden text-ellipsis @[13rem]:text-2xl @[16rem]:text-3xl">
        {children}
      </dd>
      {sub && <dd className="num mt-2.5 text-sm text-faint">{sub}</dd>}
      {extra && <dd className="mt-1.5">{extra}</dd>}
    </div>
  );
}

/**
 * The machine on Home: the same live charts as the Computer page, in the same order
 * (load, hottest sensor, draw, speed), open on the page; a click opens the Computer page.
 */
function MachineStrip({ onOpen }: { onOpen: () => void }) {
  const t = useT();
  const { status, config } = useApp();
  const series = useSeries();
  const hw = status?.hardware;
  const maxT = config?.safety.maxTempC ?? 85;
  const gpu = status?.coin.device === "gpu";
  const h = status?.stats?.hashrate10s ?? status?.stats?.hashrate60s ?? null;
  const hottest = (cpu: number | null | undefined, gpus: (number | null | undefined)[]) => {
    const all = [cpu, ...gpus].filter((v): v is number => v != null);
    return all.length ? Math.max(...all) : null;
  };
  const busiest = (cpu: number | null | undefined, gpus: (number | null | undefined)[]) => {
    const all = gpus.filter((v): v is number => v != null);
    return gpu && all.length ? Math.max(...all) : (cpu ?? null);
  };
  const hot = hottest(hw?.cpuTempC, (hw?.gpus ?? []).map((g) => g.tempC));
  const load = busiest(hw?.cpuUsage, (hw?.gpus ?? []).map((g) => g.utilPercent));
  const watts = status?.powerW ?? hw?.powerW ?? null;
  const tempTone = hot == null || hot < maxT - 10 ? "text-[#14b8a6]" : hot >= maxT ? "text-stop" : "text-hold";
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-label={t("nav.computer")}
      className="-mx-3 grid w-[calc(100%+1.5rem)] grid-cols-2 gap-x-8 gap-y-6 rounded-2xl px-3 py-2 text-left transition-colors hover:bg-surface-2/50 sm:grid-cols-4"
    >
      <Meter
        icon={<Activity />}
        label={t(gpu ? "home.kpi.loadGpu" : "home.kpi.load")}
        value={load != null ? `${load.toFixed(0)} %` : "—"}
        values={series((s) => busiest(s.cpu, s.gpus.map((g) => g.util)))}
        max={100}
        tone="text-accent"
      />
      <Meter
        icon={<Thermometer />}
        label={t("home.kpi.temp")}
        value={hot != null ? `${hot.toFixed(0)} °C` : "—"}
        values={series((s) => hottest(s.cpuTemp, s.gpus.map((g) => g.temp)))}
        max={Math.max(maxT, 100)}
        tone={tempTone}
        valueTone={tempTone === "text-[#14b8a6]" ? undefined : tempTone}
      />
      <Meter
        icon={<Zap />}
        label={t("home.kpi.power")}
        value={watts != null ? `${watts.toFixed(0)} W` : "—"}
        values={series((s) => s.power)}
        tone="text-perf"
      />
      <Meter
        icon={<Gauge />}
        label={t("home.kpi.speed")}
        value={h ? hashrate(h) : "—"}
        values={series((s) => s.hash)}
        tone="text-go"
      />
    </button>
  );
}
