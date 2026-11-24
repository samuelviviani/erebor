import {
  Banknote,
  CalendarClock,
  ChevronLeft,
  ChevronRight,
  Download,
  FlaskConical,
  Info,
  Languages,
  Monitor,
  Moon,
  LogIn,
  Play,
  RefreshCw,
  Plug,
  RotateCcw,
  ShieldAlert,
  SlidersHorizontal,
  Sun,
  SunMoon,
  Thermometer,
  Trash2,
  TrendingDown,
  Zap,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { PageTitle } from "../components/Layout";
import {
  Button,
  ConfirmDialog,
  cx,
  Explain,
  Flag,
  Segmented,
  type HelpTerm,
} from "../components/ui";
import { api, errorText, inApp, local, type AppConfig } from "../lib/api";
import { minutesToTime, timeToMinutes } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import Advanced from "./Mining";

const DAYS = {
  it: ["L", "M", "M", "G", "V", "S", "D"],
  en: ["M", "T", "W", "T", "F", "S", "S"],
};

/**
 * Settings as one page of grouped rows: an icon, what it is (with one plain line of help)
 * and a control sized to its content. Technical options live behind "Advanced options".
 */
export default function Settings() {
  const t = useT();
  const { config, saveConfig, toast } = useApp();
  const [advanced, setAdvanced] = useState(false);
  const c = config!;
  const cur = c.energy.currency === "USD" ? "$" : "€";

  const save = async (next: AppConfig) => {
    try {
      await saveConfig(next);
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    }
  };

  if (advanced)
    return (
      <div className="page-in mx-auto max-w-2xl">
        <button
          type="button"
          onClick={() => setAdvanced(false)}
          className="-ml-2 mb-4 flex items-center gap-1 rounded-lg px-2 py-1 text-sm text-accent hover:bg-accent-soft"
        >
          <ChevronLeft className="size-4" />
          {t("settings.title")}
        </button>
        <PageTitle sub={t("settings.advanced.note")}>
          {t("settings.advanced")}
        </PageTitle>
        <div className="space-y-4">
          <Group>
            <Item
              icon={<FlaskConical />}
              tone="violet"
              label={t("settings.demo")}
              hint={t("settings.demo.desc")}
            >
              <Sw
                label={t("settings.demo")}
                checked={c.demoMode}
                onChange={(demoMode) => save({ ...c, demoMode })}
              />
            </Item>
            <Item
              icon={<RefreshCw />}
              tone="blue"
              label={t("settings.autoUpdateMiners")}
              hint={t("settings.autoUpdateMiners.desc")}
            >
              <Sw
                label={t("settings.autoUpdateMiners")}
                checked={c.autoUpdateMiners}
                onChange={(autoUpdateMiners) =>
                  save({ ...c, autoUpdateMiners })
                }
              />
            </Item>
          </Group>
          <Advanced />
        </div>
      </div>
    );

  const temp = (d: number) =>
    save({
      ...c,
      safety: {
        ...c.safety,
        maxTempC: Math.min(95, Math.max(60, c.safety.maxTempC + d)),
      },
    });
  return (
    <div className="mx-auto max-w-2xl">
      <PageTitle>{t("settings.title")}</PageTitle>
      <div className="space-y-8">
        <Group title={t("settings.group.pc")}>
          <WorkerName c={c} save={save} />
        </Group>

        <Group title={t("settings.group.appearance")}>
          <Item icon={<Languages />} tone="blue" label={t("settings.language")}>
            <Segmented
              label={t("settings.language")}
              value={c.language}
              onChange={(language) => save({ ...c, language })}
              options={[
                { value: "en", label: "English", icon: <Flag lang="en" /> },
                { value: "it", label: "Italiano", icon: <Flag lang="it" /> },
              ]}
            />
          </Item>
          <Item icon={<SunMoon />} tone="indigo" label={t("settings.theme")}>
            <Segmented
              label={t("settings.theme")}
              value={c.theme}
              onChange={(theme) => save({ ...c, theme })}
              options={[
                {
                  value: "dark",
                  label: t("settings.theme.dark"),
                  icon: <Moon />,
                },
                {
                  value: "light",
                  label: t("settings.theme.light"),
                  icon: <Sun />,
                },
                {
                  value: "system",
                  label: t("settings.theme.system"),
                  icon: <Monitor />,
                },
              ]}
            />
          </Item>
        </Group>

        <Group title={t("settings.group.startup")}>
          <StartupRows c={c} save={save} />
          <Item
            icon={<CalendarClock />}
            tone="orange"
            label={t("settings.schedule")}
            hint={t("settings.schedule.desc")}
          >
            <Sw
              label={t("settings.schedule")}
              checked={c.schedule.enabled}
              onChange={(enabled) =>
                save({ ...c, schedule: { ...c.schedule, enabled } })
              }
            />
          </Item>
          {c.schedule.enabled && (
            <div className="flex flex-wrap items-center gap-3 py-3 pr-4 pl-16">
              <span className="inline-flex items-center gap-2 text-sm text-muted">
                {t("settings.schedule.from")}
                <input
                  type="time"
                  aria-label={t("settings.schedule.from")}
                  className={compact}
                  defaultValue={minutesToTime(c.schedule.startMinute)}
                  onBlur={(e) =>
                    save({
                      ...c,
                      schedule: {
                        ...c.schedule,
                        startMinute: timeToMinutes(e.target.value),
                      },
                    })
                  }
                />
              </span>
              <span className="inline-flex items-center gap-2 text-sm text-muted">
                {t("settings.schedule.to")}
                <input
                  type="time"
                  aria-label={t("settings.schedule.to")}
                  className={compact}
                  defaultValue={minutesToTime(c.schedule.endMinute)}
                  onBlur={(e) =>
                    save({
                      ...c,
                      schedule: {
                        ...c.schedule,
                        endMinute: timeToMinutes(e.target.value),
                      },
                    })
                  }
                />
              </span>
              <div className="flex gap-1">
                {DAYS[c.language].map((d, i) => (
                  <button
                    key={i}
                    type="button"
                    aria-pressed={c.schedule.days[i]}
                    onClick={() => {
                      const days = [...c.schedule.days];
                      days[i] = !days[i];
                      save({ ...c, schedule: { ...c.schedule, days } });
                    }}
                    className={cx(
                      "size-8 rounded-full text-xs font-medium",
                      c.schedule.days[i]
                        ? "bg-accent-solid text-accent-fg"
                        : "bg-surface-2 text-faint hover:text-fg",
                    )}
                  >
                    {d}
                  </button>
                ))}
              </div>
            </div>
          )}
        </Group>

        <Group title={t("settings.group.safety")} help="temp">
          <Item
            icon={<Thermometer />}
            tone="red"
            label={t("hw.maxTemp")}
            hint={t("settings.maxTemp.hint")}
          >
            <div className="flex items-center gap-1">
              <StepButton
                label="−"
                onClick={() => temp(-5)}
                disabled={c.safety.maxTempC <= 60}
              />
              <span className="num w-14 text-center text-sm font-medium">
                {c.safety.maxTempC} °C
              </span>
              <StepButton
                label="+"
                onClick={() => temp(5)}
                disabled={c.safety.maxTempC >= 95}
              />
            </div>
          </Item>
          <Item
            icon={<ShieldAlert />}
            tone="orange"
            label={t("hw.stopOnOverheat")}
          >
            <Sw
              label={t("hw.stopOnOverheat")}
              checked={c.safety.stopOnOverheat}
              onChange={(stopOnOverheat) =>
                save({ ...c, safety: { ...c.safety, stopOnOverheat } })
              }
            />
          </Item>
          <Item
            icon={<RotateCcw />}
            tone="teal"
            label={t("settings.autoRestart")}
            hint={t("settings.autoRestart.desc")}
          >
            <Sw
              label={t("settings.autoRestart")}
              checked={c.safety.autoRestart}
              onChange={(autoRestart) =>
                save({ ...c, safety: { ...c.safety, autoRestart } })
              }
            />
          </Item>
        </Group>

        <Group title={t("settings.group.power")} help="power">
          <Item
            icon={<Zap />}
            tone="yellow"
            label={t("settings.price.short")}
            hint={t("settings.price.hint")}
          >
            <NumberField
              label={t("settings.price")}
              prefix={cur}
              suffix="/kWh"
              placeholder="0,25"
              value={c.energy.pricePerKwh}
              onCommit={(pricePerKwh) =>
                save({ ...c, energy: { ...c.energy, pricePerKwh } })
              }
            />
          </Item>
          <Item icon={<Banknote />} tone="green" label={t("settings.currency")}>
            <Segmented
              label={t("settings.currency")}
              value={c.energy.currency}
              onChange={(currency) =>
                save({ ...c, energy: { ...c.energy, currency } })
              }
              options={[
                { value: "EUR", label: "€ EUR" },
                { value: "USD", label: "$ USD" },
              ]}
            />
          </Item>
          <Item
            icon={<Plug />}
            tone="teal"
            label={t("settings.watts.short")}
            hint={t("settings.watts.desc")}
          >
            <NumberField
              label={t("settings.watts")}
              suffix="W"
              placeholder="65"
              value={c.energy.estimatedWatts}
              onCommit={(estimatedWatts) =>
                save({ ...c, energy: { ...c.energy, estimatedWatts } })
              }
            />
          </Item>
          <Item
            icon={<TrendingDown />}
            tone="pink"
            label={t("settings.stopUnprofitable.short")}
            hint={t("settings.stopUnprofitable.hint")}
          >
            <Sw
              label={t("settings.stopUnprofitable")}
              checked={c.energy.stopIfUnprofitable}
              onChange={(stopIfUnprofitable) =>
                save({ ...c, energy: { ...c.energy, stopIfUnprofitable } })
              }
            />
          </Item>
          {c.energy.stopIfUnprofitable && (
            <Item icon={<span />} tone="none" label={t("settings.minProfit")}>
              <NumberField
                label={t("settings.minProfit")}
                prefix={cur}
                suffix={t("settings.perDayUnit")}
                value={c.energy.minProfitPerDay}
                onCommit={(v) =>
                  save({
                    ...c,
                    energy: { ...c.energy, minProfitPerDay: v ?? 0 },
                  })
                }
              />
            </Item>
          )}
        </Group>

        <DataGroup c={c} />

        <Group>
          <button
            type="button"
            onClick={() => setAdvanced(true)}
            className="flex w-full items-center gap-3.5 px-4 py-3 text-left hover:bg-surface-2/60"
          >
            <RowIcon tone="gray">
              <SlidersHorizontal />
            </RowIcon>
            <span className="min-w-0 flex-1">
              <span className="block text-sm">{t("settings.advanced")}</span>
              <span className="mt-0.5 block text-xs text-faint">
                {t("settings.advanced.hint")}
              </span>
            </span>
            <ChevronRight className="size-4 shrink-0 text-faint" />
          </button>
        </Group>
      </div>
    </div>
  );
}

type Props = { c: AppConfig; save: (next: AppConfig) => Promise<void> };

/** Name of this PC on the pool (Kryptex shows it per worker): tells the account owner which
 *  PCs mine, apart from Erebor's 2% ("EREBOR"). Empty = the computer's name. */
function WorkerName({ c, save }: Props) {
  const t = useT();
  const { info } = useApp();
  const fallback =
    (info?.hostname ?? "").replace(/[^A-Za-z0-9]/g, "").slice(0, 32) || "PC";
  return (
    <Item
      icon={<Monitor />}
      tone="gray"
      label={t("settings.worker")}
      hint={t("settings.worker.desc")}
    >
      <input
        key={c.pool.worker}
        aria-label={t("settings.worker")}
        className={cx(compact, "w-40")}
        maxLength={32}
        spellCheck={false}
        defaultValue={c.pool.worker}
        placeholder={fallback}
        onChange={(e) =>
          (e.target.value = e.target.value.replace(/[^A-Za-z0-9]/g, ""))
        }
        onBlur={(e) => {
          if (e.target.value !== c.pool.worker)
            save({ ...c, pool: { ...c.pool, worker: e.target.value } });
        }}
      />
    </Item>
  );
}

/** Small inputs for short values (time, price, watts): never a full-width box. */
const compact =
  "h-9 rounded-lg border border-line bg-bg px-2.5 text-sm text-fg focus:border-accent focus:outline-none";

/** A titled block of rows, iOS/macOS style. */
function Group({
  title,
  help,
  children,
}: {
  title?: string;
  help?: HelpTerm;
  children: ReactNode;
}) {
  return (
    <section>
      {title && (
        <h2 className="mb-2 flex items-center gap-1.5 px-4 text-xs font-medium tracking-wide text-faint uppercase">
          {title}
          {help && <Explain term={help} align="left" />}
        </h2>
      )}
      <div className="divide-y divide-line overflow-hidden rounded-2xl border border-line bg-surface">
        {children}
      </div>
    </section>
  );
}

/** One colour per meaning on the glyph itself, no tile around it; same in both themes. */
const TONES = {
  blue: "text-[#0a84ff]",
  indigo: "text-[#5e5ce6]",
  gray: "text-[#8e8e93]",
  green: "text-[#30d158]",
  orange: "text-[#ff9f0a]",
  red: "text-[#ff453a]",
  teal: "text-[#40c8e0]",
  yellow: "text-[#e6b800]",
  pink: "text-[#ff375f]",
  violet: "text-[#bf5af2]",
  none: "text-faint",
} as const;
type Tone = keyof typeof TONES;

function RowIcon({ tone, children }: { tone: Tone; children: ReactNode }) {
  return (
    <span
      className={cx(
        "flex size-7 shrink-0 items-center justify-center [&>svg]:size-5 [&>svg]:stroke-2",
        TONES[tone],
      )}
    >
      {children}
    </span>
  );
}

/** One row: icon, what it is (and one line of help), the control on the right. */
function Item({
  icon,
  tone,
  label,
  hint,
  children,
}: {
  icon: ReactNode;
  tone: Tone;
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center gap-3.5 px-4 py-3">
      <RowIcon tone={tone}>{icon}</RowIcon>
      <span className="min-w-0 flex-1">
        <span className="block text-sm">{label}</span>
        {hint && (
          <span className="mt-0.5 block text-xs leading-relaxed text-faint">
            {hint}
          </span>
        )}
      </span>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

/** Bare switch: the row already shows the label. */
function Sw({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cx(
        "relative block h-6 w-10 rounded-full transition-colors",
        checked ? "bg-accent" : "bg-line",
      )}
    >
      <span
        className={cx(
          "absolute top-0.5 left-0.5 size-5 rounded-full bg-white shadow transition-transform",
          checked && "translate-x-4",
        )}
      />
    </button>
  );
}

function StepButton({
  label,
  onClick,
  disabled,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label === "+" ? "+5 °C" : "−5 °C"}
      className="flex size-8 items-center justify-center rounded-full border border-line text-base text-muted hover:bg-surface-2 hover:text-fg disabled:opacity-40"
    >
      {label}
    </button>
  );
}

/** A short number with its unit around it, e.g. "€ [0,25] /kWh". Saved when leaving the field. */
function NumberField({
  label,
  value,
  onCommit,
  prefix,
  suffix,
  placeholder,
}: {
  label: string;
  value: number | null;
  onCommit: (v: number | null) => void;
  prefix?: string;
  suffix?: string;
  placeholder?: string;
}) {
  const locale = useApp((s) =>
    s.config?.language === "en" ? "en-US" : "it-IT",
  );
  const parse = (v: string) => {
    const n = Number(v.trim().replace(",", "."));
    return v.trim() === "" || !Number.isFinite(n) ? null : n;
  };
  return (
    <label className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-line bg-bg px-2.5 text-sm focus-within:border-accent">
      {prefix && <span className="text-faint">{prefix}</span>}
      <input
        className="num w-14 bg-transparent text-right text-fg placeholder:text-faint focus:outline-none"
        inputMode="decimal"
        aria-label={label}
        placeholder={placeholder}
        defaultValue={
          value == null
            ? ""
            : value.toLocaleString(locale, {
                useGrouping: false,
                maximumFractionDigits: 4,
              })
        }
        onBlur={(e) => {
          const v = parse(e.target.value);
          if (v !== value) onCommit(v);
        }}
      />
      {suffix && <span className="text-xs text-faint">{suffix}</span>}
    </label>
  );
}

function StartupRows({ c, save }: Props) {
  const t = useT();
  const toast = useApp((s) => s.toast);
  const [login, setLoginState] = useState<boolean | null>(null);
  useEffect(() => {
    if (inApp())
      local
        .loginItem()
        .then(setLoginState)
        .catch(() => {});
  }, []);
  const setLogin = async (on: boolean) => {
    try {
      await local.setLoginItem(on);
      setLoginState(await local.loginItem());
    } catch (e) {
      toast("error", errorText(e));
    }
  };
  return (
    <>
      {login != null && (
        <Item
          icon={<LogIn />}
          tone="gray"
          label={t("startup.login")}
          hint={t("startup.login.desc")}
        >
          <Sw label={t("startup.login")} checked={login} onChange={setLogin} />
        </Item>
      )}
      <Item
        icon={<Play />}
        tone="green"
        label={t("startup.mine")}
        hint={t("startup.mine.desc")}
      >
        <Sw
          label={t("startup.mine")}
          checked={c.startOnLaunch}
          onChange={(startOnLaunch) => save({ ...c, startOnLaunch })}
        />
      </Item>
    </>
  );
}

function DataGroup({ c }: { c: AppConfig }) {
  const t = useT();
  const { info, setConfig, toast } = useApp();
  const [confirm, setConfirm] = useState(false);
  const [dataDir, setDataDir] = useState("");
  useEffect(() => {
    api
      .dataDir()
      .then(setDataDir)
      .catch(() => {});
  }, []);
  const exportConfig = async () => {
    const json = await api.exportConfig();
    try {
      await navigator.clipboard.writeText(json);
      toast("info", t("settings.exported"));
    } catch {
      toast("error", json);
    }
  };
  const doDelete = async () => {
    setConfirm(false);
    try {
      setConfig(await api.deleteAllData());
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    }
  };
  return (
    <>
      <Group title={t("settings.group.data")}>
        <Item
          icon={<Download />}
          tone="blue"
          label={t("settings.export")}
          hint={t("settings.export.hint")}
        >
          <Button onClick={exportConfig}>{t("settings.export.short")}</Button>
        </Item>
        <Item
          icon={<Trash2 />}
          tone="red"
          label={t("settings.delete")}
          hint={dataDir || undefined}
        >
          <Button variant="danger" onClick={() => setConfirm(true)}>
            {t("settings.delete.short")}
          </Button>
        </Item>
        <Item icon={<Info />} tone="gray" label={t("settings.version")}>
          <span className="num text-sm text-muted">{info?.version ?? "—"}</span>
        </Item>
      </Group>
      <p className="-mt-6 px-4 text-xs leading-relaxed text-faint">
        {t("settings.privacy.desc")} {t("settings.licenses.desc")}
      </p>
      <ConfirmDialog
        open={confirm}
        danger
        text={t("settings.delete.confirm")}
        confirmLabel={t("confirm.ok")}
        cancelLabel={t("confirm.cancel")}
        onConfirm={doDelete}
        onCancel={() => setConfirm(false)}
      />
    </>
  );
}
