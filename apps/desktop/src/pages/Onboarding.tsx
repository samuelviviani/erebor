import { ArrowLeft, ArrowRight, Bitcoin, Check, Cpu, Gpu, Loader2, Power, Sparkles, Zap } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Logo } from "../components/Layout";
import { PayoutChoice, payoutMode } from "../components/PayoutChoice";
import { JobProgress, MinerSetup } from "../components/shared";
import { Button, cx, Row, Segmented } from "../components/ui";
import { api, errorText, type AppConfig, type HardwareInfo, type PoolInfo } from "../lib/api";
import { shortAddress } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

// Three questions at most: where earnings go, prepare the PC, start.
const STEPS = ["welcome", "account", "pc", "ready"] as const;
type Step = (typeof STEPS)[number];

export default function Onboarding() {
  const t = useT();
  const { config, saveConfig, toast, info, status, refreshStatus } = useApp();
  const [draft, setDraft] = useState<AppConfig>(config!);
  const [step, setStep] = useState<Step>("welcome");
  const [hw, setHw] = useState<HardwareInfo | null>(null);
  const [catalog, setCatalog] = useState<PoolInfo[]>([]);
  const [walletOk, setWalletOk] = useState(false);
  const [saving, setSaving] = useState(false);
  const lang = draft.language;
  const demo = draft.demoMode;
  const live = config!; // backend copy: the installed miner lands there directly
  const job = status?.job;
  const installing = job?.kind === "install" && !job.done;

  useEffect(() => {
    api.poolCatalog().then(setCatalog).catch(() => {});
  }, []);

  // Preparing the PC: detect hardware and install the pinned miners without asking:
  // XMRig for the CPU, plus SRBMiner when there is a usable GPU and a Kryptex ID.
  const tried = useRef<Set<string>>(new Set());
  const gpu = hw?.gpus.find((g) => /nvidia|geforce|amd|radeon/i.test(`${g.vendor ?? ""} ${g.name}`));
  const wantsGpu = !!gpu && /^(krx|bc1|[13])/i.test(draft.wallet.trim()) && !!info?.srbminerPinnedAvailable;
  useEffect(() => {
    if (step !== "pc") return;
    if (!hw) {
      api.detectHardware().then(setHw).catch(() => {});
      return;
    }
    if (installing) return;
    const kind = !live.miner.path && info?.xmrigPinnedAvailable ? "xmrig" : wantsGpu && !live.srbminer.path ? "srbminer" : null;
    if (!kind || tried.current.has(kind)) return;
    tried.current.add(kind);
    api
      .installMiner(kind)
      .then(refreshStatus)
      .catch((e) => toast("error", translateError(lang, errorText(e))));
  }, [step, hw, installing, live.miner.path, live.srbminer.path]);
  const pcReady = !!live.miner.path && !installing && (!wantsGpu || !!live.srbminer.path || tried.current.has("srbminer"));

  // A Kryptex ID only works on Kryptex: pick that pool automatically.
  const onWallet = (wallet: string) => {
    const kryptex = catalog.find((p) => p.id === "kryptex");
    const isKrx = /^krx[a-z0-9]+$/i.test(wallet.trim());
    const pool =
      isKrx && kryptex && draft.pool.id !== "kryptex"
        ? { ...draft.pool, id: kryptex.id, host: kryptex.host, port: kryptex.port, tls: kryptex.tls }
        : draft.pool;
    setDraft({ ...draft, wallet, pool });
  };

  const visible: Step[] = demo ? ["welcome", "ready"] : [...STEPS];
  const idx = visible.indexOf(step);
  const go = (d: 1 | -1) => setStep(visible[Math.max(0, Math.min(visible.length - 1, idx + d))]);

  const canNext: Record<Step, boolean> = {
    welcome: true,
    account: walletOk,
    pc: pcReady,
    ready: false,
  };

  const finish = async (start: boolean) => {
    setSaving(true);
    try {
      await saveConfig({ ...draft, onboardingCompleted: true });
      if (start) {
        await api.startMining();
        await refreshStatus();
      }
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    } finally {
      setSaving(false);
    }
  };


  const steps = visible.map((id) => ({ id, label: t(`onb.nav.${id}`) }));
  const mode = draft.wallet ? payoutMode(draft.wallet) : "kryptex";

  return (
    <div className="flex min-h-full">
      {/* Brand side: where you are and what Erebor does for you. */}
      <aside className="relative hidden w-[340px] shrink-0 flex-col overflow-hidden border-r border-line bg-sidebar p-8 lg:flex">
        <div aria-hidden className="pointer-events-none absolute -top-32 -left-24 size-80 rounded-full bg-accent/20 blur-3xl" />
        <div className="relative flex items-center gap-2.5">
          <Logo className="h-9 w-auto" />
          <span className="text-lg font-semibold tracking-wide">Erebor</span>
        </div>
        <p className="relative mt-6 font-display text-2xl leading-snug tracking-tight">{t("onb.tagline")}</p>
        <ol className="relative mt-10 space-y-1" aria-label={t("onb.step", { n: idx + 1, t: visible.length })}>
          {steps.map((s, i) => (
            <li key={s.id} aria-current={i === idx ? "step" : undefined} className={cx("flex items-center gap-3 rounded-xl px-3 py-2.5 text-sm transition-colors", i === idx ? "bg-surface-2 font-medium text-fg" : i < idx ? "text-muted" : "text-faint")}>
              <span className={cx("flex size-6 shrink-0 items-center justify-center rounded-full border text-xs", i < idx ? "border-accent bg-accent-solid text-accent-fg" : i === idx ? "border-accent text-accent" : "border-line")}>
                {i < idx ? <Check className="size-3.5" /> : i + 1}
              </span>
              {s.label}
            </li>
          ))}
        </ol>
        <p className="relative mt-auto flex gap-2 text-xs leading-relaxed text-faint">
          <Zap className="mt-0.5 size-3.5 shrink-0 text-warn" />
          {t("onb.welcome.energy")}
        </p>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center justify-between gap-3 px-6 pt-6 md:px-10">
          <div className="flex items-center gap-2.5 lg:invisible">
            <Logo className="h-7 w-auto" />
            <span className="font-semibold tracking-wide">Erebor</span>
          </div>
          <div className="flex items-center gap-3">
            <span className="num text-xs text-faint lg:hidden">{t("onb.step", { n: idx + 1, t: visible.length })}</span>
            <Segmented
              label={t("settings.language")}
              value={lang}
              onChange={(language) => setDraft({ ...draft, language })}
              options={[
                { value: "it", label: "IT" },
                { value: "en", label: "EN" },
              ]}
            />
          </div>
        </div>

        <div className="flex flex-1 items-center justify-center px-6 py-10 md:px-10">
          <div key={step} className="page-in w-full max-w-lg">
            {step === "welcome" && (
              <Section title={t("onb.welcome.title")} sub={t("onb.welcome.body")}>
                <ul className="space-y-3">
                  {(
                    [
                      [Sparkles, "onb.welcome.f1"],
                      [Bitcoin, "onb.welcome.f2"],
                      [Power, "onb.welcome.f3"],
                    ] as const
                  ).map(([Icon, key]) => (
                    <li key={key} className="flex items-start gap-3.5 rounded-2xl border border-line bg-surface p-4">
                      <span className="flex size-9 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent">
                        <Icon className="size-[18px]" />
                      </span>
                      <span className="min-w-0">
                        <span className="block text-sm font-medium">{t(`${key}.title`)}</span>
                        <span className="mt-0.5 block text-[13px] leading-relaxed text-muted">{t(`${key}.body`)}</span>
                      </span>
                    </li>
                  ))}
                </ul>
                <p className="flex gap-2 text-xs leading-relaxed text-faint lg:hidden">
                  <Zap className="mt-0.5 size-3.5 shrink-0 text-warn" />
                  {t("onb.welcome.energy")}
                </p>
              </Section>
            )}

            {step === "account" && (
              <Section title={t("onb.account.title")} sub={t("onb.account.desc")}>
                <PayoutChoice value={draft.wallet} onChange={onWallet} onValid={setWalletOk} />
              </Section>
            )}

            {step === "pc" && (
              <Section title={t("onb.pc.title")} sub={pcReady ? t("onb.pc.ready") : t("onb.pc.desc")}>
                <ol className="space-y-2">
                  <Task done={!!hw} label={t("onb.pc.detect")} detail={hw ? `${hw.cpuBrand || t("hw.cpu")}${gpu ? ` · ${gpu.name}` : ""}` : undefined} />
                  <Task done={pcReady} active={!!hw && !pcReady} label={t("onb.pc.install")} detail={t("onb.pc.install.detail")} />
                </ol>
                {!pcReady && <JobProgress />}
                {hw && (
                  <p className="flex items-start gap-2.5 rounded-xl border border-accent/30 bg-accent-soft px-4 py-3 text-[13px] leading-relaxed">
                    {gpu ? <Gpu className="mt-0.5 size-4 shrink-0 text-accent" /> : <Cpu className="mt-0.5 size-4 shrink-0 text-accent" />}
                    <span>
                      {gpu
                        ? t(wantsGpu ? "onb.pc.advice.gpu" : mode === "monero" ? "onb.pc.advice.gpuMonero" : "onb.pc.advice.gpuOff", { g: gpu.name })
                        : t("onb.pc.advice.cpu")}
                    </span>
                  </p>
                )}
                <details className="text-sm">
                  <summary className="cursor-pointer text-faint hover:text-muted">{t("onb.pc.manual")}</summary>
                  <div className="space-y-4 pt-3">
                    <MinerSetup kind="xmrig" />
                    {gpu && <MinerSetup kind="srbminer" />}
                  </div>
                </details>
              </Section>
            )}

            {step === "ready" && (
              <Section title={t("onb.ready.title")} sub={t("onb.ready.body")}>
                {!demo && (
                  <div className="rounded-2xl border border-line bg-surface px-4">
                    <Row label={t("onb.ready.payout")}>
                      <span className="font-mono text-xs">{shortAddress(draft.wallet)}</span>
                    </Row>
                    <Row label={t("onb.ready.mining")}>{t("coin.auto")}</Row>
                    <Row label={t("home.mode")}>{t(`hw.mode.${draft.performance.preset}`)}</Row>
                  </div>
                )}
                <div className="flex flex-col gap-2 pt-2 sm:flex-row">
                  <Button variant="primary" className="h-12 justify-center px-6 text-[15px] sm:flex-1" onClick={() => finish(true)} loading={saving} disabled={!demo && (!live.miner.path || !walletOk)}>
                    <Power className="size-4" />
                    {t("onb.ready.start")}
                  </Button>
                  <Button variant="ghost" className="h-12 justify-center" onClick={() => finish(false)} disabled={saving}>
                    {t("onb.ready.later")}
                  </Button>
                </div>
              </Section>
            )}

            <div className="mt-8 flex items-center justify-between gap-2">
              <Button
                variant="ghost"
                onClick={() => (demo && step === "ready" ? (setDraft({ ...draft, demoMode: false }), setStep("welcome")) : go(-1))}
                className={cx(idx === 0 && "invisible")}
              >
                <ArrowLeft className="size-4" />
                {t("onb.back")}
              </Button>
              <div className="flex gap-2">
                {step === "welcome" && (
                  <Button variant="ghost" onClick={() => (setDraft({ ...draft, demoMode: true }), setStep("ready"))}>
                    {t("onb.welcome.demo")}
                  </Button>
                )}
                {step !== "ready" && (
                  <Button variant="primary" className="px-5" onClick={() => go(1)} disabled={!canNext[step]}>
                    {t("onb.next")}
                    <ArrowRight className="size-4" />
                  </Button>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function Section({ title, sub, children }: { title: string; sub?: string; children: ReactNode }) {
  return (
    <div className="space-y-5">
      <div>
        <h1 className="font-display text-3xl tracking-tight">{title}</h1>
        {sub && <p className="mt-2 text-[15px] leading-relaxed text-muted">{sub}</p>}
      </div>
      {children}
    </div>
  );
}

/** One line of the "preparing your PC" checklist. */
function Task({ done, active, label, detail }: { done: boolean; active?: boolean; label: string; detail?: string }) {
  return (
    <li className="flex items-center gap-3 rounded-2xl border border-line bg-surface p-4">
      <span className={cx("flex size-7 shrink-0 items-center justify-center rounded-full", done ? "bg-go/15 text-go" : "bg-surface-2 text-faint")}>
        {done ? <Check className="size-4" /> : active ? <Loader2 className="size-4 animate-spin text-accent" /> : <Loader2 className="size-4 animate-spin" />}
      </span>
      <span className="min-w-0">
        <span className="block text-sm font-medium">{label}</span>
        {detail && <span className="block truncate text-xs text-faint">{detail}</span>}
      </span>
    </li>
  );
}
