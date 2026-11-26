import { Pencil, Plus, Send, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { ResultsModal, useFleetPolling, useLang } from "../components/fleet";
import { PageTitle } from "../components/Layout";
import { Button, compactInput, ConfirmDialog, cx, Empty, Field, inputClass, Modal, Toggle } from "../components/ui";
import { api, errorText, fleet, type MiningProfile, type OpResult, type PoolInfo } from "../lib/api";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

const EMPTY: MiningProfile = { id: 0, name: "", config: {}, asic: null, algo: null, start: false };

/** Reusable setups: saved once, applied to a device, a selection, a group or everything. */
export default function Profiles() {
  const t = useT();
  const lang = useLang();
  useFleetPolling();
  const { fleet: data, toast } = useApp();
  const [list, setList] = useState<MiningProfile[]>([]);
  const [edit, setEdit] = useState<MiningProfile | null>(null);
  const [apply, setApply] = useState<MiningProfile | null>(null);
  const [del, setDel] = useState<MiningProfile | null>(null);
  const [results, setResults] = useState<OpResult[] | null>(null);
  const load = () => fleet.profiles().then(setList).catch(() => {});
  useEffect(() => {
    load();
  }, []);
  const err = (e: unknown) => toast("error", translateError(lang, errorText(e).split(":")[0]));
  return (
    <div className="max-w-3xl">
      <PageTitle
        sub={t("fleet.profiles.sub")}
        action={
          <Button variant="primary" onClick={() => setEdit(EMPTY)}>
            <Plus /> {t("fleet.profiles.new")}
          </Button>
        }
      >
        {t("nav.profiles")}
      </PageTitle>
      {list.length === 0 ? (
        <Empty>{t("fleet.profiles.empty")}</Empty>
      ) : (
        <ul className="divide-y divide-line overflow-hidden rounded-2xl border border-line bg-surface shadow-[var(--highlight)]">
          {list.map((p) => (
            <li key={p.id} className="flex flex-wrap items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <div className="font-medium">{p.name}</div>
                <div className="truncate text-xs text-faint">{summary(t, p)}</div>
              </div>
              <Button size="sm" variant="primary" onClick={() => setApply(p)}>
                <Send /> {t("fleet.profiles.apply")}
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setEdit(p)} aria-label={t("fleet.profiles.edit")}>
                <Pencil />
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setDel(p)} aria-label={t("fleet.profiles.delete")}>
                <Trash2 />
              </Button>
            </li>
          ))}
        </ul>
      )}
      {edit && (
        <Editor
          p={edit}
          onClose={() => setEdit(null)}
          onSave={async (p) => {
            try {
              await fleet.saveProfile(p);
              setEdit(null);
              load();
            } catch (e) {
              err(e);
            }
          }}
        />
      )}
      {apply && data && (
        <ApplyDialog
          p={apply}
          onClose={() => setApply(null)}
          onApply={async (ids) => {
            setApply(null);
            try {
              setResults(await fleet.applyProfile(ids, apply.id));
            } catch (e) {
              err(e);
            }
          }}
        />
      )}
      <ResultsModal results={results} onClose={() => setResults(null)} />
      <ConfirmDialog
        open={!!del}
        danger
        title={t("fleet.profiles.delete.title", { name: del?.name ?? "" })}
        text={t("fleet.profiles.delete.text")}
        confirmLabel={t("fleet.profiles.delete")}
        cancelLabel={t("cancel")}
        onCancel={() => setDel(null)}
        onConfirm={() => {
          const p = del!;
          setDel(null);
          fleet.deleteProfile(p.id).then(load).catch(err);
        }}
      />
    </div>
  );
}

function summary(t: ReturnType<typeof useT>, p: MiningProfile) {
  const c = p.config as Record<string, any>;
  const parts = [
    c.coin && `${t("fleet.coin")} ${c.coin === "AUTO" ? t("fleet.coin.auto") : c.coin}`,
    c.pool?.host && `${t("fleet.pool")} ${c.pool.host}`,
    c.wallet && t("fleet.wallet"),
    c.performance?.preset && t(`hw.mode.${c.performance.preset}`),
    p.asic && `ASIC ${p.asic.url}`,
    p.start && t("fleet.profiles.startsMining"),
  ].filter(Boolean);
  return parts.join(" · ") || "—";
}

