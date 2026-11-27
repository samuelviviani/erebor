import { Boxes, CircleCheck, CircleMinus, CircleX, Cpu, Gpu, HardDrive, Monitor, Server, Undo2 } from "lucide-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { fleet, type Capability, type Conn, type DeviceKind, type DeviceRow, type FleetHistory, type MiningState, type OpResult } from "../lib/api";
import { hashrate, money } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
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

export type Metric = { key: string; label: string; ts: number[]; values: (number | null)[]; fmt: (v: number) => string; now?: string };

export const watts = (w: number) => (Math.abs(w) >= 10_000 ? `${(w / 1000).toFixed(1)} kW` : `${Math.round(w)} W`);

/**
 * History as one chart driven by tabs: each tab is a figure (its current value) and picks
 * what the chart shows. `live` overrides a tab's figure with the current reading (keyed
 * power, revenue, cost, net, temp, or `hash:<algo>`); otherwise the last bucket is shown.
 */
export function HistoryChart({ id, live = {} }: { id: string | null; live?: Record<string, string | undefined> }) {
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
  const currency = useApp((s) => s.fleet?.overview.money[0]?.currency ?? "EUR");
  const metrics = useMemo((): Metric[] => {
    const pts = h?.points ?? [];
    const ts = pts.map((p) => p.ts);
    const eur = (v: number) => money(v, currency, lang);
    return [
      { key: "power", label: t("fleet.power"), ts, values: pts.map((p) => p.powerW), fmt: watts },
      { key: "net", label: t("fleet.netDay"), ts, values: pts.map((p) => (p.revenueDay != null && p.costDay != null ? p.revenueDay - p.costDay : null)), fmt: eur },
      { key: "revenue", label: t("fleet.revenueDay"), ts, values: pts.map((p) => p.revenueDay), fmt: eur },
      { key: "cost", label: t("fleet.costDay"), ts, values: pts.map((p) => p.costDay), fmt: eur },
      { key: "temp", label: t("fleet.tempMax"), ts, values: pts.map((p) => p.tempMax), fmt: (v: number) => `${Math.round(v)} °C` },
      ...Object.entries(h?.hashrate ?? {}).map(([algo, s]) => ({ key: `hash:${algo}`, label: `${t("fleet.hashrate")} · ${algo}`, ts: s.map((x) => x.ts), values: s.map((x) => x.hashrate), fmt: hashrate })),
    ].map((m) => {
      const last = [...m.values].reverse().find((v) => v != null);
      return { ...m, now: live[m.key] ?? (last != null ? m.fmt(last) : undefined) };
    });
  }, [h, currency, lang, t, live]);
  return (
    <ChartPanel
      metrics={metrics}
      label={t("fleet.analytics")}
      long={secs > 86_400}
      controls={<Segmented size="sm" label={t("fleet.range")} value={range} onChange={setRange} options={RANGES.map((r) => ({ value: r.key, label: t(`fleet.range.${r.key}`) }))} />}
      note={t("fleet.estimates")}
    />
  );
}

/**
 * Figures as tabs over one chart: each tab shows a figure, the chosen one is charted.
 * Tabs with no figure and no data are left out.
 */
export function ChartPanel({ metrics, label, long, controls, note }: { metrics: Metric[]; label: string; long: boolean; controls?: ReactNode; note?: ReactNode }) {
  const t = useT();
  const [pick, setPick] = useState(metrics[0]?.key);
  const shown = metrics.filter((m) => m.now != null || m.values.some((v) => v != null));
  const cur = shown.find((m) => m.key === pick) ?? shown[0];
  return (
    <section className="panel overflow-hidden">
      <div role="tablist" aria-label={label} className={cx("grid grid-cols-2 border-b border-line sm:grid-cols-3", shown.length > 4 ? "lg:grid-cols-6" : "lg:grid-cols-4")}>
        {shown.map((m) => (
          <button
            key={m.key}
            type="button"
            role="tab"
            aria-selected={m === cur}
            onClick={() => setPick(m.key)}
            className={cx("relative min-w-0 px-4 py-3 text-left transition-colors", m === cur ? "bg-surface-2/70" : "hover:bg-surface-2/40")}
          >
            <span className="block truncate text-xs text-faint">{m.label}</span>
            <span className="num mt-0.5 block truncate text-base font-semibold tracking-tight text-fg">{m.now ?? "—"}</span>
            {m === cur && <span aria-hidden className="absolute inset-x-4 bottom-0 h-0.5 rounded-full bg-accent" />}
          </button>
        ))}
      </div>
      <div className="p-5">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-sm font-semibold text-fg">{cur?.label ?? label}</h2>
          {controls}
        </div>
        {!cur || cur.values.filter((v) => v != null).length < 2 ? (
          <p className="flex h-[200px] items-center justify-center text-center text-sm text-faint">{t("fleet.analytics.empty")}</p>
        ) : (
          <TrendChart key={cur.key} ts={cur.ts} values={cur.values} fmt={cur.fmt} label={cur.label} long={long} />
        )}
        {note && <p className="mt-3 text-xs text-faint">{note}</p>}
      </div>
    </section>
  );
}

const CH = 200;
const PAD = { l: 56, r: 8, t: 8, b: 22 };

/** 1, 2 or 5 times a power of ten: round axis steps. */
export function niceStep(raw: number) {
  const p = 10 ** Math.floor(Math.log10(raw));
  const f = raw / p;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * p;
}

