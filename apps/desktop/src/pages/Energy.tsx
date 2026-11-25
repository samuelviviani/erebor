import {
  CalendarClock,
  Clock,
  Flame,
  MousePointer2,
  Plus,
  SunMedium,
  Thermometer,
  TrendingDown,
  X,
  Zap,
} from "lucide-react";
import { useEffect, useState } from "react";
import { PageTitle } from "../components/Layout";
import {
  compact,
  DAYS,
  Group,
  Item,
  MONTHS,
  NumberField,
  Sub,
  SubField,
  TextField,
  Toggles,
  type Props,
} from "../components/rows";
import { Button, cx, Switch as Sw } from "../components/ui";
import {
  errorText,
  type AppConfig,
  type PriceBand,
  type Solar,
  type Thermostat,
} from "../lib/api";
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

/** Same rule as the engine (erebor-core `in_window`): Monday = 0, windows may cross midnight. */
export function inWindow(
  days: boolean[],
  start: number,
  end: number,
  weekday: number,
  minute: number,
) {
  if (start === end) return days[weekday % 7];
  if (start < end) return days[weekday % 7] && minute >= start && minute < end;
  if (minute >= start) return days[weekday % 7];
  if (minute < end) return days[(weekday + 6) % 7];
  return false;
}

/** Grid price at a time: the first band that covers it, else the base price (erebor-core `grid_price`). */
export function gridPrice(
  e: AppConfig["energy"],
  weekday: number,
  minute: number,
): number | null {
  const b = e.bands.find((b) =>
    inWindow(b.days, b.startMinute, b.endMinute, weekday, minute),
  );
  return b ? b.price : e.pricePerKwh;
}

const SLOT = 30; // minutes per bar
type SlotState = "mine" | "off" | "costly";

/**
 * Today at a glance: one bar per half hour, as tall as the electricity price, coloured by
 * what the rules do then (mine, outside the schedule, not worth it). Uses only the user's
 * own prices and rules; "not worth it" is estimated from the current speed and draw.
 */
