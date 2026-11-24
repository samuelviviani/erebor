import {
  CheckCircle2,
  CircleAlert,
  Download,
  ExternalLink,
  XCircle,
} from "lucide-react";
import { useEffect, useState } from "react";
import {
  api,
  errorText,
  openExternal,
  type CoinOption,
  type CoinPool,
  type CoinPoolChoice,
  type Requirement,
  type WalletError,
} from "../lib/api";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { CoinIcon } from "./CoinIcon";
import { payoutMode } from "./PayoutChoice";
import { MINER_LABEL } from "./shared";
import {
  Button,
  cx,
  Explain,
  Field,
  inputClass,
  Modal,
  Segmented,
  Toggle,
} from "./ui";

const NO_CHOICE: CoinPoolChoice = {
  pool: "",
  region: "",
  host: "",
  port: 0,
  tls: false,
  backup: "",
};

/** Blockers an own wallet for that coin removes (the others are about the hardware). */
export const PAYOUT_BLOCKERS = [
  "wallet_missing",
  "own_wallet_required",
  "not_on_unmineable",
  "kryptex_or_btc_required",
];

/** Pools per coin from the engine, loaded once per page. */
export function useCoinPools() {
  const [pools, setPools] = useState<Record<string, CoinPool[]>>({});
  useEffect(() => {
    api
      .coinPools()
      .then(setPools)
      .catch(() => {});
  }, []);
  return pools;
}

/** Name of the pool an own-wallet coin is mined on ("": the first listed). */
export function poolName(choice: CoinPoolChoice | undefined, list: CoinPool[]) {
  if (choice?.pool === "custom") return choice.host;
  return (list.find((p) => p.pool === choice?.pool) ?? list[0])?.name ?? "—";
}

/**
 * Everything about one coin, saved together: whether the automatic choice may pick it,
 * and where its earnings go (the main payout, or an own address on a chosen pool).
 */
