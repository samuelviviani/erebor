import type { Alert } from "./api";
import { translate, translateError } from "./i18n";

type Lang = "it" | "en";

/** Stops the screen already explains (the person pressed Stop, a setting restarted the miner…). */
const QUIET_STOPS = new Set([
  "user",
  "config_changed",
  "mode_changed",
  "app_exit",
  "agent_exit",
  "data_deleted",
  "overheat", // followed by its own "stopped_overheat"
]);

/** Alerts whose detail is a raw error from the miner or the system: shown only if translated. */
const ERROR_DETAIL = new Set([
  "miner_crashed",
  "restart_failed",
  "schedule_start_failed",
  "miner_update_failed",
  "miner_api_unreachable",
  "switch_failed_restarting",
  "storage_error",
]);

/**
 * What an engine alert says to a person, in one plain sentence, or `null` when the
 * screen already shows it (mining started, stopped by hand, restarted for a new setting).
 * Codes and English details from the engine never reach the toast as they are.
 */
export function alertText(lang: Lang, a: Pick<Alert, "code" | "detail">): string | null {
  const d = a.detail ?? "";
  if (a.code === "mining_started") return null;
  if (a.code === "mining_stopped") {
    if (QUIET_STOPS.has(d)) return null;
    return translate(lang, d === "schedule" ? "alert.mining_stopped.schedule" : "alert.mining_stopped");
  }
  const key = `alert.${a.code}`;
  const base = translate(lang, key);
  // Unknown to this version of the app: still say something readable.
  if (base === key) return a.code.replaceAll("_", " ");
  if (ERROR_DETAIL.has(a.code)) {
    const e = translateError(lang, d);
    return d && e !== d ? `${base}: ${e}` : base;
  }
  return translate(lang, key, {
    d: a.code === "tune_done" && d === "stock" ? translate(lang, "alert.tune_done.stock") : d,
    // "40% of normal" → 40
    p: /(\d+)\s*%/.exec(d)?.[1] ?? "",
  });
}
