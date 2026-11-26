import { ArrowLeft, Pencil, Play, RotateCcw, ScrollText, Square, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { ActionButton, ConnBadge, DeviceIcon, HistoryChart, kindLabel, MiningBadge, SimBadge, Stat, useAgo, useFleetPolling, useLang } from "../components/fleet";
import { Button, Card, CardTitle, compactInput, ConfirmDialog, cx, Field, inputClass, Notice, Row, Segmented } from "../components/ui";
import { errorText, fleet, type AppConfig, type AsicStatus, type CoinOption, type DeviceKind, type DeviceRow, type FleetEvent, type MiningProfile, type Preset } from "../lib/api";
import { hashrate, money } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/** One device: state, history, configuration, actions, log and operations. */
export default function DeviceDetail() {
  const t = useT();
  const lang = useLang();
  useFleetPolling();
  const { fleet: data, deviceId, setPage, refreshFleet, toast } = useApp();
  const row = data?.devices.find((d) => d.id === deviceId);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [busy, setBusy] = useState("");
  const [rename, setRename] = useState<string | null>(null);
  const ago = useAgo();

  if (!row) {
    return (
      <div>
        <BackLink />
        <p className="text-sm text-faint">{data ? t("err.device_not_found") : "…"}</p>
      </div>
    );
  }
  const act = async (what: string, f: () => Promise<unknown>, ok?: string) => {
    setBusy(what);
    try {
      const r = await f();
      const failed = Array.isArray(r) ? r.find((x: { state: string }) => x.state !== "completed") : null;
      if (failed) toast("error", translateError(lang, String(failed.error ?? failed.state).split(":")[0]));
      else if (ok) toast("info", ok);
      await refreshFleet();
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    } finally {
      setBusy("");
    }
  };
  const isAsic = row.kind === "asic";
  const local = row.kind === "local_pc";
  const fmt = (v: number | null) => (v == null ? "—" : money(v, row.currency, lang));

  return (
    <div>
      <BackLink />
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2.5">
            <DeviceIcon row={row} className="size-6" />
            {rename == null ? (
              <h1 className="truncate font-display text-2xl leading-tight">{row.name}</h1>
            ) : (
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  act("rename", () => fleet.rename(row.id, rename), t("fleet.renamed")).then(() => setRename(null));
                }}
                className="flex items-center gap-2"
              >
                <input autoFocus className={compactInput} value={rename} onChange={(e) => setRename(e.target.value)} aria-label={t("fleet.rename")} maxLength={64} />
                <Button size="sm" type="submit" variant="primary">
                  {t("save")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setRename(null)}>
                  {t("cancel")}
                </Button>
              </form>
            )}
            {rename == null && (
              <button type="button" className="rounded-full p-1.5 text-faint hover:bg-surface-2 hover:text-fg" aria-label={t("fleet.rename")} onClick={() => setRename(row.name)}>
                <Pencil className="size-4" />
              </button>
            )}
          </div>
          <div className="mt-2 flex flex-wrap items-center gap-1.5">
            <ConnBadge conn={row.conn} />
            {row.conn === "online" && <MiningBadge mining={row.mining} />}
            {row.simulated && <SimBadge />}
            <span className="text-xs text-faint">
              {kindLabel(t, row.kind)} · {row.address}
            </span>
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          <ActionButton cap={row.actions.start} variant="go" loading={busy === "start"} onClick={() => act("start", () => fleet.mining([row.id], "start"), t("fleet.started"))}>
            <Play /> {t("fleet.start")}
          </ActionButton>
          <ActionButton cap={row.actions.stop} loading={busy === "stop"} onClick={() => act("stop", () => fleet.mining([row.id], "stop"), t("fleet.stopped"))}>
            <Square /> {t("fleet.stop")}
          </ActionButton>
          <ActionButton cap={row.actions.restart} loading={busy === "restart"} onClick={() => act("restart", () => fleet.mining([row.id], "restart"), t("fleet.restarted"))}>
            <RotateCcw /> {t("fleet.restart")}
          </ActionButton>
          <ActionButton cap={row.actions.remove} variant="danger" onClick={() => setConfirmRemove(true)}>
            <Trash2 /> {t("fleet.remove")}
          </ActionButton>
        </div>
      </div>

      <div className="space-y-6">
        {row.simulated && <Notice tone="warn">{t("fleet.simulated.hint")}</Notice>}
        {row.error && (
          <Notice tone={row.conn === "online" ? "warn" : "err"}>
            {translateError(lang, row.error)}
            {row.conn === "unauthorized" && <span className="mt-1 block text-xs text-muted">{t("fleet.unauthorized.help")}</span>}
          </Notice>
        )}

        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <Stat label={t("fleet.hashrate")} value={row.hashrate != null ? hashrate(row.hashrate) : "—"} sub={row.algo ?? undefined} />
          <Stat label={t("fleet.col.temp")} value={row.tempC != null ? `${Math.round(row.tempC)} °C` : "—"} />
          <Stat label={t("fleet.power")} value={row.powerW != null ? `${Math.round(row.powerW)} W` : "—"} sub={row.powerW != null ? t(row.powerMeasured ? "fleet.power.measured" : "fleet.power.estimated") : t("fleet.unsupported")} />
          <Stat label={t("fleet.netDay")} value={fmt(row.netDay)} sub={`${t("fleet.revenueDay")} ${fmt(row.revenueDay)} · ${t("fleet.costDay")} ${fmt(row.costDay)}`} />
        </div>

        <div className="grid gap-6 lg:grid-cols-2">
          <Card>
            <CardTitle>{t("fleet.general")}</CardTitle>
            <Row label={t("fleet.add.kind")}>{isAsic || local ? kindLabel(t, row.kind) : <KindSelect row={row} />}</Row>
            <Row label={t("fleet.hardware")}>{row.hardware || "—"}</Row>
            {row.os && <Row label={t("fleet.os")}>{row.os}</Row>}
            <Row label={isAsic ? t("fleet.firmware") : t("fleet.agentVersion")}>{row.version ?? "—"}</Row>
            <Row label={t("fleet.coin")}>{row.coin ?? "—"}</Row>
            <Row label={t("fleet.pool")}>{row.pool ?? "—"}</Row>
            <Row label={t("fleet.shares")}>
              {row.accepted} / <span className={cx(row.rejected > 0 && "text-warn")}>{row.rejected}</span>
            </Row>
            <Row label={t("fleet.col.seen")}>{ago(row.lastSeen)}</Row>
            {row.gateway && <Row label={t("fleet.add.via")}>{data?.devices.find((d) => d.id === row.gateway)?.name ?? row.gateway}</Row>}
          </Card>
          <Organize row={row} />
        </div>

        <HistoryChart id={row.id} />

        {isAsic ? <AsicPanel row={row} /> : <ConfigPanel row={row} />}
        {!isAsic && <Logs id={row.id} />}
        <Operations id={row.id} />
      </div>

      <ConfirmDialog
        open={confirmRemove}
        danger
        title={t("fleet.remove.title", { name: row.name })}
        text={isAsic ? t("fleet.remove.asic") : t("fleet.remove.agent")}
        confirmLabel={t("fleet.remove")}
        cancelLabel={t("cancel")}
        onCancel={() => setConfirmRemove(false)}
        onConfirm={() => {
          setConfirmRemove(false);
          act("remove", () => fleet.remove(row.id), t("fleet.removed")).then(() => setPage("devices"));
        }}
      />
    </div>
  );
}

