import { useEffect, useRef, useState } from "react";
import {
  DefenderConsent,
  JobProgress,
  MinerSetup,
  PoolForm,
} from "../components/shared";
import {
  Button,
  Card,
  CardTitle,
  ConfirmDialog,
  Empty,
  Row,
} from "../components/ui";
import { api, errorText, type AppConfig } from "../lib/api";
import { dateTime, hashrate } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/** Settings → Advanced: miners, pool, benchmark and logs (the status is in the status bar). */
export default function Mining() {
  const t = useT();
  const { status, config, saveConfig, toast } = useApp();
  const [pool, setPool] = useState<AppConfig["pool"]>(config!.pool);
  const [logs, setLogs] = useState<string[]>([]);
  const [confirmPool, setConfirmPool] = useState(false);
  const logRef = useRef<HTMLPreElement>(null);
  const lang = config!.language;
  const locale = lang === "it" ? "it-IT" : "en-US";
  const active = status?.state === "running" || status?.state === "paused";
  const jobBusy = !!status?.job && !status.job.done;

  useEffect(() => {
    const load = () =>
      api
        .getLogs()
        .then(setLogs)
        .catch(() => {});
    load();
    const id = setInterval(load, 3000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    const el = logRef.current;
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 80)
      el.scrollTop = el.scrollHeight;
  }, [logs]);

  const fail = (e: unknown) =>
    toast("error", translateError(lang, errorText(e)));

  const savePool = async () => {
    setConfirmPool(false);
    try {
      await saveConfig({ ...config!, pool });
      toast("info", t("settings.saved"));
    } catch (e) {
      fail(e);
    }
  };

  return (
    <>
      <div className="grid gap-4">
        <Card>
          <CardTitle help="miner">{t("mining.installed")}</CardTitle>
          <div className="space-y-6">
            <DefenderConsent />
            <div>
              <p className="mb-2 text-xs text-faint">CPU</p>
              <MinerSetup kind="xmrig" />
              <div className="mt-4">
                <MinerSetup kind="cpuminer" />
              </div>
            </div>
            <div>
              <p className="mb-2 text-xs text-faint">GPU</p>
              <MinerSetup kind="srbminer" />
              <div className="mt-4">
                <MinerSetup kind="lolminer" />
              </div>
            </div>
          </div>
        </Card>

        <Card>
          <CardTitle help="pool">{t("mining.pool")}</CardTitle>
          <p className="mb-3 text-xs text-faint">{t("mining.pool.xmrOnly")}</p>
          <PoolForm pool={pool} onChange={setPool} />
          <div className="mt-4">
            <Button
              variant="primary"
              onClick={() => setConfirmPool(true)}
              disabled={JSON.stringify(pool) === JSON.stringify(config!.pool)}
            >
              {t("wallet.save")}
            </Button>
          </div>
        </Card>

        <Card>
          <CardTitle>{t("mining.benchmark")}</CardTitle>
          <p className="mb-4 text-sm leading-relaxed text-muted">
            {t("mining.benchmark.desc", {
              s: 90,
              p: {
                eco: 25,
                balanced: 50,
                performance: 100,
                custom: config!.performance.customThreadsPercent,
              }[config!.performance.preset],
            })}
          </p>
          {config!.benchmark && (
            <Row label={t("mining.benchmark.last")}>
              {hashrate(config!.benchmark.hashrate)} ·{" "}
              {config!.benchmark.threadsPercent}% ·{" "}
              {dateTime(config!.benchmark.at, locale)}
            </Row>
          )}
          {status?.job?.kind === "benchmark" && (
            <div className="my-3">
              <JobProgress />
            </div>
          )}
          <Button
            className="mt-3"
            onClick={() => api.runBenchmark().catch(fail)}
            disabled={
              active || jobBusy || !config!.miner.path || config!.demoMode
            }
          >
            {t("mining.benchmark.run")}
          </Button>
        </Card>
      </div>

      <Card className="mt-4">
        <CardTitle>{t("mining.logs")}</CardTitle>
        {logs.length === 0 ? (
          <Empty>{t("mining.logs.empty")}</Empty>
        ) : (
          <pre
            ref={logRef}
            className="max-h-80 overflow-auto rounded-lg bg-bg p-3 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap text-muted"
            aria-live="off"
          >
            {logs.slice(-300).join("\n")}
          </pre>
        )}
      </Card>
      <ConfirmDialog
        open={confirmPool}
        title={t("confirm.pool.title")}
        text={t("confirm.pool.body", { p: `${pool.host}:${pool.port}` })}
        confirmLabel={t("confirm.pool.ok")}
        cancelLabel={t("confirm.cancel")}
        onConfirm={savePool}
        onCancel={() => setConfirmPool(false)}
      />
    </>
  );
}
