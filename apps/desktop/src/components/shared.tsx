import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, Download, FolderOpen, ShieldAlert, ShieldCheck, XCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorText, isRemote, type AppConfig, type MinerKind, type PoolInfo, type WalletError } from "../lib/api";
import { hashrate } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { Badge, Button, Field, inputClass, Progress, Toggle } from "./ui";

/** Wallet address input validated by the Rust backend (debounced). */
export function WalletInput({
  value,
  onChange,
  onValid,
  placeholder,
  hint,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  onValid?: (ok: boolean) => void;
  placeholder?: string;
  hint?: string;
  label?: string;
}) {
  const t = useT();
  const [state, setState] = useState<{ ok: boolean; msg: string } | null>(null);
  useEffect(() => {
    if (!value.trim()) {
      setState(null);
      onValid?.(false);
      return;
    }
    const id = setTimeout(() => {
      api
        .validateWallet(value)
        .then((info) => {
          setState({
            ok: true,
            msg: info.kind === "kryptex" ? t("wallet.ok.kryptex") : info.kind === "bitcoin" ? t("wallet.ok.bitcoin") : `${t("wallet.ok")} · ${info.kind}`,
          });
          onValid?.(true);
        })
        .catch((e: WalletError | string) => {
          const code = typeof e === "string" ? e : e.code;
          setState({ ok: false, msg: t(`wallet.err.${code}`) });
          onValid?.(false);
        });
    }, 250);
    return () => clearTimeout(id);
  }, [value]);
  return (
    <Field
      label={label ?? t("wallet.address")}
      hint={
        state ? (
          <span className={state.ok ? "text-accent" : "text-danger"} role="status">
            {state.ok ? <CheckCircle2 className="mr-1 inline size-3.5" /> : <XCircle className="mr-1 inline size-3.5" />}
            {state.msg}
          </span>
        ) : (
          (hint ?? t("wallet.hint"))
        )
      }
    >
      {/* One line: what matters is the validation below, not reading 95 characters. */}
      <input
        value={value}
        onChange={(e) => onChange(e.target.value.trim())}
        spellCheck={false}
        autoComplete="off"
        placeholder={placeholder ?? "krx… / bc1… / 4…"}
        aria-invalid={state ? !state.ok : undefined}
        className={`${inputClass} font-mono text-xs`}
      />
    </Field>
  );
}

/** Pool selection: curated catalog or custom host/port/TLS, plus worker name. */
export function PoolForm({ pool, onChange }: { pool: AppConfig["pool"]; onChange: (p: AppConfig["pool"]) => void }) {
  const t = useT();
  const [catalog, setCatalog] = useState<PoolInfo[]>([]);
  const [check, setCheck] = useState<{ ok: boolean; msg: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const lang = useApp((s) => s.config?.language ?? "it");
  useEffect(() => {
    api.poolCatalog().then(setCatalog).catch(() => setCatalog([]));
  }, []);
  const custom = pool.id === "custom";
  const pick = (id: string) => {
    const p = catalog.find((c) => c.id === id);
    onChange(p ? { ...pool, id: p.id, host: p.host, port: p.port, tls: p.tls } : { ...pool, id: "custom" });
    setCheck(null);
  };
  const runCheck = async () => {
    setChecking(true);
    try {
      const ms = await api.checkPool(pool.host.trim(), pool.port);
      setCheck({ ok: true, msg: t("onb.pool.ok", { ms }) });
    } catch (e) {
      setCheck({ ok: false, msg: translateError(lang, errorText(e)) });
    } finally {
      setChecking(false);
    }
  };
  return (
    <div className="space-y-4">
      <div className="grid gap-2 sm:grid-cols-2">
        {catalog.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => pick(p.id)}
            aria-pressed={pool.id === p.id}
            className={`rounded-lg border p-3 text-left transition-colors ${pool.id === p.id ? "border-accent bg-accent-soft" : "border-line hover:border-faint"}`}
          >
            <div className="text-sm font-medium">{p.name}</div>
            <div className="mt-0.5 text-xs text-faint">
              {p.host}:{p.port}
              {p.tls ? " · TLS" : ""}
            </div>
          </button>
        ))}
        <button
          type="button"
          onClick={() => pick("custom")}
          aria-pressed={custom}
          className={`rounded-lg border p-3 text-left transition-colors ${custom ? "border-accent bg-accent-soft" : "border-line hover:border-faint"}`}
        >
          <div className="text-sm font-medium">{t("onb.pool.custom")}</div>
          <div className="mt-0.5 text-xs text-faint">host:port</div>
        </button>
      </div>
      {custom && (
        <div className="grid gap-3 sm:grid-cols-[1fr_120px]">
          <Field label={t("onb.pool.host")}>
            <input
              className={inputClass}
              value={pool.host}
              onChange={(e) => onChange({ ...pool, host: e.target.value.trim() })}
              placeholder="pool.example.org"
              spellCheck={false}
            />
          </Field>
          <Field label={t("onb.pool.port")}>
            <input
              className={inputClass}
              type="number"
              min={1}
              max={65535}
              value={pool.port || ""}
              onChange={(e) => onChange({ ...pool, port: Math.max(0, Math.min(65535, Number(e.target.value) || 0)) })}
            />
          </Field>
        </div>
      )}
      {custom && <Toggle checked={pool.tls} onChange={(tls) => onChange({ ...pool, tls })} label={t("onb.pool.tls")} />}
      <Field label={t("onb.pool.worker")} hint="a-z, 0-9, _ -">
        <input
          className={inputClass}
          value={pool.worker}
          maxLength={32}
          onChange={(e) => onChange({ ...pool, worker: e.target.value.replace(/[^A-Za-z0-9_-]/g, "") })}
        />
      </Field>
      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={runCheck} loading={checking} disabled={!pool.host || !pool.port}>
          {t("onb.pool.check")}
        </Button>
        {check && (
          <span className={`text-xs ${check.ok ? "text-accent" : "text-danger"}`} role="status">
            {check.msg}
          </span>
        )}
      </div>
    </div>
  );
}

