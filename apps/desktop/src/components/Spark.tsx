import { useId, type ReactNode } from "react";
import { HISTORY, useApp, type Sample } from "../lib/store";
import { cx } from "./ui";

/**
 * Live chart of the last few minutes: plain SVG (no chart library in the main bundle).
 * `max` fixes the scale (100 for percentages); otherwise it follows the data.
 */
export function Spark({ values, max, tone = "text-accent", label }: { values: (number | null)[]; max?: number; tone?: string; label: string }) {
  const id = useId();
  const pts = values.map((v, i) => [i, v] as const).filter((p): p is readonly [number, number] => p[1] != null);
  if (pts.length < 2) return <div className="h-9 border-b border-dashed border-line" aria-hidden />;
  const n = Math.max(values.length - 1, 1);
  const top = max ?? (Math.max(...pts.map((p) => p[1])) * 1.15 || 1);
  const xy = pts.map(([i, v]) => `${((i / n) * 100).toFixed(2)},${(32 - (Math.min(v, top) / top) * 30).toFixed(2)}`);
  return (
    <svg viewBox="0 0 100 32" preserveAspectRatio="none" className={cx("h-9 w-full", tone)} role="img" aria-label={label}>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="currentColor" stopOpacity="0.3" />
          <stop offset="1" stopColor="currentColor" stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={`${xy[0].split(",")[0]},32 ${xy.join(" ")} ${xy[xy.length - 1].split(",")[0]},32`} fill={`url(#${id})`} />
      <polyline points={xy.join(" ")} fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinejoin="round" strokeLinecap="round" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

/** One reading with its live chart, open on the page (no box): icon and label, value, chart. */
export function Meter({
  icon,
  label,
  value,
  sub,
  values,
  max,
  tone,
  valueTone,
}: {
  icon: ReactNode;
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  values: (number | null)[];
  max?: number;
  /** Colour of the chart (and of the icon). */
  tone: string;
  /** Colour of the figure, when it says something (hot, below expected). */
  valueTone?: string;
}) {
  return (
    <div className="min-w-0">
      <div className="flex items-center gap-1.5 text-xs text-faint">
        <span className={cx("[&>svg]:size-3.5", tone)}>{icon}</span>
        {label}
      </div>
      <div className="mt-1 flex items-baseline gap-2">
        <span className={cx("num text-xl font-semibold tracking-tight", valueTone)}>{value}</span>
        {sub && <span className="num truncate text-xs text-faint">{sub}</span>}
      </div>
      <div className="mt-2">
        <Spark values={values} max={max} tone={tone} label={label} />
      </div>
    </div>
  );
}

/**
 * The live history as chart series: at least one minute on screen, growing to 5 minutes
 * as readings arrive (empty slots on the left).
 */
export function useSeries() {
  const history = useApp((s) => s.history);
  const span = Math.min(HISTORY, Math.max(30, history.length));
  return <T,>(f: (s: Sample) => T | null): (T | null)[] => [
    ...Array<null>(Math.max(0, span - history.length)).fill(null),
    ...history.map(f),
  ];
}
