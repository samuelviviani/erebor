import { Loader2, Play, Square } from "lucide-react";
import { useState } from "react";
import { api, errorText } from "../lib/api";
import { hashrate } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { CoinIcon } from "./CoinIcon";
import { cx } from "./ui";

export const DOT = { running: "bg-go", paused: "bg-hold", error: "bg-stop", stopped: "bg-faint" } as const;

/**
 * What the miner is doing right now, in a few words, including the moments in between
 * (coin change, start-up, connecting) so the state never looks blank.
 */
export function useMinerPhase(): { state: keyof typeof DOT; text: string; busy: boolean } {
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
  if (status?.startBlocker) return { state, busy: false, text: translateError(config?.language ?? "it", status.startBlocker) };
  return { state, busy: false, text: t("phase.ready") };
}

/** Coloured dot that pulses while something is in progress. */
export function StateDot({ state, busy }: { state: keyof typeof DOT; busy?: boolean }) {
  return (
    <span className="relative flex size-2 shrink-0">
      {(busy || state === "running") && <span className={cx("absolute inline-flex size-full animate-ping rounded-full opacity-60", DOT[state])} />}
      <span className={cx("relative inline-flex size-2 rounded-full", DOT[state])} />
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
      toast("error", translateError(config?.language ?? "it", errorText(e)));
    } finally {
      await refreshStatus();
      setBusy(false);
    }
  };
  return (
    <div role="status" aria-live="polite" aria-label={t("status.label")} title={t(`state.${phase.state}`)} className="flex min-h-12 min-w-0 items-center gap-2.5 text-[13px]">
      <StateDot state={phase.state} busy={phase.busy} />
      <span className="sr-only">{t(`state.${phase.state}`)}</span>
      {status && <CoinIcon id={status.coin.id} size={18} />}
      <span className="min-w-0 flex-1 truncate">
        {status && <span className="font-medium">{status.coin.name}</span>}
        <span className={cx("num ml-2", phase.busy ? "text-accent" : phase.state === "error" ? "text-danger" : "text-muted")}>
          {phase.busy && <Loader2 className="mr-1 inline size-3.5 animate-spin" aria-hidden />}
          {phase.text}
        </span>
      </span>
      {action && status && (
        <button
          type="button"
          onClick={toggle}
          disabled={busy || (!active && !!status.startBlocker)}
          className={cx(
            // Same family as the Home buttons: soft green to start, soft red to stop.
            "flex h-8 shrink-0 items-center gap-1.5 rounded-full px-3.5 text-xs font-semibold transition-colors active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-50 [&>svg]:fill-current",
            active ? "bg-stop/10 text-stop hover:bg-stop/15" : "bg-go/12 text-go hover:bg-go/20",
          )}
        >
          {active ? <Square className="size-3.5" /> : <Play className="size-3.5" />}
          {active ? t("dash.stop") : t("dash.start")}
        </button>
      )}
    </div>
  );
}
