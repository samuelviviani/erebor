import {
  Bitcoin,
  Coins as CoinsIcon,
  Check,
  ChevronRight,
  Copy,
  Cpu,
  ExternalLink,
  Gpu,
  Plus,
  Server,
  Settings2,
  ShieldCheck,
  ShieldHalf,
  TriangleAlert,
  Wallet as WalletIcon,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import { CoinAdvice } from "../components/Advice";
import { useCoinChange, useCoinOptions } from "../components/CoinChange";
import { CoinIcon } from "../components/CoinIcon";
import {
  CoinSheet,
  PAYOUT_BLOCKERS,
  poolName,
  useCoinPools,
} from "../components/CoinWallets";
import { PageTitle } from "../components/Layout";
import {
  PayoutChoice,
  payoutMode,
  type PayoutMode,
} from "../components/PayoutChoice";
import { KRYPTEX_LOGIN } from "../components/shared";
import {
  Badge,
  Button,
  cx,
  Explain,
  Modal,
  Notice,
  Segmented,
} from "../components/ui";
import { errorText, openExternal, type CoinOption } from "../lib/api";
import { hashrate, money } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/**
 * Coins and wallets, one page: where earnings go by default (it decides which coins can
 * be mined), then every coin with where it is paid. A coin's row opens its sheet (own
 * wallet and pool, automatic choice); in "I choose" the row also selects the coin.
 */
export default function Coins() {
  const t = useT();
  const config = useApp((s) => s.config);
  const options = useCoinOptions();
  const pools = useCoinPools();
  const { ask, dialog } = useCoinChange();
  const [sheet, setSheet] = useState<CoinOption | null>(null);
  const c = config!;
  const locale = c.language === "en" ? "en-US" : "it-IT";
  const currency = c.energy.currency ?? "EUR";
  const selected = c.coin ?? "AUTO";
  // "I choose" can be opened before picking: nothing is saved until a coin is chosen.
  const [manual, setManual] = useState(selected !== "AUTO");
  const showManual = manual || selected !== "AUTO";
  const exclude = new Set(c.autoExclude ?? []);
  const best = useApp((s) => s.status?.advice.coin?.id);
  const mode = payoutMode(c.wallet);

  const destination = (o: CoinOption) => {
    const own = c.coinWallets?.[o.coin.id];
    if (own)
      return t("coins.dest.own", {
        c: o.coin.id,
        p: poolName(c.coinPools?.[o.coin.id], pools[o.coin.id] ?? []),
      });
    return c.wallet ? t(`coins.dest.${mode}`) : t("coins.dest.none");
  };

  // Mineable first, then coins that only need a wallet, then the ones this PC can't mine.
  const byValue = (a: CoinOption, b: CoinOption) =>
    rank(a) - rank(b) || (b.valuePerDay ?? -1) - (a.valuePerDay ?? -1);

  const now = useApp((s) => s.status?.coin.id);
  const sorted = (device: "gpu" | "cpu" | "asic") =>
    options
      .filter((o) =>
        device === "asic" ? o.coin.asic : o.coin.device === device && !o.coin.asic,
      )
      .sort(byValue);

  // One table per device: coin, where it is paid, what it earns a day on this PC.
  const table = (device: "gpu" | "cpu" | "asic", list: CoinOption[]) => (
    <div
      className="divide-y divide-line overflow-hidden rounded-2xl border border-line bg-surface shadow-[var(--highlight)]"
      role={showManual ? "radiogroup" : undefined}
      aria-label={t(`coins.${device}`)}
    >
      <div className="hidden grid-cols-[minmax(0,1.4fr)_minmax(0,1.2fr)_8rem_2.25rem] gap-4 px-4 py-2 text-xs text-faint sm:grid">
        <span>{t("coins.col.coin")}</span>
        <span>{t("coins.col.paid")}</span>
        <span className="text-right">{t("coins.col.day")}</span>
        <span />
      </div>
      {list.map((o) => (
        <Row
          key={o.coin.id}
          option={o}
          manual={showManual}
          selected={selected === o.coin.id}
          now={o.coin.id === now}
          best={o.coin.id === best}
          excluded={exclude.has(o.coin.id) && !o.coin.asic}
          destination={destination(o)}
          onSelect={() => ask(o.coin.id, o.coin.name)}
          onOpen={() => setSheet(o)}
          value={o.valuePerDay != null ? `≈ ${money(o.valuePerDay, currency, locale)}` : "—"}
          speed={o.hashrate != null ? hashrate(o.hashrate) : t("coin.notMeasured")}
        />
      ))}
    </div>
  );

  const section = (device: "gpu" | "cpu") => {
    const list = sorted(device);
    if (!list.length) return null;
    return (
      <section aria-labelledby={`coins-${device}`}>
        <h3
          id={`coins-${device}`}
          className="mb-2.5 flex items-center gap-2 px-1 text-sm font-semibold [&>svg]:size-4 [&>svg]:text-faint"
        >
          {device === "gpu" ? <Gpu /> : <Cpu />}
          {t(`coins.${device}`)}
        </h3>
        {table(device, list)}
      </section>
    );
  };
  const asic = sorted("asic");

  return (
    <>
      <PageTitle sub={t("coins.intro")}>{t("coins.title")}</PageTitle>

      <PayoutCard />

      <div className="mt-10 mb-4 flex flex-wrap items-center justify-between gap-3 px-1">
        <h2 className="flex items-center gap-1.5 text-base font-semibold">
          {t("coins.mine")}
          <Explain term="auto" align="left" />
        </h2>
        <Segmented
          label={t("coins.mine")}
          value={showManual ? "manual" : "auto"}
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
      </div>

      <div className="space-y-8">
        {showManual ? (
          <Notice tone="warn" icon={<TriangleAlert />}>{t("coins.manual.warn")}</Notice>
        ) : (
          <p className="-mt-2 px-1 text-sm text-muted">{t("coins.auto.line")}</p>
        )}
        <CoinAdvice card={false} warnOnly />
        {section("gpu")}
        {section("cpu")}
        {asic.length > 0 && (
          <details className="group/asic">
            <summary className="flex cursor-default list-none items-center gap-2 px-1 text-sm font-semibold [&::-webkit-details-marker]:hidden">
              <ChevronRight className="size-4 text-faint transition-transform duration-200 group-open/asic:rotate-90" aria-hidden />
              <Server className="size-4 text-faint" aria-hidden />
              <span>{t("coins.asic")}</span>
              <span className="text-xs font-normal text-faint">{t("coins.asic.short")}</span>
            </summary>
            <p className="mt-3 mb-3 max-w-2xl px-1 text-xs leading-relaxed text-faint">
              {t("coins.asic.note")}
            </p>
            {table("asic", asic)}
          </details>
        )}
      </div>

      <p className="mt-10 flex items-center gap-2 px-1 text-xs text-faint">
        <ShieldCheck className="size-4 shrink-0 text-go" />
        {t("wallet.noncustodial")}
      </p>

      {sheet && (
        <CoinSheet
          option={sheet}
          pools={pools[sheet.coin.id] ?? []}
          onClose={() => setSheet(null)}
        />
      )}
      {dialog}
    </>
  );
}

const PAYOUT_ICON: Record<PayoutMode, ReactNode> = {
  kryptex: <WalletIcon className="size-5" />,
  bitcoin: <Bitcoin className="size-5" />,
  convert: <CoinsIcon className="size-5" />,
  monero: <ShieldHalf className="size-5" />,
};

/** Where to see the balance for each payout type (opened in the browser). */
function balanceUrl(mode: PayoutMode, wallet: string): string | null {
  if (mode === "kryptex") return KRYPTEX_LOGIN;
  if (mode === "bitcoin")
    return `https://unmineable.com/coins/BTC/address/${encodeURIComponent(wallet)}`;
  if (mode === "convert") {
    const [coin, address] = wallet.split(":");
    return `https://unmineable.com/coins/${coin.toUpperCase()}/address/${encodeURIComponent(address)}`;
  }
  return null;
}

function short(a: string) {
  return a.length > 22 ? `${a.slice(0, 10)}…${a.slice(-8)}` : a;
}

/** The main payout: where every coin without its own wallet is paid. */
function PayoutCard() {
  const t = useT();
  const { config, saveConfig, toast } = useApp();
  const c = config!;
  const mode = payoutMode(c.wallet);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(c.wallet);
  const [valid, setValid] = useState(false);
  const [saving, setSaving] = useState(false);
  const [copied, setCopied] = useState(false);
  const url = c.wallet ? balanceUrl(mode, c.wallet) : null;

  const startEdit = () => {
    setDraft(c.wallet);
    setValid(false);
    setEditing(true);
  };
  const save = async () => {
    setSaving(true);
    try {
      await saveConfig({ ...c, wallet: draft.trim() });
      setEditing(false);
      toast("info", t("wallet.saved"));
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    } finally {
      setSaving(false);
    }
  };
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(c.wallet);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable: the address stays selectable */
    }
  };

  return (
    <section
      aria-labelledby="payout"
      className="flex flex-wrap items-center gap-x-4 gap-y-3 rounded-2xl border border-line bg-surface px-4 py-3.5 shadow-[var(--highlight)]"
    >
      <span
        aria-hidden
        className={cx("flex w-5 shrink-0 justify-center [&>svg]:size-5", c.wallet ? "text-btc" : "text-warn")}
      >
        {PAYOUT_ICON[mode]}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          <span id="payout" className="text-sm text-muted">
            {t("wallet.goesTo")}
          </span>
          <span className="text-sm font-semibold">
            {c.wallet ? t(`payout.${mode}.short`) : t("wallet.none")}
          </span>
          {c.wallet && (
            <button
              type="button"
              onClick={copy}
              title={c.wallet}
              aria-label={`${t("wallet.copy")} ${c.wallet}`}
              className="flex min-w-0 items-center gap-1.5 rounded-full border border-line bg-bg/50 px-2.5 py-0.5 font-mono text-xs text-muted transition-colors hover:border-line-strong hover:text-fg"
            >
              <span className="truncate">{short(c.wallet)}</span>
              {copied ? (
                <Check className="size-3.5 shrink-0 text-go" />
              ) : (
                <Copy className="size-3.5 shrink-0" />
              )}
            </button>
          )}
          <Explain
            term={
              mode === "bitcoin" || mode === "convert"
                ? "btc"
                : mode === "kryptex"
                  ? "kryptex"
                  : "wallet"
            }
            align="left"
          />
        </div>
        <p className="mt-0.5 text-xs text-faint">
          {c.wallet ? t(`wallet.oneLine.${mode}`) : t("coins.payout.missing")}
        </p>
      </div>
      <div className="flex gap-1.5">
        {url && (
          <Button size="sm" variant="ghost" onClick={() => openExternal(url)}>
            <ExternalLink /> {t("wallet.balance")}
          </Button>
        )}
        <Button size="sm" variant={c.wallet ? "secondary" : "primary"} onClick={startEdit}>
          {c.wallet ? t("wallet.change") : t("wallet.set")}
        </Button>
      </div>

      <Modal
        open={editing}
        onClose={() => setEditing(false)}
        closeLabel={t("confirm.cancel")}
        wide
        title={t("wallet.edit.title")}
        actions={
          <>
            <Button variant="ghost" onClick={() => setEditing(false)}>
              {t("confirm.cancel")}
            </Button>
            <Button
              variant="primary"
              onClick={save}
              loading={saving}
              disabled={!valid || draft.trim() === c.wallet}
            >
              {t("wallet.save")}
            </Button>
          </>
        }
      >
        <p className="mb-4">{t("confirm.wallet.body")}</p>
        <PayoutChoice value={draft} onChange={setDraft} onValid={setValid} advanced />
      </Modal>
    </section>
  );
}