function DayPlan({ c, cur }: { c: AppConfig; cur: string }) {
  const t = useT();
  const status = useApp((s) => s.status);
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(id);
  }, []);
  const weekday = (now.getDay() + 6) % 7;
  const nowMin = now.getHours() * 60 + now.getMinutes();
  const e = c.energy;
  const p = status?.projection;
  // kWh per day of the current setup, from today's cost at today's price.
  const kwhDay =
    p?.energyCostPerDay != null && status?.energyPrice
      ? p.energyCostPerDay / status.energyPrice
      : null;
  const heat = e.heatingMonths[now.getMonth()]
    ? 1 - e.heatingCreditPercent / 100
    : 1;

  const slots = Array.from({ length: (24 * 60) / SLOT }, (_, i) => {
    const m = i * SLOT;
    const price = gridPrice(e, weekday, m);
    let state: SlotState = "mine";
    if (
      c.schedule.enabled &&
      !inWindow(c.schedule.days, c.schedule.startMinute, c.schedule.endMinute, weekday, m)
    )
      state = "off";
    else if (
      e.stopIfUnprofitable &&
      price != null &&
      kwhDay != null &&
      p?.valuePerDay != null &&
      p.valuePerDay - kwhDay * price * heat < e.minProfitPerDay
    )
      state = "costly";
    return { m, price, state };
  });
  const prices = slots.map((s) => s.price).filter((v): v is number => v != null);
  const max = Math.max(0, ...prices);
  const min = Math.min(...prices);
  const flat = !prices.length || max === min;
  const height = (v: number | null) =>
    v == null || flat ? 0.55 : 0.3 + (0.7 * (v - min)) / (max - min);
  const fmt = (v: number) => `${cur} ${v.toFixed(3)}`;
  const nowIdx = Math.floor(nowMin / SLOT);
  const tone: Record<SlotState, string> = {
    mine: "bg-go/70",
    costly: "bg-warn/70",
    off: "bg-surface-3",
  };

  // One price and no rule that changes over the day: every bar would be the same. Nothing
  // to show until bands, a schedule or "only when it pays" make the day differ.
  if (flat && slots.every((s) => s.state === "mine")) return null;

  return (
    <section aria-labelledby="dayplan">
      <div className="mb-2.5 flex min-h-7 flex-wrap items-center justify-between gap-x-4 px-1">
        <h2 id="dayplan" className="text-sm font-semibold">
          {t("energy.today")}
        </h2>
        <p className="num text-xs text-faint">
          {!prices.length
            ? t("energy.today.noPrice")
            : `${flat ? fmt(max) : `${fmt(min)} – ${fmt(max)}`} /kWh`}
        </p>
      </div>
      <div
        className="relative flex h-14 items-end gap-[2px] px-1"
        role="img"
        aria-label={t("energy.today")}
      >
        {slots.map((s, i) => (
          <span
            key={s.m}
            title={`${minutesToTime(s.m)} · ${s.price != null ? fmt(s.price) : "—"} · ${t(`energy.slot.${s.state}`)}`}
            className={cx(
              "flex-1 rounded-t-[3px] rounded-b-[1px] transition-[height,background-color] duration-300 ease-[var(--ease)]",
              tone[s.state],
              i < nowIdx && "opacity-45",
            )}
            style={{ height: `${height(s.price) * 100}%` }}
          />
        ))}
        <span
          aria-hidden
          className="pointer-events-none absolute -top-1 bottom-0 w-px bg-fg/70"
          style={{ left: `calc(4px + (100% - 8px) * ${nowMin / 1440})` }}
        />
      </div>
      <div className="num mt-1.5 flex justify-between px-1 text-2xs text-faint">
        {["00", "06", "12", "18", "24"].map((h) => (
          <span key={h}>{h}</span>
        ))}
      </div>
      <ul className="mt-2 flex flex-wrap gap-x-4 gap-y-1 px-1 text-xs text-muted">
        {(["mine", "costly", "off"] as const)
          .filter((k) => slots.some((s) => s.state === k))
          .map((k) => (
            <li key={k} className="flex items-center gap-1.5">
              <span className={cx("size-2 rounded-full", tone[k])} />
              {t(`energy.slot.${k}`)}
            </li>
          ))}
        {e.stopIfUnprofitable && kwhDay == null && (
          <li className="text-faint">{t("energy.today.noDraw")}</li>
        )}
      </ul>
    </section>
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

/** Starting points for common meters: the user completes address and sensor. */
const METERS: { label: string; solar: Partial<Solar> }[] = [
  {
    label: "Home Assistant",
    solar: {
      url: "http://homeassistant.local:8123/api/states/sensor.",
      field: "state",
      exportPositive: false,
    },
  },
  {
    label: "Shelly Pro 3EM",
    solar: {
      url: "http://192.168.1.50/rpc/EM.GetStatus?id=0",
      field: "total_act_power",
      exportPositive: false,
      token: "",
    },
  },
  {
    label: "Shelly EM",
    solar: {
      url: "http://192.168.1.50/emeter/0",
      field: "power",
      exportPositive: false,
      token: "",
    },
  },
];

/** A reading from a device on the home network: a coloured dot and one plain sentence. */
function Reading({ error, text }: { error: string | null | undefined; text: string }) {
  const t = useT();
  return (
    <p
      role="status"
      className={cx("flex items-center gap-2 text-xs", error ? "text-stop" : "text-muted")}
    >
      <span
        aria-hidden
        className={cx("size-2 shrink-0 rounded-full", error ? "bg-stop" : "bg-go")}
      />
      {error ? t("energy.solar.error", { e: error }) : text}
    </p>
  );
}

/** Where the sensor is and how to read it: the same three fields for meter and thermometer. */
function SensorFields({
  url,
  field,
  token,
  urlLabel,
  urlHint,
  set,
}: {
  url: string;
  field: string;
  token: string;
  urlLabel: string;
  urlHint: string;
  set: (p: { url?: string; field?: string; token?: string }) => void;
}) {
  const t = useT();
  return (
    <>
      <SubField label={urlLabel} hint={urlHint}>
        <TextField
          wide
          label={urlLabel}
          value={url}
          placeholder="http://…"
          onCommit={(url) => set({ url })}
        />
      </SubField>
      <SubField label={t("energy.field.label")} hint={t("energy.field.hint")}>
        <TextField
          label={t("energy.field.label")}
          value={field}
          placeholder="state"
          onCommit={(field) => set({ field })}
        />
      </SubField>
      <SubField label={t("energy.token.label")} hint={t("energy.token.hint")}>
        <TextField
          wide
          secret
          label={t("energy.token.label")}
          value={token}
          placeholder={t("energy.token.placeholder")}
          onCommit={(token) => set({ token })}
        />
      </SubField>
    </>
  );
}

/** Solar surplus from a meter: energy that would be exported costs only its feed-in value. */
function SolarRows({ c, save, cur }: Props & { cur: string }) {
  const t = useT();
  const exportW = useApp((s) => s.status?.solarExportW);
  const error = useApp((s) => s.status?.solarError);
  const sol = c.energy.solar;
  const set = (p: Partial<Solar>) =>
    save({ ...c, energy: { ...c.energy, solar: { ...sol, ...p } } });
  return (
    <>
      <Item
        icon={<SunMedium />}
        tone="yellow"
        label={t("energy.solar")}
        hint={t("energy.solar.hint")}
      >
        <Sw
          label={t("energy.solar")}
          checked={sol.enabled}
          onChange={(enabled) =>
            set(
              enabled && !sol.url
                ? { enabled, ...METERS[0].solar }
                : { enabled },
            )
          }
        />
      </Item>
      {sol.enabled && (
        <Sub>
          <SubField label={t("energy.solar.meter")} hint={t("energy.solar.meter.hint")}>
            <div className="flex flex-wrap gap-1.5">
              {METERS.map((m) => (
                <button
                  key={m.label}
                  type="button"
                  onClick={() => set(m.solar)}
                  aria-pressed={sol.url.startsWith(m.solar.url!.split("sensor.")[0])}
                  className="press h-8 rounded-full border border-line px-3 text-xs text-muted transition-colors hover:border-line-strong hover:text-fg aria-pressed:border-accent/30 aria-pressed:bg-accent-soft aria-pressed:text-accent"
                >
                  {m.label}
                </button>
              ))}
            </div>
          </SubField>
          <SensorFields
            url={sol.url}
            field={sol.field}
            token={sol.token}
            urlLabel={t("energy.solar.url")}
            urlHint={t("energy.solar.url.hint")}
            set={set}
          />
          <SubField label={t("energy.solar.feedIn.short")} hint={t("energy.solar.feedIn")}>
            <NumberField
              label={t("energy.solar.feedIn")}
              prefix={cur}
              suffix="/kWh"
              value={sol.feedInPrice}
              onCommit={(v) => set({ feedInPrice: v ?? 0 })}
            />
          </SubField>
          <SubField label={t("energy.solar.sign")} hint={t("energy.solar.positive")}>
            <Sw
              label={t("energy.solar.positive")}
              checked={sol.exportPositive}
              onChange={(exportPositive) => set({ exportPositive })}
            />
          </SubField>
          <Reading
            error={error}
            text={
              exportW == null
                ? t("energy.solar.waiting")
                : exportW >= 0
                  ? t("energy.solar.exporting", { w: exportW.toFixed(0) })
                  : t("energy.solar.importing", { w: (-exportW).toFixed(0) })
            }
          />
        </Sub>
      )}
    </>
  );
}

/** Winter: the heat of mining replaces other heating, so part of the cost comes back. */
function Heating({ c, save }: Props) {
  const t = useT();
  const e = c.energy;
  const on = e.heatingMonths.some(Boolean);
  return (
    <>
      <Item
        icon={<Flame />}
        tone="orange"
        label={t("energy.heating")}
        hint={t("energy.heating.hint")}
      >
        <Sw
          label={t("energy.heating")}
          checked={on}
          onChange={(v) =>
            save({
              ...c,
              energy: {
                ...e,
                // Nov–Mar by default; every month can be changed below.
                heatingMonths: Array.from(
                  { length: 12 },
                  (_, m) => v && (m <= 2 || m >= 10),
                ),
                heatingCreditPercent:
                  v && !e.heatingCreditPercent ? 100 : e.heatingCreditPercent,
              },
            })
          }
        />
      </Item>
      {on && (
        <Sub>
          <SubField label={t("energy.months")}>
            <Toggles
              labels={MONTHS[c.language]}
              value={e.heatingMonths}
              onChange={(heatingMonths) =>
                save({ ...c, energy: { ...e, heatingMonths } })
              }
            />
          </SubField>
          <SubField label={t("energy.heating.credit")} hint={t("energy.heating.credit.hint")}>
            <NumberField
              label={t("energy.heating.credit")}
              suffix="%"
              value={e.heatingCreditPercent}
              onCommit={(v) =>
                save({
                  ...c,
                  energy: {
                    ...e,
                    heatingCreditPercent: Math.min(
                      100,
                      Math.max(0, Math.round(v ?? 0)),
                    ),
                  },
                })
              }
            />
          </SubField>
        </Sub>
      )}
    </>
  );
}

/** Room thermometer: mine to heat up to one temperature, wait above it. */
function ThermostatRows({ c, save }: Props) {
  const t = useT();
  const temp = useApp((s) => s.status?.roomTempC);
  const error = useApp((s) => s.status?.roomError);
  const th = c.energy.thermostat;
  const set = (p: Partial<Thermostat>) =>
    save({ ...c, energy: { ...c.energy, thermostat: { ...th, ...p } } });
  return (
    <>
      <Item
        icon={<Thermometer />}
        tone="red"
        label={t("energy.thermostat")}
        hint={t("energy.thermostat.hint")}
      >
        <Sw
          label={t("energy.thermostat")}
          checked={th.enabled}
          onChange={(enabled) =>
            set(
              enabled && !th.url
                ? {
                    enabled,
                    url: "http://homeassistant.local:8123/api/states/sensor.",
                    field: "state",
                  }
                : { enabled },
            )
          }
        />
      </Item>
      {th.enabled && (
        <Sub>
          {/* The rule first, as a sentence: what the user actually decides. */}
          <SubField
            label={t("energy.thermostat.rule")}
            hint={t("energy.thermostat.rule.hint", { on: th.onBelowC, off: th.offAboveC })}
          >
            <div className="flex flex-wrap items-center gap-2 text-muted">
              {t("energy.thermostat.on")}
              <NumberField
                label={t("energy.thermostat.on")}
                suffix="°C"
                value={th.onBelowC}
                onCommit={(v) => v != null && set({ onBelowC: v })}
              />
              {t("energy.thermostat.off")}
              <NumberField
                label={t("energy.thermostat.off")}
                suffix="°C"
                value={th.offAboveC}
                onCommit={(v) => v != null && set({ offAboveC: v })}
              />
            </div>
          </SubField>
          <SensorFields
            url={th.url}
            field={th.field}
            token={th.token}
            urlLabel={t("energy.thermostat.url")}
            urlHint={t("energy.thermostat.url.hint")}
            set={set}
          />
          <Reading
            error={error}
            text={
              temp == null
                ? t("energy.solar.waiting")
                : t("energy.thermostat.now", { t: temp.toFixed(1) })
            }
          />
        </Sub>
      )}
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

