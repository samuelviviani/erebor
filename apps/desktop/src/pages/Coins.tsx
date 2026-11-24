import {
  ArrowRight,
  Ban,
  Bitcoin,
  Check,
  Copy,
  Cpu,
  ExternalLink,
  Gpu,
  Pencil,
  Plus,
  Server,
  Settings2,
  ShieldCheck,
  ShieldHalf,
  Sparkles,
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
  IconTile,
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

  const byValue = (a: CoinOption, b: CoinOption) =>
    !!a.blocker !== !!b.blocker
      ? a.blocker
        ? 1
        : -1
      : (b.valuePerDay ?? -1) - (a.valuePerDay ?? -1);

  // ASIC coins (BTC, LTC…) apart: mineable from the CPU, but they earn almost nothing.
  const group = (device: "gpu" | "cpu" | "asic") => {
    const list = options
      .filter((o) =>
        device === "asic"
          ? o.coin.asic
          : o.coin.device === device && !o.coin.asic,
      )
      .sort(byValue);
    if (!list.length) return null;
    return (
      <section>
        <h3 className="eyebrow mb-3 flex items-center gap-2 px-1 [&_svg]:size-3.5">
          {device === "gpu" ? (
            <Gpu className="size-4" />
          ) : device === "asic" ? (
            <Server className="size-4" />
          ) : (
            <Cpu className="size-4" />
          )}
          {t(`coins.${device}`)}
          <Explain term="gpu" align="left" />
        </h3>
        {device === "asic" && (
          <p className="mb-2 px-1 text-xs leading-relaxed text-faint">
            {t("coins.asic.note")}
          </p>
        )}
        <div
          className="panel divide-y divide-line overflow-hidden"
          role={showManual ? "radiogroup" : undefined}
          aria-label={t(`coins.${device}`)}
        >
          {list.map((o) => (
            <Row
              key={o.coin.id}
              option={o}
              manual={showManual}
              selected={selected === o.coin.id}
              best={o.coin.id === best}
              excluded={exclude.has(o.coin.id) && !o.coin.asic}
              destination={destination(o)}
              onSelect={() => ask(o.coin.id, o.coin.name)}
              onOpen={() => setSheet(o)}
              value={
                o.valuePerDay != null
                  ? `≈ ${money(o.valuePerDay, currency, locale)}`
                  : "—"
              }
              speed={
                o.hashrate != null
                  ? `${t("home.perDay")} · ${hashrate(o.hashrate)}`
                  : t("coin.notMeasured")
              }
            />
          ))}
        </div>
      </section>
    );
  };

  return (
    <>
      <PageTitle sub={t("coins.intro")}>{t("coins.title")}</PageTitle>

      <PayoutCard />

      <div className="mt-12 mb-5 flex flex-wrap items-center justify-between gap-3 px-1">
        <h2 className="font-display text-lg">{t("coins.mine")}</h2>
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
        <div className="space-y-4">
          {showManual ? (
            <Notice tone="warn" icon={<TriangleAlert />}>{t("coins.manual.warn")}</Notice>
          ) : (
            <AutoCard />
          )}
          {(!showManual || selected !== "AUTO") && (
            <CoinAdvice card={false} warnOnly />
          )}
        </div>
        {group("gpu")}
        {group("cpu")}
        {group("asic")}
        <NotFromAPC />
      </div>

      <p className="mt-10 flex items-center justify-center gap-2 px-1 text-xs text-faint">
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
    <section className="panel flex flex-wrap items-center gap-4 px-5 py-5">
      <IconTile tone={c.wallet ? "accent" : "warn"} className="size-12 rounded-2xl [&>svg]:size-6">
        {PAYOUT_ICON[mode]}
      </IconTile>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5 text-xs text-faint">
          {t("wallet.goesTo")}
          <Explain
            term={
              mode === "bitcoin" ? "btc" : mode === "kryptex" ? "kryptex" : "wallet"
            }
            align="left"
          />
        </div>
        <div className="flex min-w-0 items-center gap-2">
          <span className="shrink-0 text-base font-semibold">
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
        </div>
        <p className="mt-0.5 text-xs text-muted">
          {c.wallet ? t(`wallet.oneLine.${mode}`) : t("coins.payout.missing")}
        </p>
      </div>
      <div className="flex gap-2">
        {url && (
          <Button variant="ghost" onClick={() => openExternal(url)}>
            <ExternalLink className="size-4" /> {t("wallet.balance")}
          </Button>
        )}
        <Button variant={c.wallet ? "secondary" : "primary"} onClick={startEdit}>
          <Pencil className="size-4" />{" "}
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
        <PayoutChoice value={draft} onChange={setDraft} onValid={setValid} />
      </Modal>
    </section>
  );
}

/** Automatic mode at a glance: the coin being mined now and what it earns. */
function AutoCard() {
  const t = useT();
  const { config, status } = useApp();
  const locale = config?.language === "en" ? "en-US" : "it-IT";
  const currency = config?.energy.currency ?? "EUR";
  const coin = status?.coin;
  const perDay = status?.projection?.valuePerDay;
  return (
    <section className="panel flex items-center gap-5 p-5">
      <span className="relative shrink-0">
        {coin ? (
          <CoinIcon id={coin.id} size={56} className="shadow-[0_8px_20px_-8px_rgb(0_0_0/0.7)]" />
        ) : (
          <span className="block size-13" />
        )}
        <span className="absolute -right-1 -bottom-1 flex size-6 items-center justify-center rounded-full border-2 border-surface bg-accent-solid text-accent-fg">
          <Sparkles className="size-3.5" />
        </span>
      </span>
      <div className="min-w-0 flex-1">
        <p className="eyebrow flex items-center gap-1.5">
          {t("coins.auto.now")}
          <Explain term="auto" align="left" />
        </p>
        <p className="font-display mt-1 truncate text-2xl">
          {coin?.name ?? "—"}
        </p>
        {coin && (
          <p className="mt-0.5 truncate text-xs text-faint">
            {t(coin.device === "gpu" ? "coins.gpu" : "coins.cpu")} · {coin.algo}
          </p>
        )}
      </div>
      {perDay != null && (
        <div className="num shrink-0 text-right">
          <p className="text-xl font-semibold">≈ {money(perDay, currency, locale)}</p>
          <p className="text-xs text-faint">{t("home.perDay")}</p>
        </div>
      )}
    </section>
  );
}

