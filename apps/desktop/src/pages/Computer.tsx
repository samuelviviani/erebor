import {
  Activity,
  Cpu,
  Gauge,
  Gpu,
  RotateCcw,
  ShieldAlert,
  Sparkles,
  Thermometer,
  Zap,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { PageTitle } from "../components/Layout";
import { Group, Item, NumberField, StepButton, Sub } from "../components/rows";
import { JobProgress } from "../components/shared";
import { Meter, useSeries } from "../components/Spark";
import { Button, Switch as Sw } from "../components/ui";
import { api, errorText, type AppConfig, type HardwareInfo } from "../lib/api";
import { bytes, hashrate } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp, type Sample } from "../lib/store";

/**
 * The machine: live charts of every chip, the safety limits and the graphics card.
 * Miners, pool, benchmark and log are advanced: they live in Settings.
 */
export default function Computer() {
  const t = useT();
  const { config, saveConfig, toast } = useApp();
  const c = config!;
  const [hw, setHw] = useState<HardwareInfo | null>(null);
  useEffect(() => {
    api.detectHardware().then(setHw).catch(() => {});
  }, []);
  const save = async (next: AppConfig) => {
    try {
      await saveConfig(next);
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    }
  };
  const temp = (d: number) =>
    save({
      ...c,
      safety: {
        ...c.safety,
        maxTempC: Math.min(95, Math.max(60, c.safety.maxTempC + d)),
      },
    });
  const liveGpus = useApp((s) => s.status?.hardware.gpus.length ?? 0);
  const hasGpu = liveGpus > 0 || !!hw?.gpus.length;

  return (
    <div className="max-w-3xl">
      <PageTitle sub={t("hw.live.sub")}>{t("nav.computer")}</PageTitle>
      <div className="space-y-9">
        <Sensors hw={hw} />

        <Group title={t("settings.group.safety")} help="temp">
          <Item icon={<Thermometer />} tone="red" label={t("hw.maxTemp")} hint={t("settings.maxTemp.hint")}>
            <div className="flex items-center gap-1">
              <StepButton
                label="−"
                onClick={() => temp(-5)}
                disabled={c.safety.maxTempC <= 60}
              />
              <span className="num w-14 text-center text-sm font-medium">
                {c.safety.maxTempC} °C
              </span>
              <StepButton
                label="+"
                onClick={() => temp(5)}
                disabled={c.safety.maxTempC >= 95}
              />
            </div>
          </Item>
          <Item icon={<ShieldAlert />} tone="orange" label={t("hw.stopOnOverheat")}>
            <Sw
              label={t("hw.stopOnOverheat")}
              checked={c.safety.stopOnOverheat}
              onChange={(stopOnOverheat) =>
                save({ ...c, safety: { ...c.safety, stopOnOverheat } })
              }
            />
          </Item>
          <Item
            icon={<RotateCcw />}
            tone="teal"
            label={t("settings.autoRestart")}
            hint={t("settings.autoRestart.desc")}
          >
            <Sw
              label={t("settings.autoRestart")}
              checked={c.safety.autoRestart}
              onChange={(autoRestart) =>
                save({ ...c, safety: { ...c.safety, autoRestart } })
              }
            />
          </Item>
        </Group>

        {hasGpu && (
          <Group title={t("settings.group.gpu")}>
            <Item
              icon={<Gauge />}
              tone="orange"
              label={t("settings.powerLimit")}
              hint={t("settings.powerLimit.hint")}
            >
              <NumberField
                label={t("settings.powerLimit")}
                suffix="W"
                placeholder="—"
                value={c.performance.gpuPowerLimitW}
                onCommit={(v) =>
                  save({
                    ...c,
                    performance: {
                      ...c.performance,
                      gpuPowerLimitW: v == null ? null : Math.round(v),
                    },
                  })
                }
              />
            </Item>
            <TuneRow />
          </Group>
        )}
      </div>
    </div>
  );
}

