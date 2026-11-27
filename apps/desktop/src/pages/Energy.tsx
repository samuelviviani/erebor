import { CalendarClock, Clock, MousePointer2, Plus, TrendingDown, X, Zap } from "lucide-react";
import { DayPlan } from "../components/DayPlan";
import { Heating, SolarRows, ThermostatRows } from "../components/EnergySensors";
import { PageTitle } from "../components/Layout";
import {
  compact,
  DAYS,
  Group,
  Item,
  NumberField,
  Sub,
  SubField,
  Toggles,
  type Props,
} from "../components/rows";
import { Button, cx, Switch as Sw } from "../components/ui";
import { errorText, type AppConfig, type PriceBand } from "../lib/api";
import { minutesToTime, timeToMinutes } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/** Electricity and every rule about when mining is worth it, with today's plan when the day varies. */
export default function Energy() {
  const t = useT();
  const { config, saveConfig, toast } = useApp();
  const c = config!;
  const cur = c.energy.currency === "USD" ? "$" : "€";
  // A window like 22:00–07:00 ends the next morning: say so, it is easy to misread.
  const overnight = (start: number, end: number) =>
    end <= start && start !== end
      ? t("energy.overnight", { t: minutesToTime(end) })
      : undefined;
  const save = async (next: AppConfig) => {
    try {
      await saveConfig(next);
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    }
  };

  return (
    <div className="max-w-2xl">
      <PageTitle sub={t("energy.sub")}>{t("nav.energy")}</PageTitle>
      <div className="space-y-9">
        <Group title={t("energy.group.price")} help="power" aside={<PriceNow cur={cur} />}>
          <Item icon={<Zap />} tone="yellow" label={t("settings.price.short")} hint={t("settings.price.hint")}>
            <NumberField
              label={t("settings.price")}
              prefix={cur}
              suffix="/kWh"
              placeholder="0,25"
              value={c.energy.pricePerKwh}
              onCommit={(pricePerKwh) =>
                save({ ...c, energy: { ...c.energy, pricePerKwh } })
              }
            />
          </Item>
          <Bands c={c} save={save} cur={cur} />
        </Group>

        <DayPlan c={c} cur={cur} />

        <Group title={t("energy.group.when")}>
          <Item
            icon={<TrendingDown />}
            tone="pink"
            label={t("settings.stopUnprofitable.short")}
            hint={t("settings.stopUnprofitable.hint")}
          >
            <Sw
              label={t("settings.stopUnprofitable")}
              checked={c.energy.stopIfUnprofitable}
              onChange={(stopIfUnprofitable) =>
                save({ ...c, energy: { ...c.energy, stopIfUnprofitable } })
              }
            />
          </Item>
          {c.energy.stopIfUnprofitable && (
            <Sub>
              <SubField label={t("settings.minProfit")} hint={t("settings.minProfit.hint")}>
                <NumberField
                  label={t("settings.minProfit")}
                  prefix={cur}
                  suffix={t("settings.perDayUnit")}
                  value={c.energy.minProfitPerDay}
                  onCommit={(v) =>
                    save({
                      ...c,
                      energy: { ...c.energy, minProfitPerDay: v ?? 0 },
                    })
                  }
                />
              </SubField>
            </Sub>
          )}
          <Item icon={<CalendarClock />} tone="orange" label={t("settings.schedule")} hint={t("settings.schedule.desc")}>
            <Sw
              label={t("settings.schedule")}
              checked={c.schedule.enabled}
              onChange={(enabled) =>
                save({ ...c, schedule: { ...c.schedule, enabled } })
              }
            />
          </Item>
          {c.schedule.enabled && (
            <Sub>
              <SubField
                label={t("energy.hours")}
                hint={overnight(c.schedule.startMinute, c.schedule.endMinute)}
              >
                <TimeRange
                  start={c.schedule.startMinute}
                  end={c.schedule.endMinute}
                  onStart={(startMinute) =>
                    save({ ...c, schedule: { ...c.schedule, startMinute } })
                  }
                  onEnd={(endMinute) =>
                    save({ ...c, schedule: { ...c.schedule, endMinute } })
                  }
                />
              </SubField>
              <SubField label={t("energy.days")}>
                <Toggles
                  labels={DAYS[c.language]}
                  value={c.schedule.days}
                  onChange={(days) =>
                    save({ ...c, schedule: { ...c.schedule, days } })
                  }
                />
              </SubField>
            </Sub>
          )}
          <IdleRow c={c} save={save} />
        </Group>

        <Group title={t("energy.group.sources")}>
          <SolarRows c={c} save={save} cur={cur} />
          <Heating c={c} save={save} />
          <ThermostatRows c={c} save={save} />
        </Group>
      </div>
    </div>
  );
}

