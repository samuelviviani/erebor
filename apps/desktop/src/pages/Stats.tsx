import { TrendingUp } from "lucide-react";
import { Cell } from "recharts";
import { useEffect, useState, type ReactNode } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { PageTitle } from "../components/Layout";
import {
  Card,
  Explain,
  cx,
  Notice,
  Segmented,
  type HelpTerm,
} from "../components/ui";
import { api, type Earnings, type StatsReport } from "../lib/api";
import { btc, duration, money } from "../lib/format";
import { useT } from "../lib/i18n";
import { useApp } from "../lib/store";

const DAY = 86_400;
const PERIODS = [
  { key: "stats.p.day", secs: DAY },
  { key: "stats.p.week", secs: 7 * DAY },
  { key: "stats.p.month", secs: 30 * DAY },
] as const;

/** Earnings for people who know nothing about crypto: euros first, technical details last. */
export default function Stats() {
  const t = useT();
  const { config, status, setPage } = useApp();
  const [reports, setReports] = useState<(StatsReport | null)[]>([
    null,
    null,
    null,
  ]);
  const [error, setError] = useState<string | null>(null);
  const locale = config?.language === "en" ? "en-US" : "it-IT";
  const cur = config?.energy.currency ?? "EUR";
  const btcPrice = status?.market.btc
    ? cur === "USD"
      ? status.market.btc.usd
      : status.market.btc.eur
    : null;
  const fmt = (v: number) => money(v, cur, locale);

  useEffect(() => {
    let alive = true;
    const load = () =>
      Promise.all(PERIODS.map((p) => api.getStats(p.secs)))
        .then((r) => alive && (setReports(r), setError(null)))
        .catch((e) => alive && setError(String(e)));
    load();
    const id = setInterval(load, 30_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  const value = (e?: Earnings) =>
    e && e.miningSecs > 0
      ? e.netValue != null
        ? fmt(e.netValue)
        : e.netBtc > 0
          ? btc(e.netBtc)
          : "—"
      : "—";
  const [span, setSpan] = useState<Span>("7d");
  const idx = SPANS.indexOf(span);
  const report = reports[idx];
  const e = report?.earnings;
  const proj = status?.projection;
  const projDay = proj?.netPerDay ?? proj?.valuePerDay ?? null;
  const loading = <div className="skeleton mt-2 h-9 w-28" />;

  return (
    <>
      <PageTitle
        sub={t("stats.sub")}
        action={
          <Segmented
            label={t("stats.byDay")}
            value={span}
            onChange={setSpan}
            options={SPANS.map((r) => ({
              value: r,
              label: t(`stats.range.${r}`),
            }))}
          />
        }
      >
        {t("nav.stats")}
      </PageTitle>
      {error && (
        <div className="mb-6">
          <Notice tone="err">{error}</Notice>
        </div>
      )}

      {/* The two numbers for the chosen period, no boxes; the profit is worked out below. */}
      <dl className="grid gap-8 sm:grid-cols-3">
        <Figure
          label={t("stats.earned")}
          help="earnings"
          sub={e && e.netBtc > 0 ? btc(e.netBtc) : undefined}
        >
          {report ? value(e) : loading}
        </Figure>
        <Figure label={t("stats.time")}>
          {report
            ? e && e.miningSecs > 0
              ? duration(e.miningSecs)
              : "—"
            : loading}
        </Figure>
      </dl>
      {(projDay != null || (e && !e.complete)) && (
        <div className="mt-6 space-y-1.5 text-sm">
          {projDay != null && (
            <p className="flex items-center gap-2 text-muted">
              <TrendingUp className="size-4 text-go" />
              {t("stats.ifContinue")}{" "}
              <span className="num font-medium text-fg">
                ≈ {fmt(projDay * 30)}
              </span>{" "}
              {t("stats.perMonth")}
            </p>
          )}
          {e && !e.complete && (
            <p className="text-xs text-faint">{t("stats.incomplete.short")}</p>
          )}
        </div>
      )}

      <div className="mt-10 grid gap-4 lg:grid-cols-[3fr_2fr]">
        <EarningsChart
          report={report}
          span={span}
          btcPrice={btcPrice}
          fmt={fmt}
          locale={locale}
        />
        <ProfitCard
          e={e}
          span={span}
          fmt={fmt}
          locale={locale}
          onSetPrice={() => setPage("settings")}
        />
      </div>
    </>
  );
}

/**
 * From earnings to profit, as a short bill for the chosen period: earned, minus the
 * electricity (kWh × price), equals what is really left.
 */
function ProfitCard({
  e,
  span,
  fmt,
  locale,
  onSetPrice,
}: {
  e?: Earnings;
  span: Span;
  fmt: (v: number) => string;
  locale: string;
  onSetPrice: () => void;
}) {
  const t = useT();
  const price = useApp((s) => s.config?.energy.pricePerKwh ?? null);
  const kwh = e?.energyKwh;
  const earned = e && e.miningSecs > 0 ? e.netValue : null;
  return (
    <Card>
      <h2 className="flex items-center gap-1.5 text-[15px] font-medium">
        {t("stats.toProfit")}
        <Explain term="power" align="left" />
      </h2>
      <p className="mt-0.5 mb-4 text-xs text-faint">
        {t(`stats.range.${span}`)} · {t("stats.feesIncluded")}
      </p>
      <dl className="space-y-3 text-sm">
        <div className="flex items-baseline justify-between gap-3">
          <dt className="text-muted">{t("stats.earned")}</dt>
          <dd className="num">{earned != null ? `+ ${fmt(earned)}` : "—"}</dd>
        </div>
        <div className="flex items-baseline justify-between gap-3">
          <dt className="min-w-0 text-muted">
            {t("stats.power")}
            {kwh != null && price != null && (
              <span className="num block text-xs text-faint">
                {kwh.toLocaleString(locale, { maximumFractionDigits: 2 })} kWh ×{" "}
                {fmt(price)}
              </span>
            )}
          </dt>
          <dd className="num shrink-0 text-right">
            {e?.energyCost != null ? (
              `− ${fmt(e.energyCost)}`
            ) : (
              <button
                type="button"
                onClick={onSetPrice}
                className="text-xs text-accent hover:underline"
              >
                {t(price == null ? "stats.power.set" : "stats.power.setWatts")}
              </button>
            )}
          </dd>
        </div>
        <div className="flex items-baseline justify-between gap-3 border-t border-line pt-3">
          <dt className="font-medium">{t("stats.profit.net")}</dt>
          <dd
            className={cx(
              "num text-2xl font-semibold tracking-tight",
              e?.netProfit != null &&
                (e.netProfit < 0 ? "text-stop" : "text-go"),
            )}
          >
            {e?.netProfit != null ? fmt(e.netProfit) : "—"}
          </dd>
        </div>
      </dl>
      {e?.netProfit != null && e.netProfit < 0 && (
        <p className="mt-3 text-xs leading-relaxed text-faint">
          {t("stats.loss.hint")}
        </p>
      )}
    </Card>
  );
}

function Figure({
  label,
  help,
  sub,
  children,
}: {
  label: string;
  help?: HelpTerm;
  sub?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="min-w-0">
      <dt className="flex items-center gap-1.5 text-sm text-muted">
        {label}
        {help && <Explain term={help} align="left" />}
      </dt>
      <dd className="num mt-1 text-4xl font-semibold tracking-tight">
        {children}
      </dd>
      {sub && <dd className="num mt-1.5 text-xs text-faint">{sub}</dd>}
    </div>
  );
}

const axis = {
  stroke: "var(--faint)",
  fontSize: 11,
  tickLine: false,
  axisLine: false,
} as const;
const tooltip = {
  contentStyle: {
    background: "var(--surface-2)",
    border: "1px solid var(--border)",
    borderRadius: 10,
    fontSize: 12,
  },
  labelStyle: { color: "var(--muted)" },
  itemStyle: { color: "var(--fg)" },
  cursor: { fill: "var(--surface-2)" },
};

type Span = "24h" | "7d" | "30d";
const SPANS: Span[] = ["24h", "7d", "30d"];

/**
 * Earnings over time with a fixed time axis: one bar per hour (24 h) or per day (7/30 days),
 * empty slots shown as faint tracks, the current slot highlighted.
 */
function EarningsChart({
  report,
  span,
  btcPrice,
  fmt,
  locale,
}: {
  report: StatsReport | null;
  span: Span;
  btcPrice: number | null;
  fmt: (v: number) => string;
  locale: string;
}) {
  const t = useT();
  const inMoney = btcPrice != null;

  const buckets = (() => {
    const now = new Date();
    const hourly = span === "24h";
    const n = hourly ? 24 : span === "7d" ? 7 : 30;
    const key = (d: Date) =>
      hourly ? `${d.toDateString()} ${d.getHours()}` : d.toDateString();
    const slots: {
      key: string;
      label: string;
      full: string;
      btc: number;
      current: boolean;
    }[] = [];
    for (let i = n - 1; i >= 0; i--) {
      const d = new Date(now);
      if (hourly) d.setHours(now.getHours() - i, 0, 0, 0);
      else d.setDate(now.getDate() - i);
      slots.push({
        key: key(d),
        label: hourly
          ? `${String(d.getHours()).padStart(2, "0")}`
          : d.toLocaleDateString(locale, { day: "2-digit", month: "2-digit" }),
        full: hourly
          ? d.toLocaleString(locale, {
              weekday: "short",
              hour: "2-digit",
              minute: "2-digit",
            })
          : d.toLocaleDateString(locale, {
              weekday: "long",
              day: "numeric",
              month: "long",
            }),
        btc: 0,
        current: i === 0,
      });
    }
    const index = new Map(slots.map((s, i) => [s.key, i]));
    for (const p of report?.series ?? []) {
      const i = index.get(key(new Date(p.ts * 1000)));
      if (i != null) slots[i].btc += p.userBtc;
    }
    return slots.map((s) => ({
      ...s,
      value: inMoney ? s.btc * btcPrice! : s.btc,
    }));
  })();

  const max = Math.max(...buckets.map((b) => b.value), 0);
  // Enough decimals that axis ticks never repeat (e.g. 0,0001 twice).
  const digits =
    max === 0
      ? 2
      : Math.min(8, Math.max(2, -Math.floor(Math.log10(max / 3)) + 1));
  const axisFmt = (v: number) =>
    inMoney
      ? v.toLocaleString(locale, {
          minimumFractionDigits: 0,
          maximumFractionDigits: digits,
        })
      : btc(v);
  const valueFmt = (v: number) => (inMoney ? fmt(v) : btc(v));

  return (
    <Card>
      <h2 className="mb-4 flex items-center gap-1.5 text-[15px] font-medium">
        {t(span === "24h" ? "stats.chart.byHour" : "stats.chart.byDay")}
        <Explain term="estimate" align="left" />
      </h2>
      <div className="h-52 w-full">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={buckets}
            margin={{ top: 4, right: 0, left: 0, bottom: 0 }}
            barCategoryGap={span === "30d" ? "18%" : "28%"}
          >
            <defs>
              <linearGradient id="bar" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--accent)" stopOpacity={1} />
                <stop
                  offset="100%"
                  stopColor="var(--accent)"
                  stopOpacity={0.55}
                />
              </linearGradient>
            </defs>
            <CartesianGrid
              stroke="var(--border)"
              strokeDasharray="3 6"
              vertical={false}
            />
            <XAxis
              dataKey="label"
              {...axis}
              interval={span === "30d" ? 4 : span === "24h" ? 3 : 0}
            />
            <YAxis
              {...axis}
              width={inMoney ? 52 : 64}
              tickCount={4}
              tickFormatter={axisFmt}
            />
            <Tooltip
              {...tooltip}
              labelFormatter={(_, p) =>
                (p?.[0]?.payload as { full?: string } | undefined)?.full ?? ""
              }
              formatter={(v) => [valueFmt(Number(v)), t("stats.earned")]}
            />
            <Bar
              dataKey="value"
              radius={[6, 6, 2, 2]}
              maxBarSize={36}
              background={{ fill: "var(--surface-2)", radius: 6 }}
              isAnimationActive
              animationDuration={500}
            >
              {buckets.map((b) => (
                <Cell
                  key={b.key}
                  fill="url(#bar)"
                  fillOpacity={b.current ? 1 : 0.7}
                />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>
    </Card>
  );
}
