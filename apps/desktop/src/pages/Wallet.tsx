import {
  Bitcoin,
  Check,
  Copy,
  ExternalLink,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  ShieldHalf,
  Wallet as WalletIcon,
} from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useCoinOptions } from "../components/CoinChange";
import { CoinIcon } from "../components/CoinIcon";
import { EditDialog } from "../components/CoinWallets";
import { PageTitle } from "../components/Layout";
import { KRYPTEX_LOGIN } from "../components/shared";
import {
  PayoutChoice,
  payoutMode,
  type PayoutMode,
} from "../components/PayoutChoice";
import {
  Button,
  cx,
  Empty,
  Explain,
  inputClass,
  Modal,
  Notice,
} from "../components/ui";
import { api, errorText, openExternal, type Coin, type PoolAccount } from "../lib/api";
import { btcFull, dateTime, money, xmr } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

const ICON: Record<PayoutMode, ReactNode> = {
  kryptex: <WalletIcon className="size-5" />,
  bitcoin: <Bitcoin className="size-5" />,
  monero: <ShieldHalf className="size-5" />,
};

/** Where to see the balance for each payout type (opened in the browser). */
function balanceUrl(mode: PayoutMode, wallet: string): string | null {
  if (mode === "kryptex") return KRYPTEX_LOGIN;
  if (mode === "bitcoin")
    return `https://unmineable.com/coins/BTC/address/${encodeURIComponent(wallet)}`;
  return null;
}

function short(a: string) {
  return a.length > 22 ? `${a.slice(0, 10)}…${a.slice(-8)}` : a;
}

/** Per-coin wallets: back with the Monero/BTC payouts (Kryptex only for now). */
const OWN_WALLETS = false;

