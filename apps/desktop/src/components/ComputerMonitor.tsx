import { Activity, Cpu, Fan, Gauge, Gpu, MemoryStick, Thermometer, Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { api, type HardwareInfo } from "../lib/api";
import { bytes, hashrate } from "../lib/format";
import { useT } from "../lib/i18n";
import { HISTORY, useApp } from "../lib/store";
import { ChartTile } from "./Spark";
import { Explain } from "./ui";

/** Colour for a temperature against the user's safety limit. */
function tempTone(t: number | null | undefined, max: number) {
  if (t == null) return "text-muted";
  if (t >= max) return "text-stop";
  if (t >= max - 10) return "text-hold";
  return "text-go";
}

const deg = (v: number | null | undefined, na: string) => (v != null ? `${v.toFixed(0)} °C` : na);

/** The whole computer at a glance: processor, memory and every GPU (with its power draw), last 5 minutes. */
export function ComputerMonitor() {
  const t = useT();
  const { status, config, history } = useApp();
  const [hw, setHw] = useState<HardwareInfo | null>(null);
  const live = status?.hardware;
  const maxT = config?.safety.maxTempC ?? 85;
  const na = t("na");
  // At least one minute on screen; the window grows to 5 minutes as readings arrive.
  const span = Math.min(HISTORY, Math.max(30, history.length));
  const series = <T,>(f: (s: (typeof history)[number]) => T | null) => [
    ...Array<null>(Math.max(0, span - history.length)).fill(null),
    ...history.map(f),
  ];

  useEffect(() => {
    api.detectHardware().then(setHw).catch(() => {});
  }, []);

  return (
    <div className="space-y-6">
      <div>
        <p className="mb-2 flex flex-wrap items-center gap-x-3 gap-y-1 px-1 text-xs text-faint">
          <span className="flex items-center gap-1.5 text-[13px] font-medium text-fg">
            <Cpu className="size-4 text-faint" />
            {hw?.cpuBrand || t("hw.cpu")}
          </span>
          {hw && (
            <span className="num">
              {hw.physicalCores ?? "—"} {t("hw.cores").toLowerCase()} · {hw.logicalThreads} {t("hw.threads").toLowerCase()} · {bytes(hw.ramTotalBytes)} RAM · {hw.osName} {hw.osVersion}
            </span>
          )}
        </p>
        <div className="grid gap-3 sm:grid-cols-3">
          <ChartTile icon={<Activity />} label={t("hw.usage")} value={live ? `${live.cpuUsage.toFixed(0)}%` : "—"} values={series((s) => s.cpu)} max={100} />
          <ChartTile
            icon={<Thermometer />}
            label={t("dash.temp")}
            value={deg(live?.cpuTempC, na)}
            sub={live && live.cpuTempC == null ? t("na.temp") : `max ${maxT} °C`}
            values={series((s) => s.cpuTemp)}
            max={Math.max(maxT, 100)}
            tone={tempTone(live?.cpuTempC, maxT)}
          />
          <ChartTile
            icon={<MemoryStick />}
            label={t("hw.memory")}
            value={bytes(live?.memUsedBytes)}
            sub={live ? `/ ${bytes(live.memTotalBytes)}` : undefined}
            values={series((s) => s.mem)}
            max={100}
            tone="text-muted"
          />
        </div>
      </div>

      {live && live.gpus.length > 0 ? (
        live.gpus.map((g, i) => (
          <div key={i}>
            <p className="mb-2 flex flex-wrap items-center gap-x-4 gap-y-1 px-1 text-xs text-faint">
              <span className="flex items-center gap-1.5 text-[13px] font-medium text-fg">
                <Gpu className="size-4 text-faint" />
                {g.name}
              </span>
              <span>{g.vendor}</span>
              <span className="num inline-flex items-center gap-1">
                <Fan className="size-3.5" /> {g.fanPercent != null ? `${g.fanPercent.toFixed(0)}%` : na}
              </span>
              <span className="num inline-flex items-center gap-1">
                <MemoryStick className="size-3.5" />
                {g.memUsedMb != null && g.memTotalMb ? `${(g.memUsedMb / 1024).toFixed(1)} / ${(g.memTotalMb / 1024).toFixed(0)} GB` : na}
              </span>
              <Explain term="gpu" align="left" />
            </p>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              <GpuHashrate i={i} watts={g.powerW} values={series((s) => s.gpus[i]?.hash ?? null)} />
              <ChartTile icon={<Activity />} label={t("gpu.load")} value={g.utilPercent != null ? `${g.utilPercent.toFixed(0)}%` : na} values={series((s) => s.gpus[i]?.util ?? null)} max={100} />
              <ChartTile
                icon={<Thermometer />}
                label={t("gpu.temp")}
                value={deg(g.tempC, na)}
                values={series((s) => s.gpus[i]?.temp ?? null)}
                max={Math.max(maxT, 100)}
                tone={tempTone(g.tempC, maxT)}
              />
              <ChartTile icon={<Zap />} label={t("gpu.power")} value={g.powerW != null ? `${g.powerW.toFixed(0)} W` : na} values={series((s) => s.gpus[i]?.power ?? null)} tone="text-perf" />
            </div>
          </div>
        ))
      ) : hw && hw.gpus.length > 0 ? (
        <p className="px-1 text-xs leading-relaxed text-faint">
          <Gpu className="mr-1.5 inline size-4" />
          {hw.gpus.map((g) => g.name).join(", ")} · {t("gpu.noLive")}
        </p>
      ) : (
        <p className="px-1 text-xs text-faint">{t("hw.gpus.none")}</p>
      )}
    </div>
  );
}

/** One card's hashrate, its efficiency and how it compares with the same model elsewhere. */
function GpuHashrate({ i, watts, values }: { i: number; watts: number | null; values: (number | null)[] }) {
  const t = useT();
  const row = useApp((s) => s.status?.gpus[i]);
  const h = row?.hashrate ?? null;
  const share = h != null && row?.expected ? h / row.expected : null;
  const sub =
    share != null && share < 0.8
      ? t("gpu.belowExpected", { p: Math.round(share * 100) })
      : h != null && watts
        ? t("gpu.efficiency", { v: hashrate(h / watts) })
        : undefined;
  return (
    <ChartTile
      icon={<Gauge />}
      label={t("gpu.hashrate")}
      value={h != null ? hashrate(h) : "—"}
      sub={sub}
      values={values}
      tone={share != null && share < 0.8 ? "text-hold" : "text-go"}
    />
  );
}