/** Colour for a temperature against the user's safety limit; `calm` below it. */
function tempTone(t: number | null | undefined, max: number, calm: string) {
  if (t == null) return calm;
  if (t >= max) return "text-stop";
  if (t >= max - 10) return "text-hold";
  return calm;
}

const fix = (v: number | null | undefined, unit: string) => (v != null ? `${v.toFixed(0)} ${unit}` : "—");

/**
 * Every chip with its live charts of the last 5 minutes, open on the page: load (blue),
 * temperature (teal, amber near the limit, red past it), draw (orange) and speed (green).
 */
function Sensors({ hw }: { hw: HardwareInfo | null }) {
  const t = useT();
  const { status, config } = useApp();
  const live = status?.hardware;
  const maxT = config?.safety.maxTempC ?? 85;
  const cpuMining = status?.state === "running" && status.coin.device === "cpu";
  const h = status?.stats?.hashrate10s ?? status?.stats?.hashrate60s ?? null;
  const series = useSeries();
  const cpuTempTone = tempTone(live?.cpuTempC, maxT, "text-[#14b8a6]");

  return (
    <section aria-labelledby="sensors" className="space-y-8">
      <div className="flex min-h-7 items-center justify-between gap-3 px-1">
        <h2 id="sensors" className="text-sm font-semibold">
          {t("hw.sensors")}
        </h2>
        <span className="num text-xs text-faint">{t("hw.limit", { t: maxT })}</span>
      </div>

      <Chip
        icon={<Cpu />}
        name={hw?.cpuBrand || t("hw.cpu")}
        detail={
          hw && live
            ? `${hw.physicalCores ?? "—"}C/${hw.logicalThreads}T · RAM ${bytes(live.memUsedBytes).replace(/ GB$/, "")}/${bytes(live.memTotalBytes)}`
            : undefined
        }
      >
        <Meter icon={<Activity />} label={t("gpu.load")} value={live ? `${live.cpuUsage.toFixed(0)} %` : "—"} values={series((s) => s.cpu)} max={100} tone="text-accent" />
        <Meter
          icon={<Thermometer />}
          label={t("gpu.temp")}
          value={live && live.cpuTempC == null ? t("na") : fix(live?.cpuTempC, "°C")}
          sub={live && live.cpuTempC == null ? t("na.temp") : undefined}
          values={series((s) => s.cpuTemp)}
          max={Math.max(maxT, 100)}
          tone={cpuTempTone}
          valueTone={cpuTempTone === "text-[#14b8a6]" ? undefined : cpuTempTone}
        />
        <Meter icon={<Zap />} label={t("gpu.power")} value={fix(live?.powerW, "W")} values={series((s) => s.cpuPower)} tone="text-perf" />
        <Meter icon={<Gauge />} label={t("gpu.hashrate")} value={cpuMining && h ? hashrate(h) : "—"} values={series((s) => (cpuMining ? s.hash : null))} tone="text-go" />
      </Chip>

      {live?.gpus.map((_, i) => (
        <GpuChip key={i} i={i} maxT={maxT} series={series} />
      ))}
      {!live?.gpus.length && hw && hw.gpus.length > 0 && (
        <p className="px-1 text-xs leading-relaxed text-faint">
          <span className="text-sm text-fg">{hw.gpus.map((g) => g.name).join(", ")}</span> · {t("gpu.noLive")}
        </p>
      )}
      {hw && !hw.gpus.length && !live?.gpus.length && (
        <p className="px-1 text-xs text-faint">{t("hw.gpus.none")}</p>
      )}
    </section>
  );
}

