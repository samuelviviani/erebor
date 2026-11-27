import {
  ArrowLeft,
  ArrowRight,
  Boxes,
  Check,
  Coins,
  Compass,
  Cpu,
  Gauge,
  Gpu,
  Hand,
  Loader2,
  Power,
  Receipt,
  ShieldCheck,
  Thermometer,
  Wallet,
  Zap,
} from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Logo, moveWindow } from "../components/Layout";
import { PayoutChoice, payoutMode } from "../components/PayoutChoice";
import { DefenderConsent, JobProgress, MinerSetup } from "../components/shared";
import {
  Button,
  cx,
  Field,
  IconTile,
  inputClass,
  Segmented,
} from "../components/ui";
import {
  api,
  errorText,
  type AppConfig,
  type HardwareInfo,
  type PoolInfo,
} from "../lib/api";
import { shortAddress } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

// Three questions at most: where earnings go, prepare the PC, start.
const STEPS = ["welcome", "account", "pc", "ready"] as const;
type Step = (typeof STEPS)[number];

export default function Onboarding() {
  const t = useT();
  const { config, saveConfig, toast, info, status, refreshStatus, setPage, setupLocal, setSetupLocal } =
    useApp();
  // First choice: mine here, or manage devices (no mining setup needed for that).
  const [use, setUse] = useState<"mining" | "devices">(config!.defaultView === "devices" ? "devices" : "mining");
  const [draft, setDraft] = useState<AppConfig>(config!);
  const [step, setStep] = useState<Step>("welcome");
  const [hw, setHw] = useState<HardwareInfo | null>(null);
  const [catalog, setCatalog] = useState<PoolInfo[]>([]);
  const [walletOk, setWalletOk] = useState(false);
  // Seasoned miners may skip the main payout and set per-coin wallets afterwards.
  const [ownLater, setOwnLater] = useState(false);
  const [saving, setSaving] = useState(false);
  const [skipDefender, setSkipDefender] = useState(false);
  const lang = draft.language;
  const demo = draft.demoMode;
  const live = config!; // backend copy: the installed miner lands there directly
  const job = status?.job;
  const installing = job?.kind === "install" && !job.done;

  useEffect(() => {
    api
      .poolCatalog()
      .then(setCatalog)
      .catch(() => {});
  }, []);

  // Preparing the PC: detect hardware and install the pinned miners without asking:
  // XMRig for the CPU, plus SRBMiner when there is a usable GPU and a Kryptex ID.
  const tried = useRef<Set<string>>(new Set());
  const [attempt, setAttempt] = useState(0); // "Retry" after a failed install
  // Windows: installing before the Defender choice would get the miner quarantined.
  const waitDefender =
    !!info?.platform.startsWith("windows") &&
    !live.defenderAllowed &&
    !skipDefender;
  const gpu = hw?.gpus.find((g) =>
    /nvidia|geforce|amd|radeon/i.test(`${g.vendor ?? ""} ${g.name}`),
  );
  const wantsGpu =
    !!gpu &&
    /^(krx|bc1|[13])/i.test(draft.wallet.trim()) &&
    !!info?.srbminerPinnedAvailable;
  useEffect(() => {
    if (step !== "pc") return;
    if (!hw) {
      api
        .detectHardware()
        .then(setHw)
        .catch(() => {});
      return;
    }
    if (installing || waitDefender) return;
    const kind =
      !live.miner.path && info?.xmrigPinnedAvailable
        ? "xmrig"
        : wantsGpu && !live.srbminer.path
          ? "srbminer"
          : wantsGpu && !live.lolminer.path && info?.lolminerPinnedAvailable
            ? "lolminer"
            : null;
    if (!kind || tried.current.has(kind)) return;
    tried.current.add(kind);
    api
      .installMiner(kind)
      .then(refreshStatus)
      .catch((e) => toast("error", translateError(lang, errorText(e))));
  }, [
    step,
    hw,
    installing,
    waitDefender,
    live.miner.path,
    live.srbminer.path,
    live.lolminer.path,
    attempt,
  ]);
  // GPU miners are a bonus: a failed download does not block the setup.
  const gpuDone = (k: "srbminer" | "lolminer") =>
    !!live[k].path || tried.current.has(k);
  const pcReady =
    !!live.miner.path &&
    !installing &&
    (!wantsGpu || (gpuDone("srbminer") && gpuDone("lolminer")));

  // A Kryptex ID only works on Kryptex: pick that pool automatically.
  const onWallet = (wallet: string) => {
    const kryptex = catalog.find((p) => p.id === "kryptex");
    const isKrx = /^krx[a-z0-9]+$/i.test(wallet.trim());
    const pool =
      isKrx && kryptex && draft.pool.id !== "kryptex"
        ? {
            ...draft.pool,
            id: kryptex.id,
            host: kryptex.host,
            port: kryptex.port,
            tls: kryptex.tls,
          }
        : draft.pool;
    setDraft({ ...draft, wallet, pool });
  };

  const visible: Step[] = demo ? ["welcome", "ready"] : [...STEPS];
  const idx = visible.indexOf(step);
  const go = (d: 1 | -1) =>
    setStep(visible[Math.max(0, Math.min(visible.length - 1, idx + d))]);

  const canNext: Record<Step, boolean> = {
    welcome: true,
    account: walletOk || ownLater,
    pc: pcReady,
    ready: false,
  };

  const openDevices = async () => {
    setSaving(true);
    try {
      await saveConfig({ ...draft, defaultView: "devices" });
      setSetupLocal(false);
      setPage("devices");
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    } finally {
      setSaving(false);
    }
  };

  const finish = async (start: boolean) => {
    setSaving(true);
    try {
      await saveConfig({ ...draft, onboardingCompleted: true, defaultView: draft.defaultView || "mining" });
      setSetupLocal(false);
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
  const mode = draft.wallet ? payoutMode(draft.wallet) : "bitcoin";

  return (
    <div className="flex min-h-full">
      {/* Brand side: where you are and what Erebor does for you. */}
      <aside onMouseDown={moveWindow} className="relative hidden w-[340px] shrink-0 flex-col overflow-hidden border-r border-line bg-sidebar p-8 lg:flex">
        <div className="relative flex items-center gap-2.5">
          <Logo className="h-9 w-auto" />
          <span className="text-lg font-semibold tracking-wide">Erebor</span>
        </div>
        <p className="relative mt-6 font-display text-2xl leading-snug tracking-tight">
          {t("onb.tagline")}
        </p>
        <ol
          className="relative mt-10 space-y-1"
          aria-label={t("onb.step", { n: idx + 1, t: visible.length })}
        >
          {steps.map((s, i) => (
            <li
              key={s.id}
              aria-current={i === idx ? "step" : undefined}
              className={cx(
                "flex items-center gap-3 rounded-xl px-3 py-2.5 text-sm transition-colors",
                i === idx
                  ? "bg-surface-2 font-medium text-fg"
                  : i < idx
                    ? "text-muted"
                    : "text-faint",
              )}
            >
              <span
                className={cx(
                  "flex size-6 shrink-0 items-center justify-center rounded-full border text-xs",
                  i < idx
                    ? "border-accent bg-accent-solid text-accent-fg"
                    : i === idx
                      ? "border-accent text-accent"
                      : "border-line",
                )}
              >
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
        <div onMouseDown={moveWindow} className="flex items-center justify-between gap-3 px-6 pt-6 md:px-10">
          <div className="flex items-center gap-2.5 lg:invisible">
            <Logo className="h-7 w-auto" />
            <span className="font-semibold tracking-wide">Erebor</span>
          </div>
          <div className="flex items-center gap-3">
            <span className="num text-xs text-faint lg:hidden">
              {t("onb.step", { n: idx + 1, t: visible.length })}
            </span>
          </div>
        </div>

        <div className="flex flex-1 items-center justify-center px-6 py-10 md:px-10">
          <div key={step} className="page-in w-full max-w-lg">
            {step === "welcome" && (
              <Section
                title={t("onb.welcome.title")}
                sub={t("onb.welcome.body")}
              >
                {/* How Erebor is used: only decides this setup and the main screen;
                    both stay available, switchable in Settings. */}
                {!setupLocal && (
                  <div role="radiogroup" aria-label={t("onb.use.label")} className="grid gap-3 sm:grid-cols-2">
                    {(
                      [
                        ["mining", Cpu, "onb.use.mining"],
                        ["devices", Boxes, "onb.use.devices"],
                      ] as const
                    ).map(([value, Icon, key]) => (
                      <button
                        key={value}
                        type="button"
                        role="radio"
                        aria-checked={use === value}
                        onClick={() => setUse(value)}
                        className={cx(
                          "press rounded-2xl border p-4 text-left transition-colors",
                          use === value ? "border-accent bg-accent-soft" : "border-line hover:bg-surface-2",
                        )}
                      >
                        <Icon className={cx("size-5", use === value ? "text-accent" : "text-faint")} aria-hidden />
                        <span className="mt-2 block text-base font-medium">{t(`${key}.title`)}</span>
                        <span className="mt-1 block text-sm leading-relaxed text-muted">{t(`${key}.body`)}</span>
                      </button>
                    ))}
                  </div>
                )}
                {use === "devices" && !setupLocal ? (
                  <p className="text-sm leading-relaxed text-muted">{t("onb.use.devices.next")}</p>
                ) : (
                <>
                {/* What Erebor takes off your hands: plain rows, a coloured glyph each,
                    the same visual language as Settings. */}
                <ul className="divide-y divide-line border-y border-line">
                  {(
                    [
                      [Compass, "text-[#0a84ff]", "onb.welcome.f1"],
                      [ShieldCheck, "text-[#30d158]", "onb.welcome.f2"],
                      [Receipt, "text-[#ff9f0a]", "onb.welcome.f3"],
                      [Power, "text-[#ff453a]", "onb.welcome.f4"],
                    ] as const
                  ).map(([Icon, tone, key]) => (
                    <li key={key} className="flex items-start gap-4 py-4">
                      <Icon
                        className={cx("mt-0.5 size-5 shrink-0", tone)}
                        strokeWidth={2}
                      />
                      <span className="min-w-0">
                        <span className="block text-base font-medium">
                          {t(`${key}.title`)}
                        </span>
                        <span className="mt-1 block text-sm leading-relaxed text-muted">
                          {t(`${key}.body`)}
                        </span>
                      </span>
                    </li>
                  ))}
                </ul>
                <p className="flex gap-2 text-xs leading-relaxed text-faint lg:hidden">
                  <Zap className="mt-0.5 size-3.5 shrink-0 text-warn" />
                  {t("onb.welcome.energy")}
                </p>
                </>
                )}
              </Section>
            )}

            {step === "account" && (
              <Section
                title={t("onb.account.title")}
                sub={t("onb.account.desc")}
              >
                <PayoutChoice
                  value={draft.wallet}
                  onChange={onWallet}
                  onValid={setWalletOk}
                />
                {!walletOk && (
                  <button
                    type="button"
                    onClick={() => {
                      onWallet("");
                      setOwnLater(true);
                      go(1);
                    }}
                    className="text-left text-xs leading-relaxed text-faint hover:text-fg hover:underline"
                  >
                    {t("onb.ownLater")}
                  </button>
                )}
                {/* Set from the start: how this PC shows up on Kryptex. */}
                <Field
                  label={t("settings.worker")}
                  hint={t("settings.worker.desc")}
                >
                  <input
                    className={inputClass}
                    value={draft.pool.worker}
                    maxLength={32}
                    spellCheck={false}
                    placeholder={
                      (info?.hostname ?? "")
                        .replace(/[^A-Za-z0-9]/g, "")
                        .slice(0, 32) || "PC"
                    }
                    onChange={(e) =>
                      setDraft({
                        ...draft,
                        pool: {
                          ...draft.pool,
                          worker: e.target.value.replace(/[^A-Za-z0-9]/g, ""),
                        },
                      })
                    }
                  />
                </Field>
              </Section>
            )}

            {step === "pc" && (
              <Section
                title={t("onb.pc.title")}
                sub={pcReady ? t("onb.pc.ready") : t("onb.pc.desc")}
              >
                <DefenderConsent onSkip={() => setSkipDefender(true)} />
                <ol className="space-y-2">
                  <Task
                    done={!!hw}
                    label={t("onb.pc.detect")}
                    detail={
                      hw
                        ? `${hw.cpuBrand || t("hw.cpu")}${gpu ? ` · ${gpu.name}` : ""}`
                        : undefined
                    }
                  />
                  <Task
                    done={pcReady}
                    active={!!hw && !pcReady}
                    label={t("onb.pc.install")}
                    detail={t("onb.pc.install.detail")}
                  />
                </ol>
                {!pcReady && <JobProgress />}
                {!pcReady && job?.done && job.error && (
                  <Button
                    variant="ghost"
                    onClick={() => {
                      tried.current.clear();
                      setAttempt((n) => n + 1);
                    }}
                  >
                    {t("computer.retry")}
                  </Button>
                )}
                {hw && (
                  <p className="flex items-start gap-2.5 rounded-xl border border-accent/30 bg-accent-soft px-4 py-3 text-sm leading-relaxed">
                    {gpu ? (
                      <Gpu className="mt-0.5 size-4 shrink-0 text-accent" />
                    ) : (
                      <Cpu className="mt-0.5 size-4 shrink-0 text-accent" />
                    )}
                    <span>
                      {gpu
                        ? t(
                            wantsGpu
                              ? "onb.pc.advice.gpu"
                              : mode === "monero"
                                ? "onb.pc.advice.gpuMonero"
                                : "onb.pc.advice.gpuOff",
                            { g: gpu.name },
                          )
                        : t("onb.pc.advice.cpu")}
                    </span>
                  </p>
                )}
                <details className="text-sm">
                  <summary className="cursor-pointer text-faint hover:text-muted">
                    {t("onb.pc.manual")}
                  </summary>
                  <div className="space-y-4 pt-3">
                    <MinerSetup kind="xmrig" />
                    {gpu && <MinerSetup kind="srbminer" />}
                    {gpu && <MinerSetup kind="lolminer" />}
                  </div>
                </details>
              </Section>
            )}

            {step === "ready" && (
              <Section title={t("onb.ready.title")} sub={t("onb.ready.body")}>
                {!demo && (
                  <div className="divide-y divide-line rounded-2xl border border-line bg-surface shadow-[var(--highlight)]">
                    <Summary icon={<Wallet />} tone="go" label={t("onb.ready.payout")}>
                      <span className="font-mono text-xs">{shortAddress(draft.wallet)}</span>
                      <button type="button" className="ml-3 text-xs text-accent hover:underline" onClick={() => setStep("account")}>
                        {t("onb.ready.change")}
                      </button>
                    </Summary>
                    <Summary icon={<Coins />} tone="accent" label={t("onb.ready.mining")} hint={t("onb.ready.mining.hint")}>
                      {t("coin.auto")}
                    </Summary>
                    <Summary icon={<Gauge />} tone="perf" label={t("home.mode")} hint={t(`hw.mode.${draft.performance.preset}.desc`)}>
                      <Segmented
                        size="sm"
                        label={t("home.mode")}
                        value={draft.performance.preset === "custom" ? "balanced" : draft.performance.preset}
                        onChange={(preset) => setDraft({ ...draft, performance: { ...draft.performance, preset } })}
                        options={(["eco", "balanced", "performance"] as const).map((m) => ({ value: m, label: t(`hw.mode.${m}`) }))}
                      />
                    </Summary>
                  </div>
                )}
                <ul className="space-y-2 text-sm text-muted">
                  {draft.safety.stopOnOverheat && (
                    <Assurance icon={<Thermometer />}>{t("onb.ready.safe.heat", { c: Math.round(draft.safety.maxTempC) })}</Assurance>
                  )}
                  <Assurance icon={<Hand />}>{t("onb.ready.safe.stop")}</Assurance>
                  {!!info?.devFeeWalletConfigured && !demo && (
                    <Assurance icon={<Receipt />}>{t("onb.ready.safe.fee", { p: info.devFeePercent })}</Assurance>
                  )}
                </ul>
              </Section>
            )}

            <div className="mt-8 flex items-center justify-between gap-2">
              {setupLocal && idx === 0 ? (
                // Came from device management: back there without setting up mining.
                <Button variant="ghost" onClick={() => setSetupLocal(false)}>
                  <ArrowLeft className="size-4" />
                  {t("onb.use.backToDevices")}
                </Button>
              ) : (
              <Button
                variant="ghost"
                onClick={() =>
                  demo && step === "ready"
                    ? (setDraft({ ...draft, demoMode: false }),
                      setStep("welcome"))
                    : go(-1)
                }
                className={cx(idx === 0 && "invisible")}
              >
                <ArrowLeft className="size-4" />
                {t("onb.back")}
              </Button>
              )}
              <div className="flex gap-2">
                {step === "welcome" && use === "devices" && !setupLocal ? (
                  <Button variant="primary" className="px-5" loading={saving} onClick={openDevices}>
                    {t("onb.use.openDevices")}
                    <ArrowRight className="size-4" />
                  </Button>
                ) : (
                <>
                {step === "welcome" && (
                  <Button
                    variant="ghost"
                    onClick={() => (
                      setDraft({ ...draft, demoMode: true }),
                      setStep("ready")
                    )}
                  >
                    {t("onb.welcome.demo")}
                  </Button>
                )}
                {step === "ready" ? (
                  <>
                    <Button variant="ghost" onClick={() => finish(false)} disabled={saving}>
                      {t("onb.ready.later")}
                    </Button>
                    <Button
                      variant="primary"
                      className="px-5"
                      onClick={() => finish(true)}
                      loading={saving}
                      disabled={!demo && (!live.miner.path || !walletOk)}
                    >
                      <Power className="size-4" />
                      {t("onb.ready.start")}
                    </Button>
                  </>
                ) : (
                  <Button
                    variant="primary"
                    className="px-5"
                    onClick={() => go(1)}
                    disabled={!canNext[step]}
                  >
                    {t("onb.next")}
                    <ArrowRight className="size-4" />
                  </Button>
                )}
                </>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/** A line of the final summary: what was chosen, and where it can be changed. */
function Summary({ icon, tone, label, hint, children }: { icon: ReactNode; tone: "go" | "accent" | "perf"; label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-3 px-4 py-3.5">
      <IconTile tone={tone}>{icon}</IconTile>
      <div className="min-w-0 flex-1">
        <div className="text-sm text-fg">{label}</div>
        {hint && <div className="text-xs text-faint">{hint}</div>}
      </div>
      <div className="flex items-center text-sm text-fg">{children}</div>
    </div>
  );
}

function Assurance({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <li className="flex items-start gap-2.5">
      <span aria-hidden className="mt-0.5 text-go [&>svg]:size-4">
        {icon}
      </span>
      {children}
    </li>
  );
}

function Section({
  title,
  sub,
  children,
}: {
  title: string;
  sub?: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-5">
      <div>
        <h1 className="font-display text-3xl tracking-tight">{title}</h1>
        {sub && (
          <p className="mt-2 text-base leading-relaxed text-muted">{sub}</p>
        )}
      </div>
      {children}
    </div>
  );
}

/** One line of the "preparing your PC" checklist. */
function Task({
  done,
  active,
  label,
  detail,
}: {
  done: boolean;
  active?: boolean;
  label: string;
  detail?: string;
}) {
  return (
    <li className="flex items-center gap-3 rounded-2xl border border-line bg-surface p-4">
      <span
        className={cx(
          "flex size-7 shrink-0 items-center justify-center rounded-full",
          done ? "bg-go/15 text-go" : "bg-surface-2 text-faint",
        )}
      >
        {done ? (
          <Check className="size-4" />
        ) : active ? (
          <Loader2 className="size-4 animate-spin text-accent" />
        ) : (
          <Loader2 className="size-4 animate-spin" />
        )}
      </span>
      <span className="min-w-0">
        <span className="block text-sm font-medium">{label}</span>
        {detail && (
          <span className="block truncate text-xs text-faint">{detail}</span>
        )}
      </span>
    </li>
  );
}