function BackLink() {
  const t = useT();
  const setPage = useApp((s) => s.setPage);
  return (
    <button type="button" onClick={() => setPage("devices")} className="mb-4 inline-flex items-center gap-1.5 text-sm text-muted hover:text-fg">
      <ArrowLeft className="size-4" /> {t("fleet.back")}
    </button>
  );
}

function KindSelect({ row }: { row: DeviceRow }) {
  const t = useT();
  const { refreshFleet } = useApp();
  return (
    <select className={cx(compactInput, "h-8 text-xs")} value={row.kind} onChange={(e) => fleet.setKind(row.id, e.target.value as DeviceKind).then(refreshFleet)} aria-label={t("fleet.add.kind")}>
      {(["remote_pc", "server", "gpu_rig", "cpu_rig"] as const).map((k) => (
        <option key={k} value={k}>
          {kindLabel(t, k)}
        </option>
      ))}
    </select>
  );
}

/** Group, tags and the electricity price used for this device. */
function Organize({ row }: { row: DeviceRow }) {
  const t = useT();
  const lang = useLang();
  const { fleet: data, refreshFleet, toast } = useApp();
  const [tag, setTag] = useState("");
  const [price, setPrice] = useState("");
  const wrap = (p: Promise<unknown>) => p.then(refreshFleet).catch((e) => toast("error", translateError(lang, errorText(e))));
  return (
    <Card>
      <CardTitle>{t("fleet.organize")}</CardTitle>
      <div className="space-y-4">
        <Field label={t("fleet.group")}>
          <select className={inputClass} value={row.group ?? ""} onChange={(e) => wrap(fleet.assignGroup([row.id], e.target.value ? Number(e.target.value) : null))}>
            <option value="">{t("fleet.ungrouped")}</option>
            {data?.groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </Field>
        <div>
          <span className="mb-1.5 block text-sm font-medium text-muted">{t("fleet.tags")}</span>
          <div className="flex flex-wrap items-center gap-1.5">
            {row.tags.map((x) => (
              <button key={x} type="button" title={t("fleet.tags.remove")} onClick={() => wrap(fleet.tag([row.id], [x], true))} className="rounded-full bg-surface-3 px-2 py-0.5 text-xs text-muted hover:bg-danger/14 hover:text-danger">
                #{x} ×
              </button>
            ))}
            <form
              onSubmit={(e) => {
                e.preventDefault();
                if (tag.trim()) wrap(fleet.tag([row.id], [tag.trim()])).then(() => setTag(""));
              }}
            >
              <input className={cx(compactInput, "h-7 w-28 text-xs")} value={tag} onChange={(e) => setTag(e.target.value)} placeholder={t("fleet.tags.ph")} aria-label={t("fleet.tags.add")} />
            </form>
          </div>
        </div>
        <Field label={t("fleet.devicePrice")} hint={t("fleet.devicePrice.hint")}>
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              wrap(fleet.setPrice(row.id, price.trim() === "" ? null : Number(price.replace(",", "."))));
            }}
          >
            <input className={compactInput} inputMode="decimal" value={price} onChange={(e) => setPrice(e.target.value)} placeholder={t("fleet.devicePrice.ph")} />
            <Button size="sm" type="submit">
              {t("save")}
            </Button>
          </form>
        </Field>
      </div>
    </Card>
  );
}

