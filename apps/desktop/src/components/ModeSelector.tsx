import { Leaf, Scale, SlidersHorizontal, Zap, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import type { Preset } from "../lib/api";
import { useT } from "../lib/i18n";
import { cx } from "./ui";

/** Each mode has its own colour: Eco green, Balanced blue, Performance orange. */
const MODES: Record<Preset, { icon: LucideIcon; color: string; text: string }> = {
  eco: { icon: Leaf, color: "var(--go)", text: "text-go" },
  balanced: { icon: Scale, color: "var(--accent)", text: "text-accent" },
  performance: { icon: Zap, color: "var(--perf)", text: "text-perf" },
  custom: { icon: SlidersHorizontal, color: "var(--fg)", text: "text-fg" },
};
const SHARE: Record<Exclude<Preset, "custom">, number> = { eco: 25, balanced: 50, performance: 100 };
/** One bar per 5% of the processor: the meter is also the custom slider's scale. */
const BARS = 20;
const snap = (v: number) => Math.min(100, Math.max(5, Math.round(v / 5) * 5));

/**
 * Performance mode as a power meter: twenty bars rising like a volume ramp light up to the
 * share of the processor the mode uses, in the mode's colour. Below, the modes as a radio group (arrow keys move the choice). With `onCustom`,
 * the meter itself is a slider: drag or click it, or use the arrow keys, to set a custom share.
 */
export function ModeSelector({
  value,
  customPercent,
  withCustom,
  onChange,
  onCustom,
  labelledBy,
}: {
  value: Preset;
  customPercent: number;
  /** Offer "Custom"; otherwise it appears only while selected. */
  withCustom?: boolean;
  onChange: (p: Preset) => void;
  /** Custom share chosen on the meter (sets the preset to "custom" too). */
  onCustom?: (percent: number) => void;
  labelledBy: string;
}) {
  const t = useT();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const meter = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const [pending, setPending] = useState<number | null>(null);
  useEffect(() => setPending(null), [customPercent, value]);

  const list: Preset[] = ["eco", "balanced", "performance", ...(withCustom || value === "custom" ? (["custom"] as const) : [])];
  const idx = list.indexOf(value);
  const n = list.length;
  const shown: Preset = drag != null ? "custom" : value;
  const share = drag ?? pending ?? (value === "custom" ? customPercent : SHARE[value]);
  const mode = MODES[shown];

  const move = (i: number) => {
    const next = (i + n) % n;
    refs.current[next]?.focus();
    onChange(list[next]);
  };
  const commit = (v: number) => {
    setDrag(null);
    if (!onCustom) return;
    if (value === "custom" && v === customPercent) return;
    setPending(v);
    onCustom(v);
  };
  const at = (e: PointerEvent) => {
    const r = meter.current!.getBoundingClientRect();
    return snap(((e.clientX - r.left) / r.width) * 100 + 2.5);
  };

  return (
    <div className="panel overflow-hidden p-1.5">
      {/* The meter */}
      <div className="px-3 pt-3 pb-2">
        <div className="mb-2 flex items-baseline justify-between">
          <span className={cx("text-[13px] font-semibold transition-colors duration-300", mode.text)}>{t(`hw.mode.${shown}`)}</span>
          <span className="num text-xs text-muted">
            <span className={cx("text-base font-semibold transition-colors duration-300", mode.text)}>{share}%</span> CPU
          </span>
        </div>
        <div
          ref={meter}
          {...(onCustom
            ? {
                role: "slider",
                tabIndex: 0,
                "aria-label": t("hw.mode.custom"),
                "aria-valuemin": 5,
                "aria-valuemax": 100,
                "aria-valuenow": share,
                "aria-valuetext": `${share}%`,
                onPointerDown: (e: PointerEvent<HTMLDivElement>) => {
                  e.currentTarget.setPointerCapture(e.pointerId);
                  setDrag(at(e));
                },
                onPointerMove: (e: PointerEvent<HTMLDivElement>) => drag != null && setDrag(at(e)),
                onPointerUp: (e: PointerEvent<HTMLDivElement>) => drag != null && commit(at(e)),
                onPointerCancel: () => setDrag(null),
                onKeyDown: (e: KeyboardEvent) => {
                  const d = e.key === "ArrowRight" || e.key === "ArrowUp" ? 5 : e.key === "ArrowLeft" || e.key === "ArrowDown" ? -5 : 0;
                  if (!d) return;
                  e.preventDefault();
                  commit(snap(share + d));
                },
              }
            : { "aria-hidden": true })}
          className={cx(
            "flex h-14 touch-none items-end gap-[3px] rounded-lg select-none focus-visible:outline-offset-4",
            onCustom && "cursor-ew-resize",
          )}
        >
          {Array.from({ length: BARS }, (_, i) => {
            const lit = (i + 1) * 5 <= share;
            return (
              <span
                key={i}
                className="flex-1 rounded-[3px] transition-colors duration-200"
                style={{
                  height: `${26 + (74 * i) / (BARS - 1)}%`,
                  backgroundColor: lit ? mode.color : "var(--surface-3)",
                }}
              />
            );
          })}
        </div>
      </div>

      {/* The modes */}
      <div role="radiogroup" aria-labelledby={labelledBy} className="relative mt-1 grid gap-1" style={{ gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` }}>
        {idx >= 0 && (
          <span
            aria-hidden
            className="absolute inset-y-0 left-0 rounded-[14px] border border-line-strong bg-surface-3 shadow-[var(--highlight)] transition-transform duration-300 ease-[var(--ease)]"
            style={{ width: `calc((100% - ${(n - 1) * 4}px) / ${n})`, transform: `translateX(calc(${idx} * (100% + 4px)))` }}
          />
        )}
        {list.map((id, i) => {
          const on = value === id;
          const Icon = MODES[id].icon;
          return (
            <button
              key={id}
              ref={(el) => {
                refs.current[i] = el;
              }}
              type="button"
              role="radio"
              aria-checked={on}
              tabIndex={on || (idx < 0 && i === 0) ? 0 : -1}
              onClick={() => onChange(id)}
              onKeyDown={(e) => {
                if (e.key === "ArrowRight" || e.key === "ArrowDown") (e.preventDefault(), move(i + 1));
                if (e.key === "ArrowLeft" || e.key === "ArrowUp") (e.preventDefault(), move(i - 1));
              }}
              title={t(`hw.mode.${id}.desc`)}
              className={cx(
                "relative z-10 flex min-w-0 flex-col items-center justify-center gap-1 rounded-[14px] px-1 py-2.5 text-xs font-medium transition-colors duration-300 focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:outline-none",
                on ? MODES[id].text : "text-muted hover:text-fg",
              )}
            >
              <Icon className="size-4" aria-hidden />
              <span className="max-w-full truncate">{t(`hw.mode.${id}`)}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
