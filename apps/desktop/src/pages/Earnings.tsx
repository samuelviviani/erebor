import { CircleCheck, Clock, Download, Gauge } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { ChartPanel, type Metric } from "../components/fleet";
import { PageTitle } from "../components/Layout";
import { Button, cx, Segmented } from "../components/ui";
import { api, errorText, type StatsReport } from "../lib/api";
import { btc, dateTime, duration, hashrate, money } from "../lib/format";
import { translateError, useLocale, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

const RANGES = [
  { key: "24h", secs: 86_400 },
  { key: "7d", secs: 7 * 86_400 },
  { key: "30d", secs: 30 * 86_400 },
  { key: "90d", secs: 90 * 86_400 },
] as const;
type Range = (typeof RANGES)[number]["key"];

/** Start of the calendar month `back` months before the current one (local time), unix seconds. */
function monthStart(back: number) {
  const d = new Date();
  return new Date(d.getFullYear(), d.getMonth() - back, 1).getTime() / 1000;
}

/**
 * What this computer really did: totals for a period (mined, electricity, net), its
 * charts, month by month (the figures people need for their own books) and the sessions.
 * Everything comes from the engine's own samples; nothing is sent anywhere.
 */
export default function Earnings() {
  const t = useT();
  const locale = useLocale();
  const { config, toast, setPage } = useApp();
  const cur = config?.energy.currency ?? "EUR";
  const [range, setRange] = useState<Range>("7d");
  const [r, setR] = useState<StatsReport | null>(null);
  const [months, setMonths] = useState<{ start: number; r: StatsReport }[]>([]);
  const secs = RANGES.find((x) => x.key === range)!.secs;

  useEffect(() => {
    let alive = true;
    const load = () => api.getStats(secs).then((x) => alive && setR(x)).catch(() => {});
    load();
    const i = setInterval(load, 60_000);
    return () => {
      alive = false;
      clearInterval(i);
    };
  }, [secs]);

  useEffect(() => {
    // The current month and the five before it, each from its first day to the next one's.
    const now = Date.now() / 1000;
    Promise.all(
      [0, 1, 2, 3, 4, 5].map(async (back) => {
        const start = monthStart(back);
        const end = back === 0 ? now : monthStart(back - 1);
        return { start, r: await api.getStats(Math.max(60, end - start), end) };
      }),
    )
      .then((m) => setMonths(m.filter((x) => x.r.earnings.miningSecs > 0)))
      .catch(() => {});
  }, []);

  const saveCsv = async () => {
    try {
      const path = await api.saveCsv(366 * 86_400);
      toast("info", t("settings.csv.saved", { path }));
    } catch (e) {
      toast("error", translateError(config?.language ?? "en", errorText(e)));
    }
  };

  const e = r?.earnings;
  const pts = r?.series ?? [];
  const shares = r ? r.hash.accepted + r.hash.rejected : 0;
  const fmt = (v: number | null | undefined) => money(v, cur, locale);
  // The period's average price and tariff turn the per-bucket amounts into running totals.
  const price = e && e.netValue != null && e.netBtc > 0 ? e.netValue / e.netBtc : null;
  const tariff = e && e.energyCost != null && e.energyKwh ? e.energyCost / e.energyKwh : null;
  const metrics = useMemo((): Metric[] => {
    if (!e) return [];
    const ts = pts.map((p) => p.ts);
    let btcSum = 0;
    let kwhSum = 0;
    const mined: number[] = [];
    const net: (number | null)[] = [];
    for (const p of pts) {
      btcSum += p.userBtc;
      kwhSum += (p.energyWh ?? 0) / 1000;
      mined.push(price != null ? btcSum * price : btcSum);
      net.push(price != null && tariff != null ? btcSum * price - kwhSum * tariff : null);
    }
    const temps = pts.flatMap((p) => (p.tempC != null ? [p.tempC] : []));
    return [
      { key: "mined", label: t("earn.chart.mined"), ts, values: mined, fmt: price != null ? fmt : btc, now: e.netValue != null ? fmt(e.netValue) : btc(e.netBtc) },
      { key: "net", label: t("earn.net"), ts, values: net, fmt, now: e.netProfit != null ? fmt(e.netProfit) : undefined },
      { key: "hash", label: t("earn.avgSpeed"), ts, values: pts.map((p) => p.hashrate), fmt: hashrate, now: hashrate(r!.hash.avg) },
      { key: "temp", label: t("fleet.tempMax"), ts, values: pts.map((p) => p.tempC), fmt: (v: number) => `${Math.round(v)} °C`, now: temps.length ? `${Math.round(Math.max(...temps))} °C` : undefined },
    ];
  }, [r, price, tariff, t, locale, cur]);

  return (
    <div className="space-y-6">
      <PageTitle
        sub={t("earn.sub")}
        action={
          <Button onClick={saveCsv}>
            <Download className="size-4" aria-hidden /> {t("settings.csv.short")}
          </Button>
        }
      >
        {t("nav.stats")}
      </PageTitle>

      <Segmented label={t("earn.period")} value={range} onChange={setRange} options={RANGES.map((x) => ({ value: x.key, label: t(`earn.range.${x.key}`) }))} />

      {!e || e.miningSecs <= 0 ? (
        <section className="panel p-5">
          <p className="py-6 text-center text-sm text-faint">{t("earn.empty")}</p>
        </section>
      ) : (
        <>
          <section className="panel p-6">
            {/* The sum people care about, written out: mined − electricity = what is left. */}
            <div className="flex flex-wrap items-stretch gap-x-5 gap-y-4">
              <Term label={t("earn.mined")} value={e.netValue != null ? fmt(e.netValue) : btc(e.netBtc)} sub={e.netValue == null ? t("earn.noPrice") : e.netBtc > 0 ? btc(e.netBtc) : undefined} />
              <Op>−</Op>
              <Term
                label={t("earn.power")}
                value={e.energyCost != null ? fmt(e.energyCost) : "—"}
                sub={e.energyKwh != null ? `${e.energyKwh.toFixed(2)} kWh · ${t(`earn.source.${e.energySource}`)}` : t("earn.source.unavailable")}
              />
              <Op>=</Op>
              <Term
                big
                label={t("earn.net")}
                value={e.netProfit != null ? fmt(e.netProfit) : "—"}
                tone={e.netProfit == null ? undefined : e.netProfit >= 0 ? "text-go" : "text-stop"}
                sub={e.netProfit == null ? (
                  <button type="button" className="text-accent hover:underline" onClick={() => setPage("energy")}>
                    {t("earn.setPrice")}
                  </button>
                ) : undefined}
              />
            </div>
            <dl className="mt-6 grid gap-4 border-t border-line pt-5 text-sm sm:grid-cols-3">
              <Fact icon={<Clock />} label={t("earn.time")} value={duration(e.miningSecs)} sub={t("earn.fee", { d: duration(e.ereborFeeSecs) })} />
              <Fact icon={<Gauge />} label={t("earn.avgSpeed")} value={hashrate(r!.hash.avg)} sub={t("earn.maxSpeed", { v: hashrate(r!.hash.max) })} />
              <Fact
                icon={<CircleCheck />}
                label={t("earn.shares")}
                value={r!.hash.accepted.toLocaleString(locale)}
                sub={shares ? t("earn.rejected", { p: ((r!.hash.rejected / shares) * 100).toFixed(1) }) : undefined}
              />
            </dl>
            {!e.complete && <p className="mt-4 text-xs text-faint">{t("earn.incomplete")}</p>}
          </section>

          <ChartPanel key={range} metrics={metrics} label={t("earn.period")} long={secs > 86_400} note={t("earn.chart.note")} />
        </>
      )}

      {months.length > 0 && (
        <section className="panel overflow-x-auto p-5">
          <h2 className="mb-3 text-base font-semibold">{t("earn.months")}</h2>
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-faint">
              <tr>
                <th className="py-2 font-normal">{t("earn.month")}</th>
                <th className="py-2 text-right font-normal">{t("earn.time")}</th>
                <th className="py-2 text-right font-normal">{t("earn.mined")}</th>
                <th className="py-2 text-right font-normal">{t("earn.power")}</th>
                <th className="py-2 text-right font-normal">{t("earn.net")}</th>
              </tr>
            </thead>
            <tbody className="num">
              {months.map(({ start, r: m }) => (
                <tr key={start} className="border-t border-line">
                  <td className="py-2 capitalize">{new Date(start * 1000).toLocaleDateString(locale, { month: "long", year: "numeric" })}</td>
                  <td className="py-2 text-right">{duration(m.earnings.miningSecs)}</td>
                  <td className="py-2 text-right">
                    {btc(m.earnings.netBtc)}
                    {m.earnings.netValue != null && <span className="block text-2xs text-faint">{money(m.earnings.netValue, cur, locale)}</span>}
                  </td>
                  <td className="py-2 text-right">{money(m.earnings.energyCost, cur, locale)}</td>
                  <td className={m.earnings.netProfit != null && m.earnings.netProfit < 0 ? "py-2 text-right text-stop" : "py-2 text-right"}>
                    {money(m.earnings.netProfit, cur, locale)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="mt-3 text-xs text-faint">{t("earn.months.hint")}</p>
        </section>
      )}

      {r && r.sessions.length > 0 && (
        <section className="panel overflow-x-auto p-5">
          <h2 className="mb-3 text-base font-semibold">{t("earn.sessions")}</h2>
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-faint">
              <tr>
                <th className="py-2 font-normal">{t("earn.session.start")}</th>
                <th className="py-2 font-normal">{t("earn.session.length")}</th>
                <th className="py-2 font-normal">{t("earn.session.miner")}</th>
                <th className="py-2 text-right font-normal">{t("earn.session.end")}</th>
              </tr>
            </thead>
            <tbody>
              {r.sessions.slice(0, 20).map((s) => (
                <tr key={s.id} className="border-t border-line">
                  <td className="num py-2">{dateTime(s.started, locale)}</td>
                  <td className="num py-2">{s.ended ? duration(s.ended - s.started) : <span className="text-go">{t("earn.running")}</span>}</td>
                  <td className="py-2 text-muted">{s.miner}</td>
                  <td className="py-2 text-right text-faint">
                    {s.endReason ? (t(`earn.end.${s.endReason}`) === `earn.end.${s.endReason}` ? s.endReason.replaceAll("_", " ") : t(`earn.end.${s.endReason}`)) : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
    </div>
  );
}

/** One side of the sum: label, figure, and a line under it. */
function Term({ label, value, sub, tone, big }: { label: string; value: string; sub?: ReactNode; tone?: string; big?: boolean }) {
  return (
    <div className={cx("min-w-0", big ? "flex-[1.4] rounded-2xl bg-surface-2/70 px-4 py-3" : "flex-1 py-3")}>
      <div className="truncate text-xs text-faint">{label}</div>
      <div className={cx("num mt-1 truncate font-semibold tracking-tight", big ? "text-2xl" : "text-xl", tone)}>{value}</div>
      {sub && <div className="num mt-0.5 truncate text-xs text-faint">{sub}</div>}
    </div>
  );
}

function Op({ children }: { children: string }) {
  return (
    <span aria-hidden className="flex items-center text-xl text-faint max-sm:hidden">
      {children}
    </span>
  );
}

function Fact({ icon, label, value, sub }: { icon: ReactNode; label: string; value: string; sub?: string }) {
  return (
    <div className="flex min-w-0 gap-3">
      <span aria-hidden className="mt-0.5 text-faint [&>svg]:size-4">
        {icon}
      </span>
      <div className="min-w-0">
        <dt className="text-xs text-faint">{label}</dt>
        <dd className="num font-medium text-fg">{value}</dd>
        {sub && <dd className="num truncate text-xs text-faint">{sub}</dd>}
      </div>
    </div>
  );
}
