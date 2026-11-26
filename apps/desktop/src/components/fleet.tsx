import { Boxes, CircleCheck, CircleMinus, CircleX, Cpu, Gpu, HardDrive, Monitor, Server, Undo2 } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { fleet, type Capability, type Conn, type DeviceKind, type DeviceRow, type FleetHistory, type MiningState, type OpResult } from "../lib/api";
import { hashrate, money } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { Spark } from "./Spark";
import { Badge, Button, cx, Modal, Segmented } from "./ui";

export const KIND_ICON: Record<DeviceKind, typeof Cpu> = {
  local_pc: Monitor,
  remote_pc: Monitor,
  server: Server,
  gpu_rig: Gpu,
  cpu_rig: Cpu,
  asic: HardDrive,
};

/** Keep the device list fresh while a device page is open (every 3 s). */
export function useFleetPolling() {
  const refresh = useApp((s) => s.refreshFleet);
  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 3000);
    return () => clearInterval(id);
  }, [refresh]);
}

export function useLang() {
  return useApp((s) => s.config?.language ?? "en");
}

export function ConnBadge({ conn }: { conn: Conn }) {
  const t = useT();
  const tone = conn === "online" ? "ok" : conn === "connecting" ? "warn" : conn === "offline" ? "neutral" : "err";
  return <Badge tone={tone}>{t(`fleet.conn.${conn}`)}</Badge>;
}

export function MiningBadge({ mining }: { mining: MiningState }) {
  const t = useT();
  const tone = mining === "mining" ? "ok" : mining === "error" ? "err" : mining === "idle" ? "neutral" : "warn";
  return <Badge tone={tone}>{t(`fleet.mining.${mining}`)}</Badge>;
}

export function SimBadge() {
  const t = useT();
  return (
    <span title={t("fleet.simulated.hint")}>
      <Badge tone="warn">{t("fleet.simulated")}</Badge>
    </span>
  );
}

/** A button for an action a device may not support: disabled, with the reason as tooltip. */
export function ActionButton({ cap, children, ...rest }: { cap: Capability; children: ReactNode } & Parameters<typeof Button>[0]) {
  const lang = useLang();
  const why = !cap.supported && cap.reason ? translateError(lang, cap.reason) : undefined;
  return (
    <span title={why} className="inline-flex">
      <Button {...rest} disabled={!cap.supported || rest.disabled} aria-description={why}>
        {children}
      </Button>
    </span>
  );
}

const RESULT_ICON = {
  completed: <CircleCheck className="size-4 text-go" />,
  skipped: <CircleMinus className="size-4 text-faint" />,
  rolled_back: <Undo2 className="size-4 text-warn" />,
  failed: <CircleX className="size-4 text-danger" />,
};

/** What a bulk operation did, device by device. */
export function ResultsModal({ results, onClose }: { results: OpResult[] | null; onClose: () => void }) {
  const t = useT();
  const lang = useLang();
  if (!results) return null;
  const ok = results.filter((r) => r.state === "completed").length;
  return (
    <Modal open onClose={onClose} closeLabel={t("close")} title={t("fleet.results.title", { ok, n: results.length })} wide actions={<Button onClick={onClose}>{t("close")}</Button>}>
      <ul className="divide-y divide-line">
        {results.map((r) => (
          <li key={r.id} className="flex items-start gap-3 py-2.5">
            <span className="mt-0.5">{RESULT_ICON[r.state]}</span>
            <span className="min-w-0 flex-1">
              <span className="block font-medium text-fg">{r.name}</span>
              <span className="block text-xs text-faint">
                {t(`fleet.result.${r.state}`)}
                {r.error && ` · ${translateError(lang, r.error.split(":")[0])}${r.error.includes(":") ? ` (${r.error.split(":").slice(1).join(":")})` : ""}`}
              </span>
            </span>
          </li>
        ))}
      </ul>
    </Modal>
  );
}

/** "3 min ago" style age of a unix time. */
export function useAgo() {
  const t = useT();
  return (ts: number | null) => {
    if (!ts) return "—";
    const s = Math.max(0, Math.floor(Date.now() / 1000) - ts);
    if (s < 60) return t("fleet.ago.s", { n: s });
    if (s < 3600) return t("fleet.ago.m", { n: Math.floor(s / 60) });
    if (s < 86400) return t("fleet.ago.h", { n: Math.floor(s / 3600) });
    return t("fleet.ago.d", { n: Math.floor(s / 86400) });
  };
}

