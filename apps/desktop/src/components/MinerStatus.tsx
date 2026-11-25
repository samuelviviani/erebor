import { Power } from "lucide-react";
import { useState } from "react";
import { api, errorText } from "../lib/api";
import { hashrate } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { CoinIcon } from "./CoinIcon";
import { cx } from "./ui";

type Phase = "running" | "paused" | "error" | "stopped";

/**
 * What the miner is doing right now, in a few words, including the moments in between
 * (coin change, start-up, connecting) so the state never looks blank.
 */
function useMinerPhase(): { state: Phase; text: string; busy: boolean } {
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


/**
 * The power key's face: a round button with a power glyph. Idle: a still ring that turns green
 * on hover. Mining: a slow arc turns around it (motion-safe); hovering turns it red. Needs a
 * `group` parent (the button). `sm` is the same key in the status bar.
 */
export function PowerKnob({
  running,
  paused,
  busy,
  size = "lg",
}: {
  running: boolean;
  paused: boolean;
  busy: boolean;
  size?: "lg" | "sm";
}) {
  const lg = size === "lg";
  return (
    <span
      className={cx(
        "relative flex shrink-0 items-center justify-center rounded-full",
        lg ? "size-[84px]" : "size-9",
      )}
    >
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
          "absolute flex items-center justify-center rounded-full bg-surface shadow-[0_1px_2px_rgb(0_0_0/0.12)] transition-colors duration-200",
          lg ? "inset-[3px]" : "inset-[2px]",
          running
            ? "text-go group-hover:text-stop"
            : paused
              ? "text-warn"
              : "text-muted group-hover:text-go group-disabled:text-faint",
        )}
      >
        <Power className={lg ? "size-8" : "size-4"} strokeWidth={2.25} />
      </span>
    </span>
  );
}

/**
 * Always-visible miner status (top of every page and of Settings): state, coin, what it
 * is doing right now (restarting, connecting…) and a start/stop button.
 */
/** `control`: show the small power key; off on Home, where the big one already is. */
export function StatusBar({ control = true }: { control?: boolean }) {
  const t = useT();
  const { status, config, toast, refreshStatus } = useApp();
  const phase = useMinerPhase();
  const [busy, setBusy] = useState(false);
  const state = status?.state ?? "stopped";
  // Waiting (PC in use, not paying now) is on: the key looks paused and its action is stop.
  const held = !!status?.held;
  const active = state === "running" || state === "paused" || held;
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
    <div className="flex min-h-14 min-w-0 items-center gap-3 text-sm">
      {/* The power key from Home, small: it shows the state and is the switch, on every page. */}
      {status && control && (
        <button
          type="button"
          onClick={toggle}
          disabled={busy || (!active && !!status.startBlocker)}
          aria-label={active ? t("dash.stop.long") : t("dash.start.long")}
          title={active ? t("dash.stop.long") : t("dash.start.long")}
          className="group press -ml-1 shrink-0 rounded-full disabled:opacity-50"
        >
          <PowerKnob
            running={state === "running"}
            paused={state === "paused" || held}
            busy={busy || phase.busy}
            size="sm"
          />
        </button>
      )}
      <div
        role="status"
        aria-live="polite"
        aria-label={t("status.label")}
        className="flex min-w-0 flex-1 items-center gap-2.5"
      >
        <span className="sr-only">{t(`state.${phase.state}`)}</span>
        {status && <CoinIcon id={status.coin.id} size={20} />}
        <span className="min-w-0 flex-1 truncate">
          {status && <span className="font-semibold">{status.coin.name}</span>}
          <span className={cx("num ml-2", phase.busy ? "text-accent" : phase.state === "error" ? "text-danger" : "text-muted")}>
            {phase.text}
          </span>
        </span>
      </div>
    </div>
  );
}
