import { Lightbulb, Sparkles } from "lucide-react";
import { useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { useCoinChange } from "./CoinChange";
import { CoinIcon } from "./CoinIcon";
import { payoutMode } from "./PayoutChoice";
import { Button, cx, Explain } from "./ui";

/** Below this gain the current setup counts as "already the best". */
const MIN_GAIN = 5;

/**
 * What is being mined and Erebor's advice for the most BTC on this PC: the coin (once
 * measured) and the payout. Manual coins and Monero payouts always get a warning.
 */
export function CoinAdvice({
  onChange,
  card = true,
  warnOnly,
}: {
  onChange?: () => void;
  card?: boolean;
  warnOnly?: boolean;
}) {
  const t = useT();
  const { config, status, setPage } = useApp();
  const { ask, dialog } = useCoinChange();
  if (!config || !status) return <div className="skeleton h-24" />;
  const a = status.advice;
  const coin = status.coin;
  const auto = status.autoCoin;
  const mine = payoutMode(config.wallet);
  const gain =
    a.btcPerDay != null && a.currentBtcPerDay
      ? Math.round((a.btcPerDay / a.currentBtcPerDay - 1) * 100)
      : null;
  const better = gain != null && gain >= MIN_GAIN;
  const device = t(
    a.device === "gpu" ? "advice.device.gpu" : "advice.device.cpu",
  );
  const toWallet = {
    label: t("advice.changePayout"),
    run: () => setPage("wallet"),
  };

  let tip: {
    warn: boolean;
    text: string;
    action?: { label: string; run: () => void };
  };
  if (mine === "monero") {
    tip = {
      warn: true,
      text: t(a.device === "gpu" ? "advice.monero.gpu" : "advice.monero.cpu"),
      action: toWallet,
    };
  } else if (!auto) {
    tip = {
      warn: true,
      text:
        a.coin && a.coin.id !== coin.id && better
          ? t("advice.manual.better", {
              c: coin.name,
              b: a.coin.name,
              g: gain!,
            })
          : t("advice.manual", { c: coin.name }),
      action: {
        label: t("advice.backToAuto"),
        run: () => ask("AUTO", t("coin.auto")),
      },
    };
  } else if (a.coin && a.payout !== mine && better) {
    tip = {
      warn: true,
      text: t("advice.payout.better", { p: t(`payout.${a.payout}`), g: gain! }),
      action: toWallet,
    };
  } else {
    tip = {
      warn: false,
      text: a.coin
        ? t("advice.best", {
            c: a.coin.name,
            d: device,
            p: t(`advice.via.${mine}`),
          })
        : t("advice.measuring", { d: device, p: t(`advice.via.${mine}`) }),
    };
  }

  if (!card && warnOnly && !tip.warn) return dialog;
  return (
    <div className="space-y-3">
      {card && (
        <div className="flex items-center gap-3.5 rounded-2xl border border-line bg-surface p-3.5">
          <span className="relative shrink-0">
            <CoinIcon id={coin.id} size={40} />
            {auto && (
              <span className="absolute -right-1 -bottom-1 flex size-5 items-center justify-center rounded-full border-2 border-surface bg-accent-solid text-accent-fg">
                <Sparkles className="size-3" />
              </span>
            )}
          </span>
          <span className="min-w-0 flex-1">
            <span className="flex items-center gap-2 text-[15px] font-medium">
              <span className="truncate">{coin.name}</span>
              <span
                className={cx(
                  "shrink-0 text-xs font-normal",
                  auto ? "text-accent" : "text-warn",
                )}
              >
                {t(auto ? "coin.auto.chip" : "coin.manual.chip")}
              </span>
            </span>
            <span className="mt-0.5 block truncate text-xs text-faint">
              {t(coin.device === "gpu" ? "coins.gpu" : "coins.cpu")} ·{" "}
              {coin.algo}
            </span>
          </span>
          {onChange && (
            <Button
              variant="ghost"
              className="border border-line"
              onClick={onChange}
            >
              {t("advice.change")}
            </Button>
          )}
        </div>
      )}
      {(!warnOnly || tip.warn) && (
        <div
          className={cx(
            "flex flex-wrap items-start gap-3 rounded-xl border px-4 py-3 text-[13px] leading-relaxed",
            tip.warn
              ? "border-warn/30 bg-warn/10"
              : "border-accent/30 bg-accent-soft",
          )}
          role="status"
        >
          <Lightbulb
            className={cx(
              "mt-0.5 size-4 shrink-0",
              tip.warn ? "text-warn" : "text-accent",
            )}
            aria-hidden
          />
          <p className="min-w-0 flex-1">
            <span className="font-medium">{t("advice.title")}</span> {tip.text}
          </p>
          <Explain term="auto" align="right" />
          {tip.action && (
            <Button
              variant="primary"
              className="w-full justify-center sm:w-auto"
              onClick={tip.action.run}
            >
              {tip.action.label}
            </Button>
          )}
        </div>
      )}
      {dialog}
    </div>
  );
}