/** "from [08:00] to [19:00]", saved when leaving each field. */
function TimeRange({
  start,
  end,
  onStart,
  onEnd,
}: {
  start: number;
  end: number;
  onStart: (m: number) => void;
  onEnd: (m: number) => void;
}) {
  const t = useT();
  return (
    <span className="inline-flex flex-wrap items-center gap-2 text-muted">
      {t("settings.schedule.from")}
      <input
        type="time"
        aria-label={t("settings.schedule.from")}
        className={compact}
        defaultValue={minutesToTime(start)}
        onBlur={(e) => onStart(timeToMinutes(e.target.value))}
      />
      {t("settings.schedule.to")}
      <input
        type="time"
        aria-label={t("settings.schedule.to")}
        className={compact}
        defaultValue={minutesToTime(end)}
        onBlur={(e) => onEnd(timeToMinutes(e.target.value))}
      />
    </span>
  );
}

/** Mine only while nobody uses the computer. */
function IdleRow({ c, save }: Props) {
  const t = useT();
  const idleSecs = useApp((s) => s.status?.idleSecs);
  const on = c.performance.idleMinutes != null;
  const set = (idleMinutes: number | null) =>
    save({ ...c, performance: { ...c.performance, idleMinutes } });
  return (
    <>
      <Item
        icon={<MousePointer2 />}
        tone="teal"
        label={t("settings.idle")}
        hint={t("settings.idle.hint")}
      >
        <Sw
          label={t("settings.idle")}
          checked={on}
          onChange={(v) => set(v ? 5 : null)}
        />
      </Item>
      {on && (
        <Sub>
          <SubField
            label={t("settings.idle.after")}
            hint={
              idleSecs === null
                ? t("settings.idle.unsupported")
                : t("settings.idle.after.hint")
            }
          >
            <NumberField
              label={t("settings.idle.after")}
              suffix={t("unit.minutes")}
              value={c.performance.idleMinutes}
              onCommit={(v) =>
                set(v == null ? 5 : Math.min(240, Math.max(1, Math.round(v))))
              }
            />
          </SubField>
        </Sub>
      )}
    </>
  );
}

/** Time-of-day prices copied from the user's bill: one block per band, its fields labelled. */
function Bands({ c, save, cur }: Props & { cur: string }) {
  const t = useT();
  const bands = c.energy.bands;
  const set = (next: PriceBand[]) =>
    save({ ...c, energy: { ...c.energy, bands: next } });
  const patch = (i: number, p: Partial<PriceBand>) =>
    set(bands.map((b, j) => (j === i ? { ...b, ...p } : b)));
  return (
    <>
      <Item
        icon={<Clock />}
        tone="indigo"
        label={t("energy.bands")}
        hint={t("energy.bands.hint")}
      >
        <Button
          size="sm"
          onClick={() =>
            set([
              ...bands,
              {
                startMinute: 19 * 60,
                endMinute: 8 * 60,
                days: Array(7).fill(true),
                price: c.energy.pricePerKwh ?? 0,
              },
            ])
          }
        >
          <Plus /> {t("energy.bands.add")}
        </Button>
      </Item>
      {bands.map((b, i) => (
        <Sub key={`${i}-${b.startMinute}-${b.endMinute}-${b.price}`}>
          <div className="flex items-center justify-between gap-3">
            <span className="text-sm font-medium">
              {t("energy.band", { n: i + 1 })}
            </span>
            <Button
              size="sm"
              variant="ghost"
              aria-label={t("energy.bands.remove")}
              onClick={() => set(bands.filter((_, j) => j !== i))}
            >
              <X /> {t("energy.bands.remove.short")}
            </Button>
          </div>
          <SubField label={t("energy.hours")}>
            <TimeRange
              start={b.startMinute}
              end={b.endMinute}
              onStart={(startMinute) => patch(i, { startMinute })}
              onEnd={(endMinute) => patch(i, { endMinute })}
            />
          </SubField>
          <SubField label={t("energy.days")}>
            <Toggles
              labels={DAYS[c.language]}
              value={b.days}
              onChange={(days) => patch(i, { days })}
            />
          </SubField>
          <SubField label={t("settings.price.short")}>
            <NumberField
              label={t("settings.price")}
              prefix={cur}
              suffix="/kWh"
              value={b.price}
              onCommit={(v) => patch(i, { price: v ?? 0 })}
            />
          </SubField>
        </Sub>
      ))}
    </>
  );
}

/** The price the decisions use right now, beside the price heading. */
function PriceNow({ cur }: { cur: string }) {
  const t = useT();
  const price = useApp((s) => s.status?.energyPrice);
  const held = useApp((s) => s.status?.held);
  if (price == null) return null;
  return (
    <span className={cx("num", held === "unprofitable" && "text-warn")}>
      {t("energy.now", { p: `${cur} ${price.toFixed(3)}` })}
      {held === "unprofitable" && ` · ${t("held.unprofitable")}`}
    </span>
  );
}