export default function WalletPage() {
  const t = useT();
  const { config, saveConfig, toast, status } = useApp();
  const c = config!;
  const lang = c.language;
  const locale = lang === "it" ? "it-IT" : "en-US";
  const mode = payoutMode(c.wallet);
  const options = useCoinOptions();
  const [editing, setEditing] = useState(false);
  const [coinEdit, setCoinEdit] = useState<{ id: string; name: string } | null>(
    null,
  );
  const [picking, setPicking] = useState(false);
  const [draft, setDraft] = useState(c.wallet);
  const [valid, setValid] = useState(false);
  const [saving, setSaving] = useState(false);
  const [copied, setCopied] = useState(false);
  const [account, setAccount] = useState<PoolAccount | null>(null);
  const [accountErr, setAccountErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const wallets = c.coinWallets ?? {};
  // Only SupportXMR (Monero address) exposes balance and payouts through a public API.
  const poolApi =
    !c.demoMode &&
    !!c.wallet &&
    mode === "monero" &&
    c.pool.id === "supportxmr";

  const refresh = useCallback(async () => {
    if (!poolApi) return;
    setLoading(true);
    try {
      setAccount(await api.poolAccount());
      setAccountErr(null);
    } catch (e) {
      setAccountErr(translateError(lang, errorText(e)));
    } finally {
      setLoading(false);
    }
  }, [poolApi, lang]);

  useEffect(() => {
    refresh();
  }, [refresh]);

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
      toast("error", translateError(lang, errorText(e)));
    } finally {
      setSaving(false);
    }
  };
  const saveCoin = async (id: string, address: string | null) => {
    const next = { ...wallets };
    if (address) next[id] = address.trim();
    else delete next[id];
    try {
      await saveConfig({ ...c, coinWallets: next });
      setCoinEdit(null);
      toast("info", t(address ? "coinWallet.saved" : "coinWallet.removed"));
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
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
  const url = c.wallet ? balanceUrl(mode, c.wallet) : null;

  /** Where a coin's earnings go, in plain words. */
  const route = (coin: Coin): { text: string; muted?: boolean } => {
    if (wallets[coin.id])
      return { text: `${t("wallet.route.own")} · ${short(wallets[coin.id])}` };
    if (!c.wallet) return { text: "—", muted: true };
    if (mode === "kryptex") return { text: t("payout.kryptex.short") };
    if (mode === "bitcoin")
      return coin.unmineableHost
        ? { text: t("payout.bitcoin.short") }
        : { text: t("wallet.route.needsKryptex"), muted: true };
    return coin.id === "XMR"
      ? { text: t("payout.monero.short") }
      : { text: t("wallet.route.addWallet"), muted: true };
  };

  const ownList = options.filter((o) => wallets[o.coin.id]);
  // Coins the main payout cannot receive and that have no own wallet yet.
  const unreachable = options.filter(
    (o) => !wallets[o.coin.id] && route(o.coin).muted && c.wallet,
  );
  const payoutName = c.wallet ? t(`payout.${mode}.short`) : t("wallet.none");
  const fiat =
    status?.minedBtc != null && status.market.btc
      ? status.minedBtc *
        status.market.btc[c.energy.currency === "USD" ? "usd" : "eur"]
      : null;

  return (
    <>
      <PageTitle sub={t("wallet.sub")}>{t("wallet.title")}</PageTitle>

      {/* Main payout: balance first, then where it goes */}
      <section className="overflow-hidden rounded-2xl border border-line bg-surface">
        {c.wallet && mode !== "monero" && (
          <div className="flex flex-wrap items-end justify-between gap-4 p-5">
            <div className="min-w-0">
              <p className="flex items-center gap-1.5 text-sm text-muted">
                {t(
                  mode === "kryptex"
                    ? "home.btc.kryptex.short"
                    : "home.btc.unmineable.short",
                )}
                <Explain term="btc" align="left" />
              </p>
              <p className="num mt-1 text-4xl font-semibold tracking-tight">
                {btcFull(status?.minedBtc ?? 0, locale)}
              </p>
              {fiat != null && (
                <p className="num mt-1 text-sm text-faint">
                  ≈ {money(fiat, c.energy.currency, locale)}
                </p>
              )}
            </div>
            {url && (
              <Button
                variant="ghost"
                className="border border-line"
                onClick={() => openExternal(url)}
              >
                <ExternalLink className="size-4" /> {t("wallet.balance")}
              </Button>
            )}
          </div>
        )}
        <div
          className={cx(
            "flex flex-wrap items-center gap-4 px-5 py-4",
            c.wallet && mode !== "monero" && "border-t border-line bg-bg/40",
          )}
        >
          <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent">
            {ICON[mode]}
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5 text-xs text-faint">
              {t("wallet.goesTo")}
              <Explain
                term={
                  mode === "bitcoin"
                    ? "btc"
                    : mode === "kryptex"
                      ? "kryptex"
                      : "wallet"
                }
                align="left"
              />
            </div>
            <div className="flex min-w-0 items-center gap-2">
              <span className="shrink-0 text-[15px] font-medium">
                {payoutName}
              </span>
              {c.wallet && (
                <button
                  type="button"
                  onClick={copy}
                  title={c.wallet}
                  aria-label={`${t("wallet.copy")} ${c.wallet}`}
                  className="flex min-w-0 items-center gap-1.5 rounded-lg px-2 py-0.5 font-mono text-xs text-faint transition-colors hover:bg-surface-2 hover:text-fg"
                >
                  <span className="truncate">{short(c.wallet)}</span>
                  {copied ? (
                    <Check className="size-3.5 shrink-0 text-go" />
                  ) : (
                    <Copy className="size-3.5 shrink-0" />
                  )}
                </button>
              )}
            </div>
            {c.wallet && (
              <p className="mt-0.5 text-xs text-muted">
                {t(`wallet.oneLine.${mode}`)}
              </p>
            )}
          </div>
          <Button variant="primary" onClick={startEdit}>
            <Pencil className="size-4" />{" "}
            {c.wallet ? t("wallet.change") : t("wallet.set")}
          </Button>
        </div>
      </section>

      {/* Exceptions only: coins sent to an own wallet. Off while Erebor is Kryptex only. */}
      {OWN_WALLETS && (
        <section className="mt-10">
          <div className="mb-3 flex flex-wrap items-end justify-between gap-3 px-1">
            <div className="min-w-0">
              <h2 className="flex items-center gap-1.5 text-sm font-medium">
                {t("wallet.own.title")}
                <Explain term="wallet" align="left" />
              </h2>
              <p className="mt-0.5 text-xs text-faint">
                {t("wallet.own.hint", { p: payoutName })}
              </p>
            </div>
            <Button
              variant="ghost"
              className="border border-line"
              onClick={() => setPicking(true)}
              disabled={ownList.length === options.length}
            >
              <Plus className="size-4" /> {t("wallet.own.add")}
            </Button>
          </div>
          {unreachable.length > 0 && (
            <div className="mb-3">
              <Notice tone="warn">
                {t(
                  unreachable.length === 1
                    ? "wallet.unreachable.one"
                    : "wallet.unreachable",
                  {
                    n: unreachable.length,
                    p: payoutName,
                    list:
                      unreachable
                        .slice(0, 3)
                        .map((o) => o.coin.name)
                        .join(", ") +
                      (unreachable.length > 3
                        ? ` +${unreachable.length - 3}`
                        : ""),
                  },
                )}
              </Notice>
            </div>
          )}
          {ownList.length === 0 ? (
            <p className="rounded-2xl border border-dashed border-line px-4 py-5 text-center text-sm text-faint">
              {t("wallet.own.none", { p: payoutName })}
            </p>
          ) : (
            <div className="divide-y divide-line overflow-hidden rounded-2xl border border-line bg-surface">
              {ownList.map(({ coin }) => (
                <div
                  key={coin.id}
                  className="flex items-center gap-3 px-4 py-3"
                >
                  <CoinIcon id={coin.id} size={28} />
                  <span className="w-36 shrink-0 truncate text-sm font-medium">
                    {coin.name}
                  </span>
                  <span
                    className="min-w-0 flex-1 truncate font-mono text-xs text-muted"
                    title={wallets[coin.id]}
                  >
                    {short(wallets[coin.id])}
                  </span>
                  <Button
                    variant="ghost"
                    onClick={() =>
                      setCoinEdit({ id: coin.id, name: coin.name })
                    }
                    aria-label={`${t("coinWallet.edit")} ${coin.name}`}
                  >
                    <Pencil className="size-3.5" /> {t("coinWallet.edit")}
                  </Button>
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      {/* Balance and payouts: only where the pool publishes them */}
      {poolApi && (
        <section className="mt-8">
          <div className="mb-3 flex items-center justify-between px-1">
            <h2 className="text-sm font-medium">{t("wallet.payouts")}</h2>
            <Button variant="ghost" onClick={refresh} loading={loading}>
              <RefreshCw className="size-4" /> {t("wallet.refresh")}
            </Button>
          </div>
          <div className="rounded-2xl border border-line bg-surface px-4 py-2">
            {account && (
              <div className="flex gap-8 py-2 text-sm">
                <span className="text-muted">
                  {t("wallet.unpaid")}{" "}
                  <span className="num ml-1 text-fg">
                    {xmr(account.unpaidXmr)}
                  </span>
                </span>
                <span className="text-muted">
                  {t("wallet.paid")}{" "}
                  <span className="num ml-1 text-fg">
                    {xmr(account.paidXmr)}
                  </span>
                </span>
              </div>
            )}
            {accountErr ? (
              <Notice tone="warn">{accountErr}</Notice>
            ) : !account?.payments.length ? (
              <Empty>{t("wallet.payouts.empty")}</Empty>
            ) : (
              account.payments.slice(0, 20).map((p, i) => (
                <div
                  key={i}
                  className="flex justify-between border-t border-line py-2 text-sm"
                >
                  <span className="text-muted">{dateTime(p.ts, locale)}</span>
                  <span className="num">{xmr(p.amountXmr)}</span>
                </div>
              ))
            )}
          </div>
        </section>
      )}

      <p className="mt-6 flex items-center gap-2 px-1 text-xs text-faint">
        <ShieldCheck className="size-4 shrink-0 text-go" />
        {t("wallet.noncustodial")}
      </p>

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
        <PayoutChoice value={draft} onChange={setDraft} onValid={setValid} />
      </Modal>
      {picking && (
        <CoinPicker
          coins={options.filter((o) => !wallets[o.coin.id]).map((o) => o.coin)}
          onPick={(coin) => {
            setPicking(false);
            setCoinEdit({ id: coin.id, name: coin.name });
          }}
          onClose={() => setPicking(false)}
        />
      )}
      {coinEdit && (
        <EditDialog
          coin={coinEdit}
          current={wallets[coinEdit.id] ?? ""}
          onSave={saveCoin}
          onClose={() => setCoinEdit(null)}
        />
      )}
    </>
  );
}

/** Pick a coin from a list that can grow long: search by name or ticker. */
function CoinPicker({
  coins,
  onPick,
  onClose,
}: {
  coins: Coin[];
  onPick: (c: Coin) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [q, setQ] = useState("");
  const query = q.trim().toLowerCase();
  const list = coins.filter(
    (c) =>
      !query ||
      c.name.toLowerCase().includes(query) ||
      c.id.toLowerCase().includes(query),
  );
  return (
    <Modal
      open
      onClose={onClose}
      closeLabel={t("close")}
      wide
      title={t("wallet.pick.title")}
    >
      <div className="relative mt-2">
        <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-faint" />
        <input
          autoFocus
          className={cx(inputClass, "pl-9")}
          placeholder={t("wallet.pick.search")}
          aria-label={t("wallet.pick.search")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </div>
      <div
        className="mt-3 max-h-80 overflow-y-auto"
        role="listbox"
        aria-label={t("wallet.pick.title")}
      >
        {list.length === 0 ? (
          <p className="py-6 text-center text-sm text-faint">
            {t("wallet.pick.empty")}
          </p>
        ) : (
          list.map((coin) => (
            <button
              key={coin.id}
              type="button"
              role="option"
              aria-selected={false}
              onClick={() => onPick(coin)}
              className="flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-left text-fg hover:bg-surface-2"
            >
              <CoinIcon id={coin.id} size={28} />
              <span className="text-sm font-medium">{coin.name}</span>
              <span className="text-xs text-faint">{coin.id}</span>
            </button>
          ))
        )}
      </div>
    </Modal>
  );
}
