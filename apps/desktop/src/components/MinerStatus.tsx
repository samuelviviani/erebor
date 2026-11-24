import { Loader2, Play, Square } from "lucide-react";
import { useState } from "react";
import { api, errorText } from "../lib/api";
import { hashrate } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { CoinIcon } from "./CoinIcon";
import { Button, cx } from "./ui";

const DOT = { running: "bg-go", paused: "bg-hold", error: "bg-stop", stopped: "bg-faint" } as const;

/**
 * What the miner is doing right now, in a few words, including the moments in between
 * (coin change, start-up, connecting) so the state never looks blank.
 */
function useMinerPhase(): { state: keyof typeof DOT; text: string; busy: boolean } {
  const t = useT();
  const { status, coinBusy, config, restartUntil } = useApp();
  const state = status?.state ?? "stopped";
  const s = status?.stats;
  const h = s?.hashrate10s ?? s?.hashrate60s ?? null;
  if (coinBusy) return { state, busy: true, text: t("phase.switching", { c: coinBusy }) };
  if (status?.restartInSecs != null) return { state, busy: true, text: t("dash.restartIn", { s: status.restartInSecs }) };
  if (state === "running" && Date.now() < restartUntil && (!s?.connected || h == null)) return { state, busy: true, text: t("phase.restarting") };
  if (state === "running") {
    if (!s) return { state, busy: true, text: t("phase.starting") };
    if (!s.connected) return { state, busy: true, text: t("phase.connecting") };
    if (h == null) return { state, busy: true, text: t("phase.warmup") };
    return { state, busy: false, text: hashrate(h) };
  }
  if (state === "paused") return { state, busy: false, text: t("state.paused") };
  if (state === "error") return { state, busy: false, text: status?.error ?? t("state.error") };
  if (status?.held) return { state: "paused", busy: false, text: t(`held.${status.held}`) };
  if (status?.startBlocker) return { state, busy: false, text: translateError(config?.language ?? "en", status.startBlocker) };
  return { state, busy: false, text: t("phase.ready") };
}

/** Coloured dot that pulses only while something is in progress (starting, switching…). */
function StateDot({ state, busy }: { state: keyof typeof DOT; busy?: boolean }) {
  return (
    <span className="relative flex size-2.5 shrink-0">
      {busy && <span className={cx("absolute inline-flex size-full animate-ping rounded-full opacity-50", DOT[state])} />}
      <span className={cx("relative inline-flex size-2.5 rounded-full shadow-[0_0_10px_currentColor]", DOT[state], state === "running" ? "text-go" : "text-transparent")} />
    </span>
  );
}

/**
 * Always-visible miner status (top of every page and of Settings): state, coin, what it
 * is doing right now (restarting, connecting…) and a start/stop button.
 */
export function StatusBar({ action = true }: { action?: boolean }) {
  const t = useT();
  const { status, config, toast, refreshStatus } = useApp();
  const phase = useMinerPhase();
  const [busy, setBusy] = useState(false);
  const state = status?.state ?? "stopped";
  const active = state === "running" || state === "paused";
  const toggle = async () => {
    setBusy(true);
    try {
      await (active ? api.stopMining() : api.startMining());
    } catch (e) {
      toast("error", translateError(config?.language ?? "en", errorText(e)));
    } finally {
      await refreshStatus();
      setBusy(false);
    }
  };
  return (
    <div role="status" aria-live="polite" aria-label={t("status.label")} title={t(`state.${phase.state}`)} className="flex min-h-14 min-w-0 items-center gap-3 text-[13px]">
      <StateDot state={phase.state} busy={phase.busy} />
      <span className="sr-only">{t(`state.${phase.state}`)}</span>
      {status && <CoinIcon id={status.coin.id} size={20} className="ring-2 ring-surface-2" />}
      <span className="min-w-0 flex-1 truncate">
        {status && <span className="font-semibold">{status.coin.name}</span>}
        <span className={cx("num ml-2", phase.busy ? "text-accent" : phase.state === "error" ? "text-danger" : "text-muted")}>
          {phase.busy && <Loader2 className="mr-1 inline size-3.5 animate-spin" aria-hidden />}
          {phase.text}
        </span>
      </span>
      {action && status && (
        <Button
          size="sm"
          variant={active ? "stop" : "go"}
          onClick={toggle}
          loading={busy}
          disabled={!active && !!status.startBlocker}
          className="[&>svg]:fill-current"
        >
          {!busy && (active ? <Square /> : <Play />)}
          {active ? t("dash.stop") : t("dash.start")}
        </Button>
      )}
    </div>
  );
}