/** What stops this coin here. Missing hardware comes first: a wallet wouldn't help. */
function rowBlocker(o: CoinOption) {
  return (
    o.requirements.find((r) => r.status === "missing" && r.kind !== "payout")
      ?.code ?? o.blocker
  );
}

/** 0: mineable now; 1: only needs a wallet; 2: this PC can't mine it. */
function rank(o: CoinOption) {
  const b = rowBlocker(o);
  return !b ? 0 : PAYOUT_BLOCKERS.includes(b) ? 1 : 2;
}

function Row({
  option: o,
  manual,
  selected,
  now,
  best,
  excluded,
  destination,
  value,
  speed,
  onSelect,
  onOpen,
}: {
  option: CoinOption;
  manual: boolean;
  selected: boolean;
  now: boolean;
  best: boolean;
  excluded: boolean;
  destination: string;
  value: string;
  speed: string;
  onSelect: () => void;
  onOpen: () => void;
}) {
  const t = useT();
  const { coin } = o;
  const blocker = rowBlocker(o);
  // An own wallet fixes a payout blocker; a hardware one leaves nothing to do.
  const fixable = rank(o) === 1;
  const dead = rank(o) === 2;
  return (
    <div
      className={cx(
        "relative grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1 px-4 py-3 transition-colors duration-200 sm:grid-cols-[minmax(0,1.4fr)_minmax(0,1.2fr)_8rem_2.25rem]",
        manual && selected
          ? "bg-accent-soft"
          : manual && !blocker && "hover:bg-surface-2/70",
      )}
    >
      {/* Coin */}
      <div className={cx("flex min-w-0 items-center gap-3", dead && "opacity-55")}>
        <CoinIcon id={coin.id} size={28} />
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
            {manual ? (
              // Stretched button: the whole row selects the coin.
              <button
                type="button"
                role="radio"
                aria-checked={selected}
                disabled={!!blocker}
                onClick={onSelect}
                className="text-left text-sm font-medium after:absolute after:inset-0 disabled:cursor-not-allowed"
              >
                {coin.name}
              </button>
            ) : (
              <span className="text-sm font-medium">{coin.name}</span>
            )}
            <span className="text-xs text-faint">{coin.id}</span>
            {now && <Badge tone="ok">{t("coins.auto.now")}</Badge>}
            {best && !now && !blocker && <Badge>{t("coins.auto.best")}</Badge>}
            {excluded && <Badge>{t("coins.excluded")}</Badge>}
          </div>
          <p className="truncate text-xs text-faint">
            {coin.algo}
            {!blocker && !o.minerInstalled && ` · ${t("coin.needsMiner")}`}
          </p>
        </div>
      </div>

      {/* Paid to, or what is missing */}
      <p
        className={cx(
          "col-span-2 row-start-2 truncate text-xs sm:col-span-1 sm:row-start-auto sm:text-sm",
          blocker ? (fixable ? "text-warn" : "text-faint") : "text-muted",
        )}
        title={blocker ? t(`coin.blocker.${blocker}`) : destination}
      >
        {blocker ? t(`coin.blocker.${blocker}`) : destination}
      </p>

      {/* Per day */}
      <div className="num hidden text-right sm:block">
        {!blocker && (
          <>
            <span className="block text-sm font-semibold">{value}</span>
            <span className="block text-xs text-faint">{speed}</span>
          </>
        )}
      </div>

      {fixable ? (
        // Same footprint as the settings button: the column is only 2.25rem wide.
        <button
          type="button"
          onClick={onOpen}
          aria-label={`${t("coins.addWallet")} ${coin.name}`}
          title={t("coins.addWallet")}
          className="relative z-10 row-start-1 flex size-9 shrink-0 items-center justify-center justify-self-end rounded-full border border-accent/40 bg-accent-soft text-accent transition-colors hover:bg-accent-solid hover:text-accent-fg sm:row-start-auto"
        >
          <Plus className="size-4" />
        </button>
      ) : (
        // Also on coins this PC can't mine: the sheet lists what is missing.
        <button
          type="button"
          onClick={onOpen}
          aria-label={t("coins.settings", { c: coin.name })}
          title={t("coins.settings", { c: coin.name })}
          className="relative z-10 row-start-1 flex size-9 shrink-0 items-center justify-center justify-self-end rounded-full text-faint transition-colors hover:bg-surface-3 hover:text-fg sm:row-start-auto"
        >
          {manual && selected ? <Check className="size-4 text-accent" /> : <Settings2 className="size-4" />}
        </button>
      )}
    </div>
  );
}