function Editor({ p, onClose, onSave }: { p: MiningProfile; onClose: () => void; onSave: (p: MiningProfile) => void }) {
  const t = useT();
  const c = p.config as Record<string, any>;
  const [name, setName] = useState(p.name);
  const [coin, setCoin] = useState<string>(c.coin ?? "");
  const [wallet, setWallet] = useState<string>(c.wallet ?? "");
  const [worker, setWorker] = useState<string>(c.pool?.worker ?? "");
  const [pool, setPool] = useState<string>(c.pool?.id ?? "");
  const [preset, setPreset] = useState<string>(c.performance?.preset ?? "");
  const [maxTemp, setMaxTemp] = useState<string>(c.safety?.maxTempC?.toString() ?? "");
  const [autoRestart, setAutoRestart] = useState<boolean>(!!c.safety?.autoRestart);
  const [start, setStart] = useState(p.start);
  const [asicUrl, setAsicUrl] = useState(p.asic?.url ?? "");
  const [asicUser, setAsicUser] = useState(p.asic?.user ?? "");
  const [asicPass, setAsicPass] = useState(p.asic?.password ?? "");
  const [algo, setAlgo] = useState(p.algo ?? "");
  const [catalog, setCatalog] = useState<PoolInfo[]>([]);
  useEffect(() => {
    api.poolCatalog().then(setCatalog).catch(() => {});
  }, []);
  const build = (): MiningProfile => {
    const config: Record<string, any> = {};
    const poolInfo = catalog.find((x) => x.id === pool);
    if (coin.trim()) config.coin = coin.trim().toUpperCase();
    if (wallet.trim()) config.wallet = wallet.trim();
    if (poolInfo || worker.trim()) config.pool = { ...(poolInfo ? { id: poolInfo.id, host: poolInfo.host, port: poolInfo.port, tls: poolInfo.tls } : {}), ...(worker.trim() ? { worker: worker.trim() } : {}) };
    if (preset) config.performance = { preset };
    if (maxTemp.trim() || autoRestart) config.safety = { ...(maxTemp.trim() ? { maxTempC: Number(maxTemp) } : {}), ...(autoRestart ? { autoRestart: true } : {}) };
    return { id: p.id, name: name.trim(), config, asic: asicUrl.trim() ? { url: asicUrl.trim(), user: asicUser.trim(), password: asicPass } : null, algo: algo.trim() || null, start };
  };
  return (
    <Modal
      open
      wide
      onClose={onClose}
      closeLabel={t("close")}
      title={p.id ? t("fleet.profiles.edit") : t("fleet.profiles.new")}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" disabled={!name.trim()} onClick={() => onSave(build())}>
            {t("save")}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Field label={t("fleet.profiles.name")}>
          <input className={inputClass} value={name} onChange={(e) => setName(e.target.value)} placeholder={t("fleet.profiles.name.ph")} />
        </Field>
        <h3 className="pt-2 text-sm font-semibold text-fg">{t("fleet.profiles.computers")}</h3>
        <p className="-mt-2 text-xs text-faint">{t("fleet.profiles.computers.hint")}</p>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={t("fleet.coin")} hint={t("fleet.profiles.coin.hint")}>
            <input className={inputClass} value={coin} onChange={(e) => setCoin(e.target.value)} placeholder="AUTO, XMR, RVN…" spellCheck={false} />
          </Field>
          <Field label={t("fleet.pool")}>
            <select className={inputClass} value={pool} onChange={(e) => setPool(e.target.value)}>
              <option value="">{t("fleet.profiles.keep")}</option>
              {catalog.map((x) => (
                <option key={x.id} value={x.id}>
                  {x.name}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <Field label={t("fleet.wallet")} hint={t("fleet.wallet.hint")}>
          <input className={cx(inputClass, "num")} value={wallet} onChange={(e) => setWallet(e.target.value)} spellCheck={false} autoComplete="off" />
        </Field>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label={t("fleet.worker")} hint={t("fleet.profiles.worker.hint")}>
            <input className={inputClass} value={worker} onChange={(e) => setWorker(e.target.value)} placeholder="{device}" spellCheck={false} />
          </Field>
          <Field label={t("fleet.preset")}>
            <select className={inputClass} value={preset} onChange={(e) => setPreset(e.target.value)}>
              <option value="">{t("fleet.profiles.keep")}</option>
              {(["eco", "balanced", "performance"] as const).map((x) => (
                <option key={x} value={x}>
                  {t(`hw.mode.${x}`)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("hw.maxTemp")}>
            <input className={inputClass} inputMode="numeric" value={maxTemp} onChange={(e) => setMaxTemp(e.target.value.replace(/\D/g, ""))} placeholder="85" />
          </Field>
        </div>
        <Toggle checked={autoRestart} onChange={setAutoRestart} label={t("fleet.profiles.autoRestart")} />
        <Toggle checked={start} onChange={setStart} label={t("fleet.profiles.start")} description={t("fleet.profiles.start.hint")} />
        <h3 className="pt-2 text-sm font-semibold text-fg">{t("fleet.profiles.asics")}</h3>
        <Field label={t("fleet.asic.url")}>
          <input className={inputClass} value={asicUrl} onChange={(e) => setAsicUrl(e.target.value)} placeholder="stratum+tcp://pool.example:3333" spellCheck={false} />
        </Field>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label={t("fleet.asic.user")}>
            <input className={inputClass} value={asicUser} onChange={(e) => setAsicUser(e.target.value)} placeholder="bc1q….{device}" spellCheck={false} />
          </Field>
          <Field label={t("fleet.asic.password")}>
            <input className={inputClass} value={asicPass} onChange={(e) => setAsicPass(e.target.value)} placeholder="x" />
          </Field>
          <Field label={t("fleet.profiles.algo")}>
            <input className={inputClass} value={algo} onChange={(e) => setAlgo(e.target.value)} placeholder="sha256d" spellCheck={false} />
          </Field>
        </div>
      </div>
    </Modal>
  );
}

/** Where to apply: a group, every device, or picked devices. */
function ApplyDialog({ p, onClose, onApply }: { p: MiningProfile; onClose: () => void; onApply: (ids: string[]) => void }) {
  const t = useT();
  const data = useApp((s) => s.fleet)!;
  const [target, setTarget] = useState<string>("all");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const ids =
    target === "all"
      ? data.devices.map((d) => d.id)
      : target === "pick"
        ? [...picked]
        : data.devices.filter((d) => String(d.group) === target).map((d) => d.id);
  return (
    <Modal
      open
      onClose={onClose}
      closeLabel={t("close")}
      title={t("fleet.applyProfileNamed", { name: p.name })}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" disabled={!ids.length} onClick={() => onApply(ids)}>
            {t("fleet.profiles.applyTo", { n: ids.length })}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <select className={inputClass} value={target} onChange={(e) => setTarget(e.target.value)} aria-label={t("fleet.profiles.target")}>
          <option value="all">{t("fleet.profiles.target.all")}</option>
          {data.groups.map((g) => (
            <option key={g.id} value={g.id}>
              {t("fleet.group")}: {g.name}
            </option>
          ))}
          <option value="pick">{t("fleet.profiles.target.pick")}</option>
        </select>
        {target === "pick" && (
          <ul className="max-h-60 space-y-1 overflow-y-auto">
            {data.devices.map((d) => (
              <li key={d.id}>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    className={cx(compactInput, "size-4 accent-[var(--accent)]")}
                    checked={picked.has(d.id)}
                    onChange={() => {
                      const s = new Set(picked);
                      if (s.has(d.id)) s.delete(d.id);
                      else s.add(d.id);
                      setPicked(s);
                    }}
                  />
                  {d.name}
                </label>
              </li>
            ))}
          </ul>
        )}
        <p className="text-xs text-faint">{t("fleet.profiles.applyNote")}</p>
      </div>
    </Modal>
  );
}
