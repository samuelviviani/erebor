import { useEffect, useState } from "react";
import { cx } from "./ui";
import { type AppConfig } from "../lib/api";
import { minutesToTime } from "../lib/format";
import { useT } from "../lib/i18n";
import { useApp } from "../lib/store";

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
export function DayPlan({ c, cur }: { c: AppConfig; cur: string }) {
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
