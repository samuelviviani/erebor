import {
  Bitcoin,
  Check,
  Coins,
  ExternalLink,
  Wallet,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";
import { openExternal } from "../lib/api";
import { useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { WalletInput } from "./shared";
import { cx, Explain } from "./ui";

export type PayoutMode = "kryptex" | "bitcoin" | "convert" | "monero";

/** Coins unMineable pays out besides BTC (same list as `wallet::UNMINEABLE_PAYOUTS`). */
export const CONVERT_COINS = ["LTC", "DOGE", "USDT"] as const;
/** What the user ends up holding: Bitcoin, or the coins as mined. */
type Pays = "btc" | "coin";

/**
 * Payout methods as data: a new service (another pool account, another auto-exchange,
 * another coin's own wallet) is one more entry here plus its texts and validator.
 */
const ALL_METHODS: {
  id: PayoutMode;
  pays: Pays;
  icon: LucideIcon;
  recommended?: boolean;
  signup?: boolean;
  /** Only offered to experienced miners (or when already in use). */
  advanced?: boolean;
}[] = [
  // BTC straight to the user's own wallet, no account: the default.
  { id: "bitcoin", pays: "btc", icon: Bitcoin, recommended: true },
  { id: "kryptex", pays: "btc", icon: Wallet, signup: true },
  // Same unMineable route, paid in a coin with a low payout threshold.
  { id: "convert", pays: "btc", icon: Coins, advanced: true },
  // Coins paid in themselves: Coins & wallets › the coin's sheet (own wallet).
];

/** Recognise what was pasted; `null` when it doesn't look like anything known yet. */
function detect(wallet: string): PayoutMode | null {
  const w = wallet.trim();
  if (/^krx/i.test(w)) return "kryptex";
  if (/^(LTC|DOGE|USDT):/i.test(w)) return "convert";
  if (/^(bc1|tb1|[13])/i.test(w)) return "bitcoin";
  if (/^[48]/.test(w)) return "monero";
  return null;
}

export function payoutMode(wallet: string): PayoutMode {
  return detect(wallet) ?? "bitcoin";
}

/**
 * "Where do you want your earnings?" in two plain steps: Bitcoin or the mined coins, then
 * the service. The field recognises what is pasted and switches to the matching method.
 */
export function PayoutChoice({
  value,
  onChange,
  onValid,
  advanced = false,
}: {
  value: string;
  onChange: (v: string) => void;
  onValid?: (ok: boolean) => void;
  /** Offer the methods for experienced miners too (never during onboarding). */
  advanced?: boolean;
}) {
  const t = useT();
  const info = useApp((s) => s.info);
  const [initial] = useState(() => payoutMode(value));
  const METHODS = ALL_METHODS.filter(
    (m) => advanced || !m.advanced || m.id === initial,
  );
  const [mode, setMode] = useState<PayoutMode>(() =>
    METHODS.some((m) => m.id === initial) ? initial : METHODS[0].id,
  );
  const pays = METHODS.find((m) => m.id === mode)!.pays;
  const methods = METHODS.filter((m) => m.pays === pays);
  const method = METHODS.find((m) => m.id === mode)!;
  // "LTC:address": the coin is picked apart from the address the user pastes.
  const [coin, setCoin] = useState<string>(() =>
    mode === "convert" ? value.split(":")[0].toUpperCase() : CONVERT_COINS[0],
  );
  const address = mode === "convert" ? value.replace(/^[A-Za-z]+:/, "") : value;

  const pick = (m: PayoutMode) => {
    setMode(m);
    if (value && detect(value) !== m) onChange("");
  };
  const type = (v: string) => {
    const d = detect(v);
    // A full "LTC:address" picks its coin; in the coin's field everything else is
    // that coin's address (a Litecoin "3…" address is not Bitcoin here).
    if (d === "convert") setCoin(v.split(":")[0].toUpperCase());
    else if (mode === "convert") return onChange(v ? `${coin}:${v}` : "");
    if (d && d !== mode && METHODS.some((m) => m.id === d)) setMode(d);
    onChange(v);
  };
  const pickCoin = (c: string) => {
    setCoin(c);
    // An address is only valid for its own coin: start over.
    onChange("");
  };

  // A single method: just the field, no choice to make. Step 1 only when methods
  // pay in different things.
  const choose = METHODS.length > 1;
  const choosePays = new Set(METHODS.map((m) => m.pays)).size > 1;
  return (
    <div className="space-y-5">
      {/* Step 1: what you want to receive */}
      {choosePays && (
        <div
          role="radiogroup"
          aria-label={t("payout.pays.title")}
          className="grid grid-cols-2 gap-2"
        >
          {(["btc", "coin"] as Pays[]).map((p) => {
            const on = pays === p;
            const Icon = p === "btc" ? Bitcoin : Coins;
            return (
              <button
                key={p}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() =>
                  !on && pick(METHODS.find((m) => m.pays === p)!.id)
                }
                className={cx(
                  "rounded-2xl border p-3.5 text-left transition-colors",
                  on
                    ? "border-accent bg-accent-soft"
                    : "border-line bg-surface hover:border-faint",
                )}
              >
                <span className="flex items-center gap-2">
                  <Icon
                    className={cx(
                      "size-[18px]",
                      on ? "text-accent" : "text-faint",
                    )}
                  />
                  <span className="text-sm font-medium">
                    {t(`payout.pays.${p}`)}
                  </span>
                  {p === "btc" && (
                    <span className="ml-auto rounded-lg bg-accent-solid px-1.5 py-0.5 text-2xs font-semibold text-accent-fg">
                      {t("payout.kryptex.badge")}
                    </span>
                  )}
                </span>
                <span className="mt-1 block text-xs leading-relaxed text-muted">
                  {t(`payout.pays.${p}.desc`)}
                </span>
              </button>
            );
          })}
        </div>
      )}

      {/* Step 2: which service */}
      {choose && (
        <div>
          <p className="mb-2 flex items-center gap-1.5 text-sm font-medium text-fg">
            {t("payout.via")}
            <Explain term={pays === "btc" ? "btc" : "wallet"} align="left" />
          </p>
          <div
            role="radiogroup"
            aria-label={t("payout.via")}
            className="divide-y divide-line overflow-hidden rounded-2xl border border-line bg-surface"
          >
            {methods.map(({ id, icon: Icon, recommended }) => {
              const on = mode === id;
              return (
                <button
                  key={id}
                  type="button"
                  role="radio"
                  aria-checked={on}
                  onClick={() => pick(id)}
                  className={cx(
                    "flex w-full items-center gap-3 px-3.5 py-3 text-left transition-colors",
                    on ? "bg-accent-soft/60" : "hover:bg-surface-2/60",
                  )}
                >
                  <Icon
                    className={cx(
                      "size-[18px] shrink-0",
                      on ? "text-accent" : "text-faint",
                    )}
                  />
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-2 text-sm font-medium">
                      {t(`payout.${id}`)}
                      {recommended && (
                        <span className="text-2xs font-normal text-accent">
                          {t("payout.kryptex.badge")}
                        </span>
                      )}
                    </span>
                    <span className="mt-0.5 block text-xs leading-relaxed text-muted">
                      {t(`payout.${id}.desc`)}
                    </span>
                  </span>
                  <span
                    className={cx(
                      "flex size-5 shrink-0 items-center justify-center rounded-full border",
                      on
                        ? "border-accent bg-accent-solid text-accent-fg"
                        : "border-line",
                    )}
                  >
                    {on && <Check className="size-3" />}
                  </span>
                </button>
              );
            })}
          </div>
          {pays === "coin" && (
            <p className="mt-2 text-xs leading-relaxed text-faint">
              {t("payout.coin.more")}
            </p>
          )}
        </div>
      )}

      {mode === "convert" && (
        <div
          role="radiogroup"
          aria-label={t("payout.convert.coin")}
          className="flex flex-wrap gap-2"
        >
          {CONVERT_COINS.map((c) => (
            <button
              key={c}
              type="button"
              role="radio"
              aria-checked={coin === c}
              onClick={() => coin !== c && pickCoin(c)}
              className={cx(
                "rounded-full border px-3 py-1 text-xs font-medium transition-colors",
                coin === c
                  ? "border-accent bg-accent-soft text-fg"
                  : "border-line text-muted hover:border-faint",
              )}
            >
              {t(`payout.convert.${c}`)}
            </button>
          ))}
        </div>
      )}

      <div>
        <WalletInput
          value={address}
          prefix={mode === "convert" ? `${coin}:` : ""}
          onChange={type}
          onValid={onValid}
          label={t(`payout.${mode}.field`, { c: coin })}
          placeholder={t(
            mode === "convert"
              ? `payout.convert.${coin}.placeholder`
              : `payout.${mode}.placeholder`,
          )}
          hint={t(`payout.${mode}.hint`)}
        />
        {(mode === "bitcoin" || mode === "convert") && info && (
          <p className="mt-2 text-xs leading-relaxed text-faint">
            {t("payout.referral", { code: info.unmineableReferral })}
          </p>
        )}
        {method.signup && !value && (
          <button
            type="button"
            onClick={() =>
              openExternal(info?.kryptexSignupUrl ?? "https://www.kryptex.com/")
            }
            className="mt-2 inline-flex items-center gap-1.5 text-xs text-accent hover:underline"
          >
            {t(`payout.${mode}.signup`)} <ExternalLink className="size-3.5" />
          </button>
        )}
      </div>
    </div>
  );
}
