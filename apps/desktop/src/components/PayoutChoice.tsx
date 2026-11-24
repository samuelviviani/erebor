import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Bitcoin,
  Check,
  Coins,
  ExternalLink,
  Wallet,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";
import { inApp } from "../lib/api";
import { useT } from "../lib/i18n";
import { WalletInput } from "./shared";
import { cx, Explain } from "./ui";

export type PayoutMode = "kryptex" | "bitcoin" | "monero";
/** What the user ends up holding: Bitcoin, or the coins as mined. */
type Pays = "btc" | "coin";

/**
 * Payout methods as data: a new service (another pool account, another auto-exchange,
 * another coin's own wallet) is one more entry here plus its texts and validator.
 */
const METHODS: {
  id: PayoutMode;
  pays: Pays;
  icon: LucideIcon;
  recommended?: boolean;
  signup?: string;
}[] = [
  {
    id: "kryptex",
    pays: "btc",
    icon: Wallet,
    recommended: true,
    signup: "https://www.kryptex.com/",
  },
  // Kryptex only for now (the backend refuses the others): they come back here.
  // { id: "bitcoin", pays: "btc", icon: Bitcoin },
  // { id: "monero", pays: "coin", icon: ShieldHalf },
];

/** Recognise what was pasted; `null` when it doesn't look like anything known yet. */
function detect(wallet: string): PayoutMode | null {
  const w = wallet.trim();
  if (/^krx/i.test(w)) return "kryptex";
  if (/^(bc1|tb1|[13])/i.test(w)) return "bitcoin";
  if (/^[48]/.test(w)) return "monero";
  return null;
}

export function payoutMode(wallet: string): PayoutMode {
  return detect(wallet) ?? "kryptex";
}

const open = (url: string) =>
  inApp()
    ? openUrl(url).catch(() => {})
    : window.open(url, "_blank", "noopener");

/**
 * "Where do you want your earnings?" in two plain steps: Bitcoin or the mined coins, then
 * the service. The field recognises what is pasted and switches to the matching method.
 */
export function PayoutChoice({
  value,
  onChange,
  onValid,
}: {
  value: string;
  onChange: (v: string) => void;
  onValid?: (ok: boolean) => void;
}) {
  const t = useT();
  const [mode, setMode] = useState<PayoutMode>(() =>
    METHODS.some((m) => m.id === payoutMode(value))
      ? payoutMode(value)
      : METHODS[0].id,
  );
  const pays = METHODS.find((m) => m.id === mode)!.pays;
  const methods = METHODS.filter((m) => m.pays === pays);
  const method = METHODS.find((m) => m.id === mode)!;

  const pick = (m: PayoutMode) => {
    setMode(m);
    if (value && detect(value) !== m) onChange("");
  };
  const type = (v: string) => {
    const d = detect(v);
    if (d && d !== mode && METHODS.some((m) => m.id === d)) setMode(d);
    onChange(v);
  };

  // A single method: just the field, no choice to make.
  const choose = METHODS.length > 1;
  return (
    <div className="space-y-5">
      {/* Step 1: what you want to receive */}
      {choose && (
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
                    <span className="ml-auto rounded-md bg-accent-solid px-1.5 py-0.5 text-[10px] font-semibold text-accent-fg">
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
          <p className="mb-2 flex items-center gap-1.5 text-xs font-medium tracking-wide text-faint uppercase">
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
                        <span className="text-[11px] font-normal text-accent">
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

      <div>
        <WalletInput
          value={value}
          onChange={type}
          onValid={onValid}
          label={t(`payout.${mode}.field`)}
          placeholder={t(`payout.${mode}.placeholder`)}
          hint={t(`payout.${mode}.hint`)}
        />
        {method.signup && !value && (
          <button
            type="button"
            onClick={() => open(method.signup!)}
            className="mt-2 inline-flex items-center gap-1.5 text-xs text-accent hover:underline"
          >
            {t(`payout.${mode}.signup`)} <ExternalLink className="size-3.5" />
          </button>
        )}
      </div>
    </div>
  );
}
