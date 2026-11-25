import { ShieldAlert, ShieldCheck, Timer } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Group, Item, Sub } from "./rows";
import { DefenderConsent, JobProgress, PoolForm, useMiner } from "./shared";
import { Badge, Button, ConfirmDialog, Explain } from "./ui";
import { api, errorText, type AppConfig, type MinerKind } from "../lib/api";
import { dateTime, hashrate } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/** For people who already mine (Settings › Advanced): miners, the Monero pool, benchmark, log. */
export function MinerTools() {
  const t = useT();
  return (
    <>
      <div className="space-y-3">
        <DefenderConsent />
        <Group title={t("mining.installed")} help="miner">
          {(["xmrig", "cpuminer", "srbminer", "lolminer"] as const).map((k) => (
            <MinerRow key={k} kind={k} />
          ))}
        </Group>
      </div>
      <PoolGroup />
      <BenchmarkGroup />
      <MinerLog />
    </>
  );
}

/** A miner on one line: version and whether Erebor verified it, install/update, or pick a file. */
function MinerRow({ kind }: { kind: MinerKind }) {
  const t = useT();
  const job = useApp((s) => s.status?.job);
  const miner = useMiner(kind);
  if (!miner) return null;
  const { m, label, pinned, pinnedVersion, outdated, busy, install, select } = miner;
  const installing = job?.kind === "install" && !job.done;
  return (
    <>
      <div className="flex min-h-13 flex-wrap items-center gap-x-4 gap-y-2 px-4 py-2.5">
        <span className="min-w-0 flex-1">
          <span className="flex flex-wrap items-center gap-2 text-sm">
            <span className="font-medium">{label}</span>
            {m.path ? (
              <>
                <span className="num text-faint">{m.version}</span>
                {m.verified ? (
                  <Badge tone="ok">
                    <ShieldCheck /> {t("mining.verified.short")}
                  </Badge>
                ) : (
                  <Badge tone="warn">
                    <ShieldAlert /> {t("mining.unverified.short")}
                  </Badge>
                )}
              </>
            ) : (
              <span className="text-faint">{t("mining.notConfigured")}</span>
            )}
          </span>
          {m.path ? (
            <span className="mt-0.5 block truncate font-mono text-xs text-faint" title={m.path}>
              {m.path}
            </span>
          ) : (
            !pinned && (
              <span className="mt-0.5 block text-xs text-faint">
                {t("mining.install.unavailable", { m: label })}
              </span>
            )
          )}
        </span>
        <span className="flex shrink-0 gap-1.5">
          <Button size="sm" variant="ghost" onClick={select} disabled={busy}>
            {t("mining.select.short")}
          </Button>
          {pinned && (!m.path || outdated || !m.verified) && (
            <Button
              size="sm"
              variant={m.path && !outdated ? "secondary" : "primary"}
              onClick={install}
              disabled={busy}
              title={t("mining.install.tip", { v: pinnedVersion })}
            >
              {outdated ? t("mining.update.short", { v: pinnedVersion }) : t("mining.install.short")}
            </Button>
          )}
        </span>
      </div>
      {installing && (
        <Sub>
          <JobProgress />
        </Sub>
      )}
    </>
  );
}

