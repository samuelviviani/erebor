import { Check, ChevronDown, ChevronRight, Sparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { money } from "../lib/format";
import { useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { useCoinChange, useCoinOptions } from "./CoinChange";
import { CoinIcon } from "./CoinIcon";
import { cx } from "./ui";

/** How many coins the menu lists; the rest are on the Coins page. */
const TOP = 6;

/**
 * The coin in Home's title, as a menu: automatic first, then the coins this PC can mine,
 * best paid first, with what each earns a day. Choosing one asks before restarting the miner.
 */
export function CoinPicker() {
  const t = useT();
  const { config, status, setPage } = useApp();
  const options = useCoinOptions();
  const { ask, dialog } = useCoinChange();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) =>
      ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);
  if (!config || !status) return null;
  const coin = status.coin;
  const auto = status.autoCoin;
  const locale = config.language === "it" ? "it-IT" : "en-US";
  const currency = config.energy.currency ?? "EUR";
  const list = options
    .filter((o) => !o.blocker && !o.coin.asic)
    .sort((a, b) => (b.valuePerDay ?? -1) - (a.valuePerDay ?? -1))
    .slice(0, TOP);
  const pick = (id: string, name: string) => {
    setOpen(false);
    ask(id, name);
  };
  const item =
    "flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left text-sm transition-colors hover:bg-surface-3/70 focus-visible:bg-surface-3/70 focus-visible:outline-none";

  return (
    <span ref={ref} className="relative inline-block align-bottom">
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`${coin.name} · ${t("coin.change")}`}
        onClick={() => setOpen(!open)}
        className="press -mx-1.5 inline-flex items-center gap-2 rounded-xl px-1.5 transition-colors hover:bg-surface-2"
      >
        <span className="relative">
          <CoinIcon id={coin.id} size={30} />
          {auto && (
            <span className="absolute -right-1 -bottom-1 flex size-4 items-center justify-center rounded-full border-2 border-bg bg-accent-solid text-accent-fg">
              <Sparkles className="size-2.5" aria-hidden />
            </span>
          )}
        </span>
        {coin.name}
        <ChevronDown
          className={cx("size-5 text-faint transition-transform duration-200", open && "rotate-180")}
          aria-hidden
        />
      </button>

      {open && (
        <span
          role="menu"
          aria-label={t("coin.change")}
          className="pop-in float absolute top-full left-0 z-40 mt-2 block w-80 rounded-2xl p-1.5 font-sans text-base font-normal tracking-normal"
        >
          <button type="button" role="menuitemradio" aria-checked={auto} onClick={() => pick("AUTO", t("coin.auto"))} className={item}>
            <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-accent-solid text-accent-fg">
              <Sparkles className="size-3.5" aria-hidden />
            </span>
            <span className="min-w-0 flex-1">
              <span className="block font-medium">{t("coin.auto")}</span>
              <span className="block text-xs text-faint">{t("coin.auto.menu")}</span>
            </span>
            {auto && <Check className="size-4 shrink-0 text-accent" aria-hidden />}
          </button>
          <span aria-hidden className="mx-3 my-1 block border-t border-line" />
          {list.map((o) => {
            const on = !auto && o.coin.id === coin.id;
            return (
              <button
                key={o.coin.id}
                type="button"
                role="menuitemradio"
                aria-checked={on}
                onClick={() => pick(o.coin.id, o.coin.name)}
                className={item}
              >
                <CoinIcon id={o.coin.id} size={24} />
                <span className="min-w-0 flex-1 truncate">
                  {o.coin.name} <span className="text-xs text-faint">{o.coin.id}</span>
                </span>
                <span className="num shrink-0 text-xs text-muted">
                  {o.valuePerDay != null ? `≈ ${money(o.valuePerDay, currency, locale)}` : "—"}
                </span>
                <Check className={cx("size-4 shrink-0 text-accent", !on && "invisible")} aria-hidden />
              </button>
            );
          })}
          <span aria-hidden className="mx-3 my-1 block border-t border-line" />
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              setPage("coins");
            }}
            className={cx(item, "text-accent")}
          >
            <span className="flex-1">{t("coin.all")}</span>
            <ChevronRight className="size-4" aria-hidden />
          </button>
        </span>
      )}
      {/* The confirmation lives outside the heading it was opened from. */}
      {createPortal(dialog, document.body)}
    </span>
  );
}
