import { Check, Cpu, Gpu, Sparkles } from "lucide-react";
import { useState, type ReactNode } from "react";
import { CoinAdvice } from "../components/Advice";
import { useCoinChange, useCoinOptions } from "../components/CoinChange";
import { CoinIcon } from "../components/CoinIcon";
import { PageTitle } from "../components/Layout";
import { cx, Explain, Notice, Segmented } from "../components/ui";
import type { CoinOption } from "../lib/api";
import { hashrate, money } from "../lib/format";
import { useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/**
 * What to mine. Automatic: Erebor picks, and no coin list is shown. "I choose": every
 * coin as a tidy list, with a warning that Erebor will no longer switch on its own.
 */
export default function Coins() {
  const t = useT();
  const config = useApp((s) => s.config);
  const options = useCoinOptions();
  const { ask, dialog } = useCoinChange();
  const locale = config?.language === "en" ? "en-US" : "it-IT";
  const currency = config?.energy.currency ?? "EUR";
  const selected = config?.coin ?? "AUTO";
  // "I choose" can be opened before picking: nothing is saved until a coin is chosen.
  const [manual, setManual] = useState(selected !== "AUTO");
  const showList = manual || selected !== "AUTO";
  const byValue = (a: CoinOption, b: CoinOption) =>
    !!a.blocker !== !!b.blocker
      ? a.blocker
        ? 1
        : -1
      : (b.valuePerDay ?? -1) - (a.valuePerDay ?? -1);

  const group = (device: "gpu" | "cpu") => {
    const list = options.filter((o) => o.coin.device === device).sort(byValue);
    if (!list.length) return null;
    return (
      <section>
        <h2 className="mb-2 flex items-center gap-2 px-1 text-sm font-medium text-muted">
          {device === "gpu" ? (
            <Gpu className="size-4" />
          ) : (
            <Cpu className="size-4" />
          )}
          {t(device === "gpu" ? "coins.gpu" : "coins.cpu")}
          <Explain term="gpu" align="left" />
        </h2>
        <div
          className="divide-y divide-line overflow-hidden rounded-2xl border border-line bg-surface"
          role="radiogroup"
          aria-label={t(device === "gpu" ? "coins.gpu" : "coins.cpu")}
        >
          {list.map((o) => (
            <Row
              key={o.coin.id}
              selected={selected === o.coin.id}
              disabled={!!o.blocker}
              onClick={() => ask(o.coin.id, o.coin.name)}
              icon={<CoinIcon id={o.coin.id} size={40} />}
              title={o.coin.name}
              tag={o.coin.id}
              sub={
                o.blocker
                  ? t(`coin.blocker.${o.blocker}`)
                  : `${t("coin.algoLabel")} ${o.coin.algo}${o.minerInstalled ? "" : ` · ${t("coin.needsMiner")}`}`
              }
              aside={
                o.blocker ? null : (
                  <>
                    <span className="block text-[15px] text-fg">
                      {o.valuePerDay != null
                        ? `≈ ${money(o.valuePerDay, currency, locale)}`
                        : "—"}
                    </span>
                    <span className="block text-xs text-faint">
                      {o.hashrate != null
                        ? `${t("home.perDay")} · ${hashrate(o.hashrate)}`
                        : t("coin.notMeasured")}
                    </span>
                  </>
                )
              }
            />
          ))}
        </div>
      </section>
    );
  };

  const segmented = (
    <Segmented
      label={t("coins.title")}
      value={showList ? "manual" : "auto"}
      onChange={(v) => {
        if (v === "auto") {
          setManual(false);
          if (selected !== "AUTO") ask("AUTO", t("coin.auto"));
        } else setManual(true);
      }}
      options={[
        { value: "auto", label: t("coin.auto") },
        { value: "manual", label: t("coins.manual") },
      ]}
    />
  );

  return (
    <>
      <PageTitle sub={t("coins.intro")} action={segmented}>
        {t("coins.title")}
      </PageTitle>
      {showList ? (
        <div className="space-y-8">
          <Notice tone="warn">{t("coins.manual.warn")}</Notice>
          {selected !== "AUTO" && <CoinAdvice card={false} warnOnly />}
          {group("gpu")}
          {group("cpu")}
        </div>
      ) : (
        <div className="space-y-4">
          <AutoCard options={options} />
          <CoinAdvice card={false} warnOnly />
        </div>
      )}
      {dialog}
    </>
  );
}

/**
 * Automatic mode at a glance: the coin being mined now, and every coin this PC can mine
 * with where Erebor is at (to try, measured, best).
 */
function AutoCard({ options }: { options: CoinOption[] }) {
  const t = useT();
  const { config, status } = useApp();
  const locale = config?.language === "en" ? "en-US" : "it-IT";
  const currency = config?.energy.currency ?? "EUR";
  const coin = status?.coin;
  const best = status?.advice.coin?.id;
  const usable = options
    .filter((o) => !o.blocker)
    .sort((a, b) => (b.valuePerDay ?? -1) - (a.valuePerDay ?? -1));
  const perDay = status?.projection?.valuePerDay;
  return (
    <section className="overflow-hidden rounded-2xl border border-line bg-surface">
      <div className="flex items-center gap-4 p-5">
        <span className="relative shrink-0">
          {coin ? (
            <CoinIcon id={coin.id} size={52} />
          ) : (
            <span className="block size-13" />
          )}
          <span className="absolute -right-1 -bottom-1 flex size-6 items-center justify-center rounded-full border-2 border-surface bg-accent-solid text-accent-fg">
            <Sparkles className="size-3.5" />
          </span>
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-xs font-medium tracking-wide text-faint uppercase">
            {t("coins.auto.now")}
          </p>
          <p className="mt-0.5 truncate text-xl font-semibold">
            {coin?.name ?? "—"}
          </p>
          {coin && (
            <p className="mt-0.5 truncate text-xs text-faint">
              {t(coin.device === "gpu" ? "coins.gpu" : "coins.cpu")} ·{" "}
              {coin.algo}
            </p>
          )}
        </div>
        {perDay != null && (
          <div className="num shrink-0 text-right">
            <p className="text-lg">≈ {money(perDay, currency, locale)}</p>
            <p className="text-xs text-faint">{t("home.perDay")}</p>
          </div>
        )}
      </div>
      <div className="border-t border-line bg-bg/40 px-5 py-4">
        <p className="mb-3 flex items-center gap-1.5 text-xs text-faint">
          {t("coins.auto.candidates")}
          <Explain term="auto" align="left" />
        </p>
        <ul className="flex flex-wrap gap-2">
          {usable.map((o) => {
            const state =
              o.coin.id === best
                ? "best"
                : o.hashrate != null
                  ? "measured"
                  : "pending";
            return (
              <li
                key={o.coin.id}
                className={cx(
                  "flex items-center gap-2 rounded-full border py-1 pr-3 pl-1 text-[13px]",
                  state === "best"
                    ? "border-accent/40 bg-accent-soft"
                    : "border-line bg-surface",
                )}
              >
                <CoinIcon id={o.coin.id} size={22} />
                <span className="font-medium">{o.coin.name}</span>
                <span
                  className={cx(
                    "text-xs",
                    state === "best" ? "text-accent" : "text-faint",
                  )}
                >
                  {t(`coins.auto.${state}`)}
                </span>
              </li>
            );
          })}
        </ul>
      </div>
    </section>
  );
}

function Row({
  icon,
  title,
  tag,
  sub,
  aside,
  selected,
  disabled,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  tag?: string;
  sub: ReactNode;
  aside?: ReactNode;
  selected: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <div
      className={cx(
        "relative flex items-center gap-4 px-4 py-3.5 transition-colors",
        selected ? "bg-accent-soft" : !disabled && "hover:bg-surface-2",
        disabled && "opacity-50",
      )}
    >
      <span className="shrink-0">{icon}</span>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          {/* Stretched button: the whole row is clickable. */}
          <button
            type="button"
            role="radio"
            aria-checked={selected}
            disabled={disabled}
            onClick={onClick}
            className="text-left text-[15px] font-medium after:absolute after:inset-0 disabled:cursor-not-allowed"
          >
            {title}
          </button>
          {tag && <span className="text-xs text-faint">{tag}</span>}
        </div>
        <p className="mt-0.5 text-[13px] leading-relaxed text-muted">{sub}</p>
      </div>
      {aside && <div className="num shrink-0 text-right">{aside}</div>}
      <Check
        className={cx("size-5 shrink-0 text-accent", !selected && "invisible")}
        aria-hidden
      />
    </div>
  );
}