/** Monero (CPU) pool: graphics-card coins always use Kryptex or their own pool. */
function PoolGroup() {
  const t = useT();
  const { config, saveConfig, toast } = useApp();
  const [pool, setPool] = useState<AppConfig["pool"]>(config!.pool);
  const [confirm, setConfirm] = useState(false);
  const changed = JSON.stringify(pool) !== JSON.stringify(config!.pool);
  const savePool = async () => {
    setConfirm(false);
    try {
      await saveConfig({ ...config!, pool });
      toast("info", t("settings.saved"));
    } catch (e) {
      toast("error", translateError(config!.language, errorText(e)));
    }
  };
  return (
    <section aria-labelledby="pool">
      <div className="mb-2.5 flex min-h-7 items-center justify-between gap-3 px-1">
        <h2 id="pool" className="flex items-center gap-1.5 text-sm font-semibold">
          {t("mining.pool")}
          <Explain term="pool" align="left" />
        </h2>
        {changed && (
          <Button size="sm" variant="primary" onClick={() => setConfirm(true)}>
            {t("wallet.save")}
          </Button>
        )}
      </div>
      <div className="rounded-2xl border border-line bg-surface p-4 shadow-[var(--highlight)]">
        <p className="mb-3 text-xs text-faint">{t("mining.pool.xmrOnly")}</p>
        <PoolForm pool={pool} onChange={setPool} />
      </div>
      <ConfirmDialog
        open={confirm}
        title={t("confirm.pool.title")}
        text={t("confirm.pool.body", { p: `${pool.host}:${pool.port}` })}
        confirmLabel={t("confirm.pool.ok")}
        cancelLabel={t("confirm.cancel")}
        onConfirm={savePool}
        onCancel={() => setConfirm(false)}
      />
    </section>
  );
}

function BenchmarkGroup() {
  const t = useT();
  const { status, config, toast } = useApp();
  const c = config!;
  const locale = c.language === "it" ? "it-IT" : "en-US";
  const active = status?.state === "running" || status?.state === "paused";
  const jobBusy = !!status?.job && !status.job.done;
  const share = {
    eco: 25,
    balanced: 50,
    performance: 100,
    custom: c.performance.customThreadsPercent,
  }[c.performance.preset];
  const run = () =>
    api.runBenchmark().catch((e) => toast("error", translateError(c.language, errorText(e))));
  return (
    <Group title={t("mining.benchmark")}>
      <Item
        icon={<Timer />}
        tone="blue"
        label={
          c.benchmark
            ? `${hashrate(c.benchmark.hashrate)} · ${c.benchmark.threadsPercent}%`
            : t("mining.benchmark.never")
        }
        hint={
          c.benchmark
            ? `${t("mining.benchmark.last")} · ${dateTime(c.benchmark.at, locale)}`
            : t("mining.benchmark.desc", { s: 90, p: share })
        }
      >
        <Button
          size="sm"
          onClick={run}
          disabled={active || jobBusy || !c.miner.path || c.demoMode}
        >
          {t("mining.benchmark.run")}
        </Button>
      </Item>
      {status?.job?.kind === "benchmark" && (
        <Sub>
          <JobProgress />
        </Sub>
      )}
    </Group>
  );
}

/** The miner's own output, folded away: opened when something looks wrong. */
function MinerLog() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [logs, setLogs] = useState<string[]>([]);
  const ref = useRef<HTMLPreElement>(null);
  useEffect(() => {
    if (!open) return;
    const load = () => api.getLogs().then(setLogs).catch(() => {});
    load();
    const id = setInterval(load, 3000);
    return () => clearInterval(id);
  }, [open]);
  useEffect(() => {
    const el = ref.current;
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 80)
      el.scrollTop = el.scrollHeight;
  }, [logs]);
  return (
    <details
      className="group/log rounded-2xl border border-line bg-surface shadow-[var(--highlight)]"
      onToggle={(e) => setOpen(e.currentTarget.open)}
    >
      <summary className="flex min-h-12 cursor-default list-none items-center justify-between gap-3 px-4 text-sm font-semibold [&::-webkit-details-marker]:hidden">
        {t("mining.logs")}
        <span className="text-xs font-normal text-faint group-open/log:hidden">{t("mining.logs.show")}</span>
      </summary>
      <div className="border-t border-line p-3">
        {logs.length === 0 ? (
          <p className="px-1 py-4 text-center text-sm text-faint">{t("mining.logs.empty")}</p>
        ) : (
          <pre
            ref={ref}
            className="max-h-80 overflow-auto rounded-lg bg-bg p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap text-muted"
            aria-live="off"
          >
            {logs.slice(-300).join("\n")}
          </pre>
        )}
      </div>
    </details>
  );
}
