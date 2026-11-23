import { Leaf, Scale, SlidersHorizontal, Zap, type LucideIcon } from "lucide-react";
import { useRef } from "react";
import type { Preset } from "../lib/api";
import { useT } from "../lib/i18n";
import { cx } from "./ui";

/** Each mode has its own colour: Eco green, Balanced blue, Performance orange. */
const MODES: Record<Preset, { icon: LucideIcon; on: string; hover: string; indicator: string }> = {
  eco: { icon: Leaf, on: "text-go", hover: "hover:text-go hover:bg-go/5", indicator: "border-go/40 bg-go/12" },
  balanced: { icon: Scale, on: "text-accent", hover: "hover:text-accent hover:bg-accent/5", indicator: "border-accent/40 bg-accent/12" },
  performance: { icon: Zap, on: "text-perf", hover: "hover:text-perf hover:bg-perf/5", indicator: "border-perf/40 bg-perf/12" },
  custom: { icon: SlidersHorizontal, on: "text-fg", hover: "hover:text-fg hover:bg-surface-2", indicator: "border-faint/50 bg-surface-2" },
};
const SHARE: Record<Exclude<Preset, "custom">, number> = { eco: 25, balanced: 50, performance: 100 };

/**
 * Performance mode as one sliding control (Home and Computer pages). Arrow keys move the
 * choice like a native radio group; each option shows the share of the processor it uses.
 */
export function ModeSelector({
  value,
  customPercent,
  withCustom,
  onChange,
  labelledBy,
}: {
  value: Preset;
  customPercent: number;
  /** Offer "Custom" (Computer page); on Home it appears only while selected. */
  withCustom?: boolean;
  onChange: (p: Preset) => void;
  labelledBy: string;
}) {
  const t = useT();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const list: Preset[] = ["eco", "balanced", "performance", ...(withCustom || value === "custom" ? (["custom"] as const) : [])];
  const idx = list.indexOf(value);
  const n = list.length;

  const move = (i: number) => {
    const next = (i + n) % n;
    refs.current[next]?.focus();
    onChange(list[next]);
  };

  return (
    <div
      role="radiogroup"
      aria-labelledby={labelledBy}
      className="relative grid gap-2 rounded-2xl border border-line bg-surface p-1.5"
      style={{ gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` }}
    >
      {idx >= 0 && (
        <span
          aria-hidden
          className={cx(
            "absolute top-1.5 bottom-1.5 left-1.5 rounded-xl border transition-[transform,background-color,border-color] duration-500 ease-[cubic-bezier(0.2,0.8,0.2,1)]",
            MODES[list[idx]].indicator,
          )}
          style={{ width: `calc((100% - 12px - ${(n - 1) * 8}px) / ${n})`, transform: `translateX(calc(${idx} * (100% + 8px)))` }}
        />
      )}
      {list.map((id, i) => {
        const on = value === id;
        const Icon = MODES[id].icon;
        const share = id === "custom" ? customPercent : SHARE[id];
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
              "relative z-10 flex flex-col items-center gap-1 rounded-xl px-2 py-2.5 text-[13px] font-medium transition-colors duration-300 focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:outline-none",
              on ? MODES[id].on : cx("text-muted", MODES[id].hover),
            )}
          >
            <span key={on ? `${id}-on` : id} className={on ? "animate-pop" : undefined}>
              <Icon className="size-5" aria-hidden />
            </span>
            {t(`hw.mode.${id}`)}
            <span className={cx("num text-[11px] font-normal", on ? "opacity-80" : "text-faint")}>{t("mode.share", { p: share })}</span>
          </button>
        );
      })}
    </div>
  );
}