export function CoinSheet({
  option,
  pools,
  onClose,
}: {
  option: CoinOption;
  pools: CoinPool[];
  onClose: () => void;
}) {
  const t = useT();
  const { config, saveConfig, toast } = useApp();
  const c = config!;
  const { coin } = option;
  const saved = c.coinWallets?.[coin.id] ?? "";
  const savedChoice = c.coinPools?.[coin.id] ?? NO_CHOICE;
  const mode = payoutMode(c.wallet);
  const payout = c.wallet ? t(`payout.${mode}.short`) : t("wallet.none");
  // With an own wallet saved the engine reports no payout blocker: redo its checks.
  const viaUnmineable = mode === "bitcoin" && !!c.wallet && !!coin.unmineableHost;
  const mainBlocked = saved
    ? (!c.wallet && !c.demoMode) ||
      (!coin.host && !viaUnmineable) ||
      (!!c.wallet && mode === "bitcoin" && !coin.unmineableHost)
    : PAYOUT_BLOCKERS.includes(option.blocker ?? "");

  const [own, setOwn] = useState(!!saved || mainBlocked);
  const [inAuto, setInAuto] = useState(!c.autoExclude?.includes(coin.id));
  const [value, setValue] = useState(saved);
  const [choice, setCh] = useState<CoinPoolChoice>(savedChoice);
  // "" is the first listed pool (Kryptex when the coin is there); none listed: custom.
  const first = pools[0]?.pool ?? "custom";
  const ch = { ...choice, pool: choice.pool || first };
  const [state, setState] = useState<{ ok: boolean; msg: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const pool = pools.find((p) => p.pool === ch.pool);
  const custom = ch.pool === "custom";
  const regions = pool?.hosts.map(([r]) => r) ?? [];

  useEffect(() => {
    if (!own || !value.trim()) {
      setState(null);
      return;
    }
    const id = setTimeout(() => {
      api
        .validateCoinWallet(coin.id, value.trim())
        .then(() =>
          setState({ ok: true, msg: t("coinWallet.ok", { c: coin.name }) }),
        )
        .catch((e: WalletError | string) =>
          setState({
            ok: false,
            msg: t(`wallet.err.${typeof e === "string" ? e : e.code}`),
          }),
        );
    }, 250);
    return () => clearTimeout(id);
  }, [value, own]);

  // The saved pool's public balance page, when it has one.
  const balance =
    saved && savedChoice.pool !== "custom"
    ? (
        pools.find((p) => p.pool === savedChoice.pool) ?? pools[0]
      )?.balanceUrl?.replace("{a}", encodeURIComponent(saved))
    : undefined;
  const customOk = !custom || (ch.host.trim() !== "" && ch.port > 0);
  const ready = !own || (!!state?.ok && customOk);

  const save = async () => {
    const wallets = { ...c.coinWallets };
    const choices = { ...c.coinPools };
    if (own) {
      wallets[coin.id] = value.trim();
      choices[coin.id] = {
        ...ch,
        pool: ch.pool === first ? "" : ch.pool,
        region: custom || !regions.includes(ch.region) ? "" : ch.region,
        host: custom ? ch.host.trim() : "",
        port: custom ? ch.port : 0,
        tls: custom && ch.tls,
        backup: ch.backup === ch.pool ? "" : ch.backup,
      };
    } else {
      delete wallets[coin.id];
      delete choices[coin.id];
    }
    const exclude = new Set(c.autoExclude ?? []);
    if (inAuto) exclude.delete(coin.id);
    else exclude.add(coin.id);
    setSaving(true);
    try {
      await saveConfig({
        ...c,
        coinWallets: wallets,
        coinPools: choices,
        autoExclude: [...exclude],
      });
      toast("info", t("sheet.saved", { c: coin.name }));
      onClose();
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      open
      wide
      onClose={onClose}
      closeLabel={t("confirm.cancel")}
      icon={<CoinIcon id={coin.id} size={40} />}
      title={coin.name}
      actions={
        <>
          {balance && (
            <Button
              variant="ghost"
              className="mr-auto"
              onClick={() => openExternal(balance)}
            >
              <ExternalLink className="size-4" /> {t("sheet.balance")}
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            {t("confirm.cancel")}
          </Button>
          <Button
            variant="primary"
            onClick={save}
            loading={saving}
            disabled={!ready}
          >
            {t("wallet.save")}
          </Button>
        </>
      }
    >
      <p className="text-xs text-faint">
        {coin.id} · {t("coin.algoLabel")} {coin.algo}
      </p>
      <div className="mt-5 space-y-6">
        <Checklist option={option} />
        {!coin.asic && (
          <div className="rounded-xl border border-line px-3.5 py-1.5">
            <Toggle
              checked={inAuto}
              onChange={setInAuto}
              label={t("sheet.auto")}
              description={t("sheet.auto.desc")}
            />
          </div>
        )}

        <div className="space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="flex items-center gap-1.5 text-xs font-medium tracking-wide text-faint uppercase">
              {t("sheet.payTo")}
              <Explain term="ownWallets" align="left" />
            </p>
            <Segmented
              label={t("sheet.payTo")}
              value={own ? "own" : "main"}
              onChange={(v) => setOwn(v === "own")}
              options={[
                { value: "main", label: t("sheet.main") },
                { value: "own", label: t("sheet.own", { c: coin.id }) },
              ]}
            />
          </div>

          {!own ? (
            <p
              className={cx(
                "rounded-xl px-3.5 py-3 text-[13px]",
                mainBlocked ? "bg-warn/10 text-fg" : "bg-surface-2",
              )}
            >
              {mainBlocked ? (
                t("sheet.main.blocked", { p: payout, c: coin.id })
              ) : (
                <>
                  <span className="font-medium text-fg">{payout}</span> ·{" "}
                  {c.wallet
                    ? t(`wallet.oneLine.${mode}`)
                    : t("coins.payout.missing")}
                </>
              )}
            </p>
          ) : (
            <OwnRoute
              coinId={coin.id}
              coinName={coin.name}
              pools={pools}
              value={value}
              setValue={setValue}
              state={state}
              ch={ch}
              setCh={setCh}
              regions={regions}
            />
          )}
        </div>
      </div>
    </Modal>
  );
}

/** Address, pool, region, custom stratum and backup for one coin. */
function OwnRoute({
  coinId,
  coinName,
  pools,
  value,
  setValue,
  state,
  ch,
  setCh,
  regions,
}: {
  coinId: string;
  coinName: string;
  pools: CoinPool[];
  value: string;
  setValue: (v: string) => void;
  state: { ok: boolean; msg: string } | null;
  ch: CoinPoolChoice;
  setCh: (c: CoinPoolChoice) => void;
  regions: string[];
}) {
  const t = useT();
  const pool = pools.find((p) => p.pool === ch.pool);
  const custom = ch.pool === "custom";
  return (
    <div className="space-y-5">
      <Field
        label={t("coinWallet.address", { c: coinName })}
        hint={
          state ? (
            <span
              className={state.ok ? "text-go" : "text-danger"}
              role="status"
            >
              {state.ok ? (
                <CheckCircle2 className="mr-1 inline size-3.5" />
              ) : (
                <XCircle className="mr-1 inline size-3.5" />
              )}
              {state.msg}
            </span>
          ) : pool && !pool.needsAddress ? (
            t("adv.p2pool.address")
          ) : (
            t("coinWallet.hint")
          )
        }
      >
        <input
          className={`${inputClass} font-mono text-xs`}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          spellCheck={false}
          autoComplete="off"
        />
      </Field>

      <div>
        <p className="mb-2 flex items-center gap-1.5 text-xs font-medium tracking-wide text-faint uppercase">
          {t("adv.pool")}
          <Explain term="poolChoice" align="left" />
        </p>
        <div
          role="radiogroup"
          aria-label={t("adv.pool")}
          className="divide-y divide-line overflow-hidden rounded-2xl border border-line"
        >
          {[...pools.map((p) => p.pool), "custom"].map((id) => {
            const p = pools.find((x) => x.pool === id);
            const on = ch.pool === id;
            return (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => setCh({ ...ch, pool: id, region: "" })}
                className={cx(
                  "flex w-full items-center gap-3 px-3.5 py-2.5 text-left",
                  on ? "bg-accent-soft/60" : "hover:bg-surface-2/60",
                )}
              >
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium text-fg">
                    {p?.name ?? t("adv.custom")}
                  </span>
                  <span className="block text-xs text-faint">
                    {p
                      ? [
                          p.feePercent != null
                            ? t("adv.fee", { v: p.feePercent })
                            : t("adv.fee.kryptex"),
                          p.scheme,
                          p.minPayout != null
                            ? t("adv.minShort", { v: p.minPayout, c: coinId })
                            : null,
                        ]
                          .filter(Boolean)
                          .join(" · ")
                      : t("adv.custom.desc")}
                  </span>
                </span>
                <span
                  className={cx(
                    "size-4 shrink-0 rounded-full border",
                    on ? "border-accent bg-accent-solid" : "border-line",
                  )}
                />
              </button>
            );
          })}
        </div>
      </div>

      {regions.length > 1 && (
        <div className="flex flex-wrap items-center gap-3">
          <span className="flex items-center gap-1.5 text-sm text-muted">
            {t("adv.region")}
            <Explain term="poolChoice" align="left" />
          </span>
          <Segmented
            label={t("adv.region")}
            value={regions.includes(ch.region) ? ch.region : regions[0]}
            onChange={(region) => setCh({ ...ch, region })}
            options={regions.map((r) => ({
              value: r,
              label: t(`region.${r}`),
            }))}
          />
        </div>
      )}

      {custom && (
        <div className="flex flex-wrap items-center gap-3">
          <input
            aria-label={t("adv.custom.host")}
            placeholder="pool.example.com"
            className={cx(inputClass, "w-56 font-mono text-xs")}
            value={ch.host}
            onChange={(e) => setCh({ ...ch, host: e.target.value })}
            spellCheck={false}
          />
          <input
            aria-label={t("adv.custom.port")}
            placeholder="3333"
            inputMode="numeric"
            className={cx(inputClass, "w-24 font-mono text-xs")}
            value={ch.port || ""}
            onChange={(e) =>
              setCh({
                ...ch,
                port: Math.min(
                  65535,
                  Number(e.target.value.replace(/\D/g, "")) || 0,
                ),
              })
            }
          />
          <label className="flex items-center gap-2 text-sm text-muted">
            <input
              type="checkbox"
              checked={ch.tls}
              onChange={(e) => setCh({ ...ch, tls: e.target.checked })}
            />
            TLS
          </label>
          <p className="w-full text-xs text-faint">{t("adv.custom.login")}</p>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-3">
        <span className="flex items-center gap-1.5 text-sm text-muted">
          {t("adv.backup")}
          <Explain term="backupPool" align="left" />
        </span>
        <select
          aria-label={t("adv.backup")}
          className={cx(inputClass, "w-auto")}
          value={ch.backup}
          onChange={(e) => setCh({ ...ch, backup: e.target.value })}
        >
          <option value="">{t("adv.backup.none")}</option>
          {pools
            .filter((p) => p.pool !== ch.pool && p.needsAddress)
            .map((p) => (
              <option key={p.pool} value={p.pool}>
                {p.name}
              </option>
            ))}
        </select>
      </div>
    </div>
  );
}

const REQ_ICON = {
  ok: <CheckCircle2 className="size-4 shrink-0 text-go" aria-hidden />,
  warn: <CircleAlert className="size-4 shrink-0 text-warn" aria-hidden />,
  missing: <XCircle className="size-4 shrink-0 text-danger" aria-hidden />,
};

/** What mining this coin takes on this PC, met or not: hardware, system, miner, payout. */
function Checklist({ option }: { option: CoinOption }) {
  const t = useT();
  const toast = useApp((s) => s.toast);
  const lang = useApp((s) => s.config?.language ?? "en");
  const { coin } = option;
  const [installing, setInstalling] = useState(false);
  const text = (r: Requirement) =>
    r.status === "missing"
      ? t(`coin.blocker.${r.code}`)
      : t(`req.${r.code}`, { m: MINER_LABEL[coin.miner], c: coin.id });
  const install = async () => {
    setInstalling(true);
    try {
      await api.installMiner(coin.miner);
      toast("info", t("req.installing", { m: MINER_LABEL[coin.miner] }));
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
      setInstalling(false);
    }
  };
  return (
    <div>
      <p className="mb-2 text-xs font-medium tracking-wide text-faint uppercase">
        {t("req.title")}
      </p>
      <ul className="divide-y divide-line rounded-xl border border-line">
        {option.requirements.map((r) => (
          <li
            key={r.kind}
            className="flex min-h-11 items-center gap-2.5 px-3.5 py-2 text-[13px]"
          >
            {REQ_ICON[r.status]}
            <span
              className={cx(
                "min-w-0 flex-1",
                r.status === "ok" ? "text-muted" : "text-fg",
              )}
            >
              {text(r)}
            </span>
            {r.code === "miner_missing" && (
              <Button
                variant="secondary"
                className="h-8 px-3"
                onClick={install}
                disabled={installing}
              >
                <Download className="size-3.5" /> {t("req.install")}
              </Button>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