export function JobProgress() {
  const t = useT();
  const job = useApp((s) => s.status?.job);
  const lang = useApp((s) => s.config?.language ?? "it");
  if (!job) return null;
  return (
    <div className="space-y-2 rounded-lg border border-line bg-bg p-3" aria-live="polite">
      <div className="flex items-center justify-between gap-3 text-sm">
        <span>{job.done ? t("job.done") : t(`job.${job.kind}`)}</span>
        <span className="num text-xs text-muted">
          {job.kind === "benchmark" && job.value != null && hashrate(job.value)}
          {job.kind === "install" && job.value != null && !job.done && `${(job.value / 1e6).toFixed(1)} MB`}
        </span>
      </div>
      <Progress value={job.progress} />
      {job.error && <p className="text-xs text-danger">{translateError(lang, job.error)}</p>}
      {!job.done && (
        <Button variant="ghost" onClick={() => api.cancelJob()} disabled={job.kind === "install"}>
          {t("job.cancel")}
        </Button>
      )}
    </div>
  );
}

/** Install the pinned, hash-verified miner or select an existing binary. */
export function MinerSetup({ kind = "xmrig" }: { kind?: MinerKind }) {
  const t = useT();
  const { info, config, setConfig, toast, status } = useApp();
  const lang = config?.language ?? "it";
  const busy = !!status?.job && !status.job.done;
  if (!info || !config) return null;
  const m = kind === "xmrig" ? config.miner : config.srbminer;
  const label = kind === "xmrig" ? "XMRig" : "SRBMiner-MULTI";
  const pinned = kind === "xmrig" ? info.xmrigPinnedAvailable : info.srbminerPinnedAvailable;
  const pinnedVersion = kind === "xmrig" ? info.xmrigPinnedVersion : info.srbminerPinnedVersion;
  const install = async () => {
    try {
      await api.installMiner(kind);
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    }
  };
  const select = async () => {
    const path = await open({ multiple: false, directory: false, title: label });
    if (typeof path !== "string") return;
    try {
      setConfig(await api.selectMiner(path, kind));
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    }
  };
  return (
    <div className="space-y-4">
      {m.path ? (
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="font-medium">
            {label} {m.version}
          </span>
          {m.verified ? (
            <Badge tone="ok">
              <ShieldCheck className="size-3.5" /> {t("mining.verified")}
            </Badge>
          ) : (
            <Badge tone="warn">
              <ShieldAlert className="size-3.5" /> {t("mining.unverified")}
            </Badge>
          )}
          <span className="w-full truncate font-mono text-xs text-faint" title={m.path}>
            {m.path}
          </span>
        </div>
      ) : (
        <p className="text-sm text-muted">{t("mining.notConfigured")}</p>
      )}
      {!pinned && <p className="text-xs text-faint">{t("mining.install.unavailable", { m: label })}</p>}
      <div className="flex flex-wrap gap-2">
        <Button
          variant={m.path ? "secondary" : "primary"}
          onClick={install}
          disabled={!pinned || busy}
          title={t("mining.install.tip", { v: pinnedVersion })}
        >
          <Download className="size-4" /> {t("mining.install", { m: label })}
        </Button>
        {/* Picking a file only makes sense on the computer you are sitting at. */}
        {!isRemote() && (
          <Button onClick={select} disabled={busy}>
            <FolderOpen className="size-4" /> {t("mining.select", { m: label })}
          </Button>
        )}
      </div>
      {status?.job?.kind === "install" && <JobProgress />}
    </div>
  );
}