/** One chip: its name and specs on a line, then its charts side by side. */
function Chip({ icon, name, detail, children }: { icon: ReactNode; name: string; detail?: string; children: ReactNode }) {
  return (
    <div>
      <p className="mb-3 flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-0.5 px-1">
        <span className="flex min-w-0 items-center gap-2 text-sm font-medium [&>svg]:size-4 [&>svg]:shrink-0 [&>svg]:text-faint">
          {icon}
          <span className="truncate">{name}</span>
        </span>
        {detail && <span className="num truncate text-xs text-faint">{detail}</span>}
      </p>
      <div className="grid grid-cols-2 gap-x-8 gap-y-6 px-1 sm:grid-cols-4">{children}</div>
    </div>
  );
}

/** One card: its readings, speed, and how it compares with the same model elsewhere. */
function GpuChip({
  i,
  maxT,
  series,
}: {
  i: number;
  maxT: number;
  series: <T>(f: (s: Sample) => T | null) => (T | null)[];
}) {
  const t = useT();
  const g = useApp((s) => s.status?.hardware.gpus[i]);
  const row = useApp((s) => s.status?.gpus[i]);
  if (!g) return null;
  const h = row?.hashrate ?? null;
  const share = h != null && row?.expected ? h / row.expected : null;
  const low = share != null && share < 0.8;
  const vram =
    g.memUsedMb != null && g.memTotalMb
      ? `${(g.memUsedMb / 1024).toFixed(1)} / ${(g.memTotalMb / 1024).toFixed(0)} GB`
      : null;
  const tone = tempTone(g.tempC, maxT, "text-[#14b8a6]");
  return (
    <Chip
      icon={<Gpu />}
      name={g.name}
      detail={[g.vendor, g.fanPercent != null && `${t("gpu.fan")} ${g.fanPercent.toFixed(0)}%`, vram]
        .filter(Boolean)
        .join(" · ")}
    >
      <Meter icon={<Activity />} label={t("gpu.load")} value={fix(g.utilPercent, "%")} values={series((s) => s.gpus[i]?.util ?? null)} max={100} tone="text-accent" />
      <Meter
        icon={<Thermometer />}
        label={t("gpu.temp")}
        value={fix(g.tempC, "°C")}
        values={series((s) => s.gpus[i]?.temp ?? null)}
        max={Math.max(maxT, 100)}
        tone={tone}
        valueTone={tone === "text-[#14b8a6]" ? undefined : tone}
      />
      <Meter icon={<Zap />} label={t("gpu.power")} value={fix(g.powerW, "W")} values={series((s) => s.gpus[i]?.power ?? null)} tone="text-perf" />
      <Meter
        icon={<Gauge />}
        label={t("gpu.hashrate")}
        value={h != null ? hashrate(h) : "—"}
        sub={
          low
            ? t("gpu.belowExpected", { p: Math.round(share! * 100) })
            : h != null && g.powerW
              ? t("gpu.efficiency", { v: hashrate(h / g.powerW) })
              : undefined
        }
        values={series((s) => s.gpus[i]?.hash ?? null)}
        tone={low ? "text-hold" : "text-go"}
        valueTone={low ? "text-hold" : undefined}
      />
    </Chip>
  );
}

/** Efficiency search: tries lower power limits on the running miner and keeps the best. */
function TuneRow() {
  const t = useT();
  const { status, toast, config } = useApp();
  const job = status?.job;
  const gpuMining = status?.state === "running" && status.coin.device === "gpu";
  const start = async () => {
    try {
      await api.tunePower();
    } catch (e) {
      toast("error", translateError(config?.language ?? "en", errorText(e)));
    }
  };
  return (
    <>
      <Item icon={<Sparkles />} tone="violet" label={t("tune.title")} hint={gpuMining ? t("tune.hint") : t("tune.needsGpu")}>
        <Button size="sm" onClick={start} disabled={!gpuMining || (!!job && !job.done)}>
          {t("tune.run")}
        </Button>
      </Item>
      {job?.kind === "tune" && (
        <Sub>
          <JobProgress />
        </Sub>
      )}
    </>
  );
}