export function kindLabel(t: ReturnType<typeof useT>, k: DeviceKind) {
  return t(`fleet.kind.${k}`);
}

export const RANGES = [
  { secs: 3600, key: "1h" },
  { secs: 86400, key: "24h" },
  { secs: 604800, key: "7d" },
  { secs: 2592000, key: "30d" },
] as const;

/** Power, money and hashrate (one line per algorithm) over a chosen range. */
export function HistoryChart({ id }: { id: string | null }) {
  const t = useT();
  const lang = useLang();
  const [range, setRange] = useState<(typeof RANGES)[number]["key"]>("24h");
  const [h, setH] = useState<FleetHistory | null>(null);
  const secs = RANGES.find((r) => r.key === range)!.secs;
  useEffect(() => {
    let alive = true;
    const load = () => fleet.history(id, secs).then((x) => alive && setH(x)).catch(() => {});
    load();
    const i = setInterval(load, 60_000);
    return () => {
      alive = false;
      clearInterval(i);
    };
  }, [id, secs]);
  const pts = h?.points ?? [];
  const last = <T,>(f: (p: (typeof pts)[number]) => T | null) => [...pts].reverse().map(f).find((v) => v != null) ?? null;
  const algos = useMemo(() => Object.entries(h?.hashrate ?? {}), [h]);
  const currency = useApp((s) => s.fleet?.overview.money[0]?.currency ?? "EUR");
  return (
    <section className="panel p-5">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-base font-semibold text-fg">{t("fleet.analytics")}</h2>
        <Segmented size="sm" label={t("fleet.range")} value={range} onChange={setRange} options={RANGES.map((r) => ({ value: r.key, label: t(`fleet.range.${r.key}`) }))} />
      </div>
      {pts.length < 2 ? (
        <p className="py-6 text-center text-sm text-faint">{t("fleet.analytics.empty")}</p>
      ) : (
        <div className="grid gap-6 sm:grid-cols-2">
          <Series label={t("fleet.power")} value={last((p) => p.powerW) != null ? `${Math.round(last((p) => p.powerW)!)} W` : "—"} values={pts.map((p) => p.powerW)} tone="text-perf" />
          <Series label={t("fleet.revenueDay")} value={last((p) => p.revenueDay) != null ? money(last((p) => p.revenueDay)!, currency, lang) : "—"} values={pts.map((p) => p.revenueDay)} tone="text-go" />
          <Series label={t("fleet.costDay")} value={last((p) => p.costDay) != null ? money(last((p) => p.costDay)!, currency, lang) : "—"} values={pts.map((p) => p.costDay)} tone="text-stop" />
          <Series label={t("fleet.tempMax")} value={last((p) => p.tempMax) != null ? `${Math.round(last((p) => p.tempMax)!)} °C` : "—"} values={pts.map((p) => p.tempMax)} tone="text-warn" />
          {algos.map(([algo, s]) => (
            <Series key={algo} label={`${t("fleet.hashrate")} · ${algo}`} value={hashrate(s[s.length - 1]?.hashrate)} values={s.map((x) => x.hashrate)} tone="text-accent" />
          ))}
        </div>
      )}
      <p className="mt-4 text-xs text-faint">{t("fleet.estimates")}</p>
    </section>
  );
}

function Series({ label, value, values, tone }: { label: string; value: string; values: (number | null)[]; tone: string }) {
  return (
    <div className="min-w-0">
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate text-xs text-faint">{label}</span>
        <span className="num text-sm font-semibold">{value}</span>
      </div>
      <div className="mt-1.5">
        <Spark values={values} tone={tone} label={label} />
      </div>
    </div>
  );
}

/** Small figure with a label: the dashboard's summary tiles. */
export function Stat({ label, value, sub, tone }: { label: string; value: ReactNode; sub?: ReactNode; tone?: string }) {
  return (
    <div className="min-w-0 rounded-2xl border border-line bg-surface px-4 py-3 shadow-[var(--highlight)]">
      <div className="truncate text-xs text-faint">{label}</div>
      <div className={cx("num mt-1 truncate text-lg font-semibold tracking-tight", tone)}>{value}</div>
      {sub && <div className="num mt-0.5 truncate text-2xs text-faint">{sub}</div>}
    </div>
  );
}

export function DeviceIcon({ row, className }: { row: Pick<DeviceRow, "kind">; className?: string }) {
  const Icon = KIND_ICON[row.kind] ?? Boxes;
  return <Icon className={cx("shrink-0 text-faint", className)} aria-hidden />;
}