const PRESETS: Preset[] = ["eco", "balanced", "performance"];

/** A computer's mining setup, read from it and saved back (validated by its own engine). */
function ConfigPanel({ row }: { row: DeviceRow }) {
  const t = useT();
  const lang = useLang();
  const { refreshFleet, toast } = useApp();
  const [cfg, setCfg] = useState<AppConfig | null>(null);
  const [coins, setCoins] = useState<CoinOption[]>([]);
  const [profiles, setProfiles] = useState<MiningProfile[]>([]);
  const [json, setJson] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const online = row.conn === "online";
  useEffect(() => {
    if (!online) return;
    fleet.config(row.id).then(setCfg).catch(() => {});
    fleet.query<CoinOption[]>(row.id, "coins").then(setCoins).catch(() => {});
    fleet.profiles().then(setProfiles).catch(() => {});
  }, [row.id, online]);
  if (!online || !cfg) {
    return (
      <Card>
        <CardTitle>{t("fleet.config")}</CardTitle>
        <p className="text-sm text-faint">{online ? "…" : translateError(lang, row.actions.configure.reason ?? "device_offline")}</p>
      </Card>
    );
  }
  const save = async (next: AppConfig) => {
    setSaving(true);
    try {
      await fleet.setConfig(row.id, next);
      setCfg(await fleet.config(row.id));
      toast("info", t("fleet.config.saved"));
      await refreshFleet();
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    } finally {
      setSaving(false);
    }
  };
  return (
    <Card>
      <CardTitle
        action={
          <Button size="sm" variant="ghost" onClick={() => setJson(json == null ? JSON.stringify(cfg, null, 2) : null)}>
            {json == null ? t("fleet.config.json") : t("fleet.config.form")}
          </Button>
        }
        sub={t("fleet.config.sub")}
      >
        {t("fleet.config")}
      </CardTitle>
      {json != null ? (
        <div className="space-y-3">
          <textarea className={cx(inputClass, "num h-72 py-2 text-xs")} value={json} onChange={(e) => setJson(e.target.value)} spellCheck={false} aria-label={t("fleet.config.json")} />
          <div className="flex justify-end">
            <Button
              variant="primary"
              loading={saving}
              onClick={() => {
                try {
                  save(JSON.parse(json));
                } catch {
                  toast("error", t("fleet.config.badJson"));
                }
              }}
            >
              {t("save")}
            </Button>
          </div>
        </div>
      ) : (
        <ConfigForm cfg={cfg} coins={coins} saving={saving} onSave={save} />
      )}
      {profiles.length > 0 && (
        <div className="mt-5 flex flex-wrap items-center gap-2 border-t border-line pt-4">
          <span className="text-sm text-muted">{t("fleet.applyProfile")}</span>
          <select
            className={cx(compactInput, "h-8 text-xs")}
            value=""
            onChange={async (e) => {
              const p = profiles.find((x) => String(x.id) === e.target.value);
              if (!p) return;
              const r = await fleet.applyProfile([row.id], p.id).catch((err) => (toast("error", translateError(lang, errorText(err))), null));
              const res = r?.[0];
              if (res) toast(res.state === "completed" ? "info" : "error", res.state === "completed" ? t("fleet.profileApplied", { name: p.name }) : translateError(lang, (res.error ?? res.state).split(":")[0]));
              setCfg(await fleet.config(row.id));
              refreshFleet();
            }}
            aria-label={t("fleet.applyProfile")}
          >
            <option value="">—</option>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </div>
      )}
    </Card>
  );
}

