import { useId, type ReactNode } from "react";
import { cx } from "./ui";

/**
 * Live chart of the last few minutes: plain SVG (no chart library in the main bundle).
 * `max` fixes the scale (100 for percentages); otherwise it follows the data.
 */
export function Spark({ values, max, tone = "text-accent", label }: { values: (number | null)[]; max?: number; tone?: string; label: string }) {
  const id = useId();
  const pts = values.map((v, i) => [i, v] as const).filter((p): p is readonly [number, number] => p[1] != null);
  if (pts.length < 2) return <div className="skeleton h-14 opacity-40" aria-hidden />;
  const n = Math.max(values.length - 1, 1);
  const top = max ?? (Math.max(...pts.map((p) => p[1])) * 1.15 || 1);
  const xy = pts.map(([i, v]) => `${((i / n) * 100).toFixed(2)},${(32 - (Math.min(v, top) / top) * 30).toFixed(2)}`);
  return (
    <svg viewBox="0 0 100 32" preserveAspectRatio="none" className={cx("h-14 w-full", tone)} role="img" aria-label={label}>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="currentColor" stopOpacity="0.32" />
          <stop offset="1" stopColor="currentColor" stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={`${xy[0].split(",")[0]},32 ${xy.join(" ")} ${xy[xy.length - 1].split(",")[0]},32`} fill={`url(#${id})`} />
      <polyline points={xy.join(" ")} fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinejoin="round" strokeLinecap="round" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

/** Metric tile with its live chart. */
export function ChartTile({
  icon,
  label,
  value,
  sub,
  values,
  max,
  tone,
}: {
  icon: ReactNode;
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  values: (number | null)[];
  max?: number;
  tone?: string;
}) {
  return (
    <div className="panel min-w-0 p-4">
      <div className="flex items-center gap-1.5 text-xs font-medium text-faint [&>svg]:size-3.5">
        {icon}
        {label}
      </div>
      <div className="mt-1 flex items-baseline gap-2">
        <span className={cx("num text-[22px] font-bold tracking-tight", tone)}>{value}</span>
        {sub && <span className="truncate text-xs text-faint">{sub}</span>}
      </div>
      <div className="mt-2">
        <Spark values={values} max={max} tone={tone} label={label} />
      </div>
    </div>
  );
}