/**
 * Line over time with a light wash, round y ticks, time labels, and a crosshair that
 * snaps to the nearest reading (pointer or arrow keys). Plain SVG, measured width.
 */
function TrendChart({ ts, values, fmt, label, long }: { ts: number[]; values: (number | null)[]; fmt: (v: number) => string; label: string; long: boolean }) {
  const lang = useLang();
  const ref = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(640);
  const [hover, setHover] = useState<number | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(([e]) => setW(e.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const nums = values.filter((v): v is number => v != null);
  const step = niceStep((Math.max(...nums, 0) - Math.min(...nums, 0)) / 3 || 1);
  const top = Math.ceil(Math.max(...nums, 0) / step) * step || step;
  const bot = Math.floor(Math.min(...nums, 0) / step) * step;
  const ticks: number[] = [];
  for (let v = bot; v <= top + step / 2; v += step) ticks.push(v);
  const [t0, t1] = [ts[0], ts[ts.length - 1]];
  const x = (t: number) => PAD.l + ((t - t0) / (t1 - t0 || 1)) * (w - PAD.l - PAD.r);
  const y = (v: number) => PAD.t + (1 - (v - bot) / (top - bot)) * (CH - PAD.t - PAD.b);
  // Contiguous runs: a gap in the data is a gap in the line.
  const runs: [number, number][][] = [[]];
  values.forEach((v, i) => (v == null ? runs[runs.length - 1].length && runs.push([]) : runs[runs.length - 1].push([x(ts[i]), y(v)])));
  const line = runs.filter((r) => r.length).map((r) => `M${r.map((p) => p.join(",")).join("L")}`).join("");
  const base = y(Math.max(bot, 0));
  const area = runs.filter((r) => r.length > 1).map((r) => `M${r[0][0]},${base}L${r.map((p) => p.join(",")).join("L")}L${r[r.length - 1][0]},${base}Z`).join("");
  const when = (s: number, o: Intl.DateTimeFormatOptions) => new Date(s * 1000).toLocaleString(lang, o);
  const xFmt: Intl.DateTimeFormatOptions = long ? { day: "numeric", month: "short" } : { hour: "2-digit", minute: "2-digit" };
  const xTicks = [0, 1, 2, 3, 4].map((k) => t0 + ((t1 - t0) * k) / 4);
  const nearest = (px: number) => {
    let best: number | null = null;
    values.forEach((v, i) => {
      if (v != null && (best == null || Math.abs(x(ts[i]) - px) < Math.abs(x(ts[best]) - px))) best = i;
    });
    return best;
  };
  const step1 = (d: number) => {
    let i = hover ?? (d > 0 ? -1 : values.length);
    do i += d;
    while (i >= 0 && i < values.length && values[i] == null);
    if (i >= 0 && i < values.length) setHover(i);
  };
  const hv = hover != null ? values[hover] : null;
  return (
    <div
      ref={ref}
      className="relative outline-none focus-visible:ring-2 focus-visible:ring-accent/40 rounded-lg"
      tabIndex={0}
      aria-label={`${label}: ${fmt(nums[0])} → ${fmt(nums[nums.length - 1])}`}
      onKeyDown={(e) => {
        if (e.key === "ArrowRight" || e.key === "ArrowLeft") (e.preventDefault(), step1(e.key === "ArrowRight" ? 1 : -1));
      }}
      onBlur={() => setHover(null)}
    >
      <svg
        width={w}
        height={CH}
        className="block touch-none select-none"
        role="img"
        aria-hidden
        onPointerMove={(e) => setHover(nearest(e.clientX - e.currentTarget.getBoundingClientRect().left))}
        onPointerLeave={() => setHover(null)}
      >
        {ticks.map((v) => (
          <g key={v}>
            <line x1={PAD.l} x2={w - PAD.r} y1={y(v)} y2={y(v)} stroke="var(--border)" />
            <text x={PAD.l - 8} y={y(v)} dy="0.32em" textAnchor="end" className="num fill-faint text-2xs">
              {fmt(v)}
            </text>
          </g>
        ))}
        {xTicks.map((s, k) => (
          <text key={k} x={x(s)} y={CH - 4} textAnchor={k === 0 ? "start" : k === 4 ? "end" : "middle"} className="num fill-faint text-2xs">
            {when(s, xFmt)}
          </text>
        ))}
        <g className="text-accent">
          <path d={area} fill="currentColor" opacity={0.1} />
          <path d={line} fill="none" stroke="currentColor" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
          {hover != null && hv != null && (
            <>
              <line x1={x(ts[hover])} x2={x(ts[hover])} y1={PAD.t} y2={CH - PAD.b} stroke="var(--border-strong)" />
              <circle cx={x(ts[hover])} cy={y(hv)} r={4} fill="currentColor" stroke="var(--surface)" strokeWidth={2} />
            </>
          )}
        </g>
      </svg>
      {hover != null && hv != null && (
        <div
          role="status"
          className="pointer-events-none absolute top-0 -translate-x-1/2 rounded-xl border border-line bg-surface px-3 py-1.5 text-center shadow-[var(--shadow-lg)]"
          style={{ left: Math.min(Math.max(x(ts[hover]), PAD.l + 50), w - 60) }}
        >
          <div className="num text-sm font-semibold text-fg">{fmt(hv)}</div>
          <div className="num text-2xs text-faint">{when(ts[hover], { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}</div>
        </div>
      )}
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
