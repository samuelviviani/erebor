// Display formatting. `null`/`undefined` always renders as "—" so missing data is
// never confused with zero.
export const DASH = "—";

export function hashrate(h: number | null | undefined): string {
  if (h == null || !Number.isFinite(h)) return DASH;
  if (h >= 1e12) return `${(h / 1e12).toFixed(2)} TH/s`;
  if (h >= 1e9) return `${(h / 1e9).toFixed(2)} GH/s`;
  if (h >= 1e6) return `${(h / 1e6).toFixed(2)} MH/s`;
  if (h >= 1e3) return `${(h / 1e3).toFixed(2)} kH/s`;
  return `${h.toFixed(0)} H/s`;
}

export function duration(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(secs) || secs < 0) return DASH;
  const s = Math.floor(secs);
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s % 60}s`;
  return `${s % 60}s`;
}

/** XMR with enough precision for tiny CPU-mining amounts. */
export function xmr(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return DASH;
  if (v === 0) return "0 XMR";
  const abs = Math.abs(v);
  const digits = abs >= 1 ? 4 : abs >= 0.001 ? 6 : 8;
  return `${v.toFixed(digits)} XMR`;
}

/** Bitcoin amount; tiny amounts in satoshi (1 BTC = 100,000,000 sat) so they stay readable. */
export function btc(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return DASH;
  if (v === 0) return "0 BTC";
  const sats = v * 1e8;
  if (Math.abs(v) < 0.001) return `${sats < 10 ? sats.toFixed(1) : Math.round(sats).toLocaleString()} sat`;
  return `${v.toFixed(6)} BTC`;
}

/** Bitcoin with all 8 decimals, never in satoshis: the unit people see on Kryptex. */
export function btcFull(v: number | null | undefined, locale = "it-IT"): string {
  if (v == null || !Number.isFinite(v)) return DASH;
  return `${v.toLocaleString(locale, { minimumFractionDigits: 8, maximumFractionDigits: 8 })} BTC`;
}

/** Coin amount with enough significant digits to be meaningful. */
export function coinAmount(v: number, ticker: string): string {
  if (!Number.isFinite(v)) return DASH;
  if (v === 0) return `0 ${ticker}`;
  const abs = Math.abs(v);
  const digits = abs >= 100 ? 2 : abs >= 1 ? 4 : Math.min(10, Math.max(4, -Math.floor(Math.log10(abs)) + 3));
  return `${v.toFixed(digits)} ${ticker}`;
}

export function money(v: number | null | undefined, currency: string, locale = "it-IT"): string {
  if (v == null || !Number.isFinite(v)) return DASH;
  const abs = Math.abs(v);
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: abs < 1 ? 4 : 2,
  }).format(v);
}

export function bytes(b: number | null | undefined): string {
  if (b == null || !Number.isFinite(b)) return DASH;
  const gb = b / 1024 ** 3;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(b / 1024 ** 2).toFixed(0)} MB`;
}

export function percent(v: number | null | undefined, digits = 0): string {
  if (v == null || !Number.isFinite(v)) return DASH;
  return `${v.toFixed(digits)}%`;
}

export function dateTime(unix: number | null | undefined, locale = "it-IT"): string {
  if (unix == null) return DASH;
  return new Date(unix * 1000).toLocaleString(locale, {
    dateStyle: "short",
    timeStyle: "short",
  });
}

export function minutesToTime(m: number): string {
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

export function timeToMinutes(t: string): number {
  const [h, m] = t.split(":").map(Number);
  return (h || 0) * 60 + (m || 0);
}

export function shortAddress(a: string): string {
  return a.length > 20 ? `${a.slice(0, 10)}…${a.slice(-8)}` : a;
}