function Row({
  option: o,
  manual,
  selected,
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
  // What this PC lacks comes first: a wallet wouldn't help without the hardware.
  const blocker =
    o.requirements.find(
      (r) => r.status === "missing" && r.kind !== "payout",
    )?.code ?? o.blocker;
  // An own wallet fixes a payout blocker; a hardware one leaves nothing to do.
  const fixable = !!blocker && PAYOUT_BLOCKERS.includes(blocker);
  const dead = !!blocker && !fixable;
  return (
    <div
      className={cx(
        "relative flex items-center gap-4 px-5 py-4 transition-colors duration-200",
        manual && selected ? "bg-accent-soft" : manual && !blocker && "hover:bg-surface-2/70",
      )}
    >
      <span className={cx("shrink-0", dead && "opacity-50")}>
        <CoinIcon id={coin.id} size={40} />
      </span>
      <div className={cx("min-w-0 flex-1", dead && "opacity-60")}>
        <div className="flex flex-wrap items-center gap-x-2">
          {manual ? (
            // Stretched button: the whole row selects the coin.
            <button
              type="button"
              role="radio"
              aria-checked={selected}
              disabled={!!blocker}
              onClick={onSelect}
              className="text-left text-[15px] font-medium after:absolute after:inset-0 disabled:cursor-not-allowed"
            >
              {coin.name}
            </button>
          ) : (
            <span className="text-[15px] font-medium">{coin.name}</span>
          )}
          <span className="text-xs text-faint">{coin.id}</span>
          {best && !blocker && (
            <Badge tone="ok">{t("coins.auto.best")}</Badge>
          )}
          {excluded && (
            <Badge>{t("coins.excluded")}</Badge>
          )}
        </div>
        <p
          className={cx(
            "mt-0.5 text-[13px] leading-relaxed",
            fixable ? "text-warn" : "text-muted",
          )}
        >
          {blocker
            ? t(`coin.blocker.${blocker}`)
            : `${t("coin.algoLabel")} ${coin.algo}${o.minerInstalled ? "" : ` · ${t("coin.needsMiner")}`}`}
        </p>
        {!blocker && (
          <p className="mt-0.5 flex items-center gap-1 text-xs text-faint">
            <ArrowRight className="size-3 shrink-0" aria-hidden />
            <span className="truncate">{destination}</span>
          </p>
        )}
      </div>
      {!blocker && (
        <div className="num shrink-0 text-right">
          <span className="block text-[15px] font-semibold text-fg">{value}</span>
          <span className="block text-xs text-faint">{speed}</span>
        </div>
      )}
      {fixable ? (
        <Button
          size="sm"
          className="relative z-10"
          onClick={onOpen}
          aria-label={`${t("coins.addWallet")} ${coin.name}`}
        >
          <Plus className="size-4" /> {t("coins.addWallet")}
        </Button>
      ) : (
        // Also on coins this PC can't mine: the sheet lists what is missing.
        <button
          type="button"
          onClick={onOpen}
          aria-label={t("coins.settings", { c: coin.name })}
          title={t("coins.settings", { c: coin.name })}
          className="relative z-10 flex size-9 shrink-0 items-center justify-center rounded-full text-faint transition-colors hover:bg-surface-3 hover:text-fg"
        >
          <Settings2 className="size-4" />
        </button>
      )}
      {manual && (
        <Check
          className={cx("size-5 shrink-0 text-accent", !selected && "invisible")}
          aria-hidden
        />
      )}
    </div>
  );
}

/** Big coins a PC can't mine: dedicated machines (ASIC) or no mining at all. */
const ASIC_ONLY: [string, string][] = [
  ["ZEC", "Zcash"],
  ["DASH", "Dash"],
  ["KDA", "Kadena"],
  ["CKB", "Nervos"],
  ["ZEN", "Horizen"],
  ["HNS", "Handshake"],
  ["SC", "Siacoin"],
];

function NotFromAPC() {
  const t = useT();
  return (
    <section>
      <h3 className="eyebrow mb-3 flex items-center gap-2 px-1">
        <Ban className="size-3.5" />
        {t("coins.none.title")}
      </h3>
      <div className="space-y-3 rounded-[20px] border border-dashed border-line-strong px-5 py-5">
        <p className="text-[13px] leading-relaxed text-muted">
          {t("coins.none.asic")}
        </p>
        <ul className="flex flex-wrap gap-2">
          {ASIC_ONLY.map(([id, name]) => (
            <li
              key={id}
              className="rounded-full border border-line bg-surface-2/60 px-3 py-1 text-xs text-muted"
            >
              {name} <span className="text-faint">{id}</span>
            </li>
          ))}
        </ul>
        <p className="text-[13px] leading-relaxed text-faint">
          {t("coins.none.pos")}
        </p>
      </div>
    </section>
  );
}