function ConfigForm({ cfg, coins, saving, onSave }: { cfg: AppConfig; coins: CoinOption[]; saving: boolean; onSave: (c: AppConfig) => void }) {
  const t = useT();
  const [d, setD] = useState(cfg);
  useEffect(() => setD(cfg), [cfg]);
  const changed = JSON.stringify(d) !== JSON.stringify(cfg);
  return (
    <div className="space-y-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label={t("fleet.coin")}>
          <select className={inputClass} value={d.coin} onChange={(e) => setD({ ...d, coin: e.target.value })}>
            <option value="AUTO">{t("fleet.coin.auto")}</option>
            {coins.map((c) => (
              <option key={c.coin.id} value={c.coin.id} disabled={!!c.blocker && c.coin.id !== d.coin}>
                {c.coin.name} ({c.coin.algo}){c.blocker ? ` — ${t("fleet.coin.blocked")}` : ""}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("fleet.worker")}>
          <input className={inputClass} value={d.pool.worker} onChange={(e) => setD({ ...d, pool: { ...d.pool, worker: e.target.value } })} spellCheck={false} />
        </Field>
      </div>
      <Field label={t("fleet.wallet")} hint={t("fleet.wallet.hint")}>
        <input className={cx(inputClass, "num")} value={d.wallet} onChange={(e) => setD({ ...d, wallet: e.target.value })} spellCheck={false} autoComplete="off" />
      </Field>
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <span className="mb-1.5 block text-sm font-medium text-muted">{t("fleet.preset")}</span>
          <Segmented
            size="sm"
            label={t("fleet.preset")}
            value={PRESETS.includes(d.performance.preset) ? d.performance.preset : "balanced"}
            onChange={(preset) => setD({ ...d, performance: { ...d.performance, preset } })}
            options={PRESETS.map((p) => ({ value: p, label: t(`hw.mode.${p}`) }))}
          />
        </div>
        <label className="flex items-center gap-2 text-sm text-muted">
          {t("hw.maxTemp")}
          <input type="number" min={60} max={95} className={cx(compactInput, "w-20")} value={d.safety.maxTempC} onChange={(e) => setD({ ...d, safety: { ...d.safety, maxTempC: Number(e.target.value) } })} />
          °C
        </label>
        <Button variant="primary" disabled={!changed} loading={saving} onClick={() => onSave(d)}>
          {t("save")}
        </Button>
      </div>
    </div>
  );
}

/** ASIC: hashboards, fans, pools, and a new pool (wallet and worker in its user field). */
function AsicPanel({ row }: { row: DeviceRow }) {
  const t = useT();
  const lang = useLang();
  const { refreshFleet, toast } = useApp();
  const [st, setSt] = useState<AsicStatus | null>(null);
  const [pool, setPool] = useState({ url: "", user: "", password: "" });
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const load = () => fleet.query<AsicStatus | null>(row.id, "asic").then(setSt).catch(() => {});
    load();
    const i = setInterval(load, 15_000);
    return () => clearInterval(i);
  }, [row.id]);
  const setNew = async () => {
    setBusy(true);
    try {
      const [r] = await fleet.applyPatch([row.id], { name: t("fleet.asic.changePool"), config: {}, asic: pool, algo: null, start: false });
      if (r.state === "completed") toast("info", t("fleet.asic.poolSet"));
      else toast("error", translateError(lang, (r.error ?? r.state).split(":")[0]));
      await refreshFleet();
      setSt(await fleet.query<AsicStatus | null>(row.id, "asic"));
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <Card>
        <CardTitle sub={st?.model ?? undefined}>{t("fleet.asic.boards")}</CardTitle>
        {!st?.boards.length ? (
          <p className="text-sm text-faint">{t("fleet.unsupported")}</p>
        ) : (
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-faint">
              <tr>
                <th className="py-1 font-medium">#</th>
                <th className="py-1 text-right font-medium">{t("fleet.hashrate")}</th>
                <th className="py-1 text-right font-medium">{t("fleet.asic.board")}</th>
                <th className="py-1 text-right font-medium">{t("fleet.asic.chip")}</th>
                <th className="py-1 text-right font-medium">{t("fleet.asic.chips")}</th>
              </tr>
            </thead>
            <tbody className="num">
              {st.boards.map((b) => (
                <tr key={b.index} className="border-t border-line">
                  <td className="py-1.5">{b.index}</td>
                  <td className="text-right">{hashrate(b.hashrate)}</td>
                  <td className="text-right">{b.tempC != null ? `${Math.round(b.tempC)}°` : "—"}</td>
                  <td className="text-right">{b.chipTempC != null ? `${Math.round(b.chipTempC)}°` : "—"}</td>
                  <td className="text-right">{b.chips ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <div className="mt-4 text-sm">
          <Row label={t("fleet.asic.fans")}>{st?.fansRpm.length ? st.fansRpm.map((f) => `${f} rpm`).join(" · ") : "—"}</Row>
          <Row label={t("fleet.asic.hwErrors")}>{st?.hardwareErrors ?? "—"}</Row>
          <Row label={t("fleet.asic.api")}>{st ? `${st.vendor}${st.apiVersion ? ` · API ${st.apiVersion}` : ""}` : "—"}</Row>
        </div>
      </Card>
      <Card>
        <CardTitle sub={t("fleet.asic.pools.sub")}>{t("fleet.asic.pools")}</CardTitle>
        <ul className="divide-y divide-line text-sm">
          {st?.pools.map((p) => (
            <li key={p.index} className="py-2">
              <div className="flex items-center justify-between gap-2">
                <span className="num truncate">{p.url}</span>
                {p.active && <span className="text-xs font-medium text-go">{t("fleet.asic.active")}</span>}
              </div>
              <div className="num truncate text-xs text-faint">
                {p.user} · {p.status}
              </div>
            </li>
          ))}
        </ul>
        <div className="mt-4 space-y-3 border-t border-line pt-4">
          <Field label={t("fleet.asic.url")}>
            <input className={inputClass} value={pool.url} onChange={(e) => setPool({ ...pool, url: e.target.value })} placeholder="stratum+tcp://pool.example:3333" spellCheck={false} />
          </Field>
          <div className="grid gap-3 sm:grid-cols-[2fr_1fr]">
            <Field label={t("fleet.asic.user")} hint={t("fleet.asic.user.hint")}>
              <input className={inputClass} value={pool.user} onChange={(e) => setPool({ ...pool, user: e.target.value })} placeholder="bc1q….rig01" spellCheck={false} />
            </Field>
            <Field label={t("fleet.asic.password")}>
              <input className={inputClass} value={pool.password} onChange={(e) => setPool({ ...pool, password: e.target.value })} placeholder="x" />
            </Field>
          </div>
          <div className="flex justify-end">
            <ActionButton cap={row.actions.setPool} variant="primary" loading={busy} disabled={!pool.url || !pool.user} onClick={setNew}>
              {t("fleet.asic.changePool")}
            </ActionButton>
          </div>
        </div>
      </Card>
    </div>
  );
}

function Logs({ id }: { id: string }) {
  const t = useT();
  const [lines, setLines] = useState<string[] | null>(null);
  const load = () => fleet.query<string[]>(id, "logs").then(setLines).catch(() => setLines([]));
  return (
    <Card>
      <CardTitle
        icon={<ScrollText />}
        action={
          <Button size="sm" variant="ghost" onClick={load}>
            {lines ? t("fleet.logs.refresh") : t("fleet.logs.show")}
          </Button>
        }
      >
        {t("fleet.logs")}
      </CardTitle>
      {lines && (
        <pre className="num max-h-72 overflow-auto rounded-xl bg-surface-2 p-3 text-2xs leading-relaxed text-muted">
          {lines.length ? lines.join("\n") : t("fleet.logs.empty")}
        </pre>
      )}
    </Card>
  );
}

function Operations({ id }: { id: string }) {
  const t = useT();
  const ago = useAgo();
  const lang = useLang();
  const [ev, setEv] = useState<FleetEvent[]>([]);
  useEffect(() => {
    fleet.events(0, 500).then((e) => setEv(e.filter((x) => x.deviceId === id).slice(0, 30))).catch(() => {});
  }, [id]);
  return (
    <Card>
      <CardTitle>{t("fleet.history")}</CardTitle>
      {ev.length === 0 ? (
        <p className="text-sm text-faint">{t("fleet.history.empty")}</p>
      ) : (
        <ul className="divide-y divide-line text-sm">
          {ev.map((e) => (
            <li key={e.id} className="flex items-baseline justify-between gap-3 py-2">
              <span className={cx(e.level === "error" ? "text-danger" : e.level === "warning" ? "text-warn" : "text-fg")}>
                {t(`fleet.ev.${e.code}`) === `fleet.ev.${e.code}` ? e.code : t(`fleet.ev.${e.code}`)}
                {e.detail && <span className="text-faint"> · {translateError(lang, e.detail)}</span>}
              </span>
              <span className="num shrink-0 text-xs text-faint">{ago(e.ts)}</span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
