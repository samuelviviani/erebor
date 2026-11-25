import {
  Banknote,
  ChevronRight,
  Download,
  FileSpreadsheet,
  FlaskConical,
  HeartHandshake,
  Languages,
  LogIn,
  Monitor,
  Moon,
  Play,
  RefreshCw,
  Send,
  Sun,
  SunMoon,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { PageTitle } from "../components/Layout";
import { MinerTools } from "../components/MinerTools";
import {
  compact,
  Group,
  Item,
  TextField,
  type Props,
} from "../components/rows";
import {
  Button,
  ConfirmDialog,
  cx,
  Flag,
  Segmented,
  Switch as Sw,
} from "../components/ui";
import { api, errorText, inApp, local, type AppConfig } from "../lib/api";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/**
 * The app itself, as one page of grouped rows: general, this PC, notifications, data.
 * Electricity rules live in Energy, the machine and miners in Computer.
 */
export default function Settings() {
  const t = useT();
  const { config, saveConfig, toast, info } = useApp();
  const c = config!;

  const save = async (next: AppConfig) => {
    try {
      await saveConfig(next);
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    }
  };

  return (
    <div className="max-w-2xl">
      <PageTitle>{t("settings.title")}</PageTitle>
      <div className="space-y-9">
        <Group title={t("settings.group.general")}>
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
                { value: "dark", label: t("settings.theme.dark"), icon: <Moon /> },
                { value: "light", label: t("settings.theme.light"), icon: <Sun /> },
                { value: "system", label: t("settings.theme.system"), icon: <Monitor /> },
              ]}
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
        </Group>

        <Group title={t("settings.group.pc")}>
          <WorkerName c={c} save={save} />
          <StartupRows c={c} save={save} />
        </Group>

        <NotifyGroup c={c} save={save} />

        {/* Opt-in only: off until the user picks a share. Builds without a project
            wallet take no fee, so there is nothing to give to either. */}
        {info?.devFeeWalletConfigured && (
          <Group title={t("settings.group.support")}>
            <Item
              icon={<HeartHandshake />}
              tone="pink"
              label={t("settings.donation")}
              hint={t("settings.donation.hint", { f: info.devFeePercent })}
            >
              <Segmented
                label={t("settings.donation")}
                value={String(c.donationPercent ?? 0)}
                onChange={(v) => save({ ...c, donationPercent: Number(v) })}
                options={Array.from(
                  { length: info.maxDonationPercent + 1 },
                  (_, i) => ({
                    value: String(i),
                    label: i === 0 ? t("settings.donation.off") : `+${i}%`,
                  }),
                )}
              />
            </Item>
          </Group>
        )}

        <DataGroup c={c} />

        {/* Closed by default: someone new never has to look at miners and pools.
            One tinted panel: while scrolling, everything inside it is advanced. */}
        <details className="group/adv rounded-3xl border border-line bg-surface-2/60 open:pb-5">
          <summary className="flex cursor-default list-none items-center gap-2 rounded-3xl px-5 py-4 transition-colors hover:bg-surface-2 group-open/adv:mb-2 group-open/adv:rounded-b-none group-open/adv:border-b group-open/adv:border-line [&::-webkit-details-marker]:hidden">
            <ChevronRight className="size-4 text-faint transition-transform duration-200 group-open/adv:rotate-90" aria-hidden />
            <span className="text-sm font-semibold">{t("settings.advanced")}</span>
            <span className="text-xs text-faint">{t("settings.advanced.hint")}</span>
          </summary>
          <div className="space-y-9 px-4 pt-3">
            <Group>
              <Item
                icon={<RefreshCw />}
                tone="blue"
                label={t("settings.autoUpdateMiners")}
                hint={t("settings.autoUpdateMiners.desc")}
              >
                <Sw
                  label={t("settings.autoUpdateMiners")}
                  checked={c.autoUpdateMiners}
                  onChange={(autoUpdateMiners) => save({ ...c, autoUpdateMiners })}
                />
              </Item>
              <Item icon={<FlaskConical />} tone="violet" label={t("settings.demo")} hint={t("settings.demo.desc")}>
                <Sw
                  label={t("settings.demo")}
                  checked={c.demoMode}
                  onChange={(demoMode) => save({ ...c, demoMode })}
                />
              </Item>
            </Group>

            <MinerTools />
          </div>
        </details>

        <p className="px-1 text-xs leading-relaxed text-faint">
          {t("settings.privacy.desc")} {t("settings.licenses.desc")}
        </p>
      </div>
    </div>
  );
}

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
  const { setConfig, toast } = useApp();
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
  const saveCsv = async () => {
    try {
      const path = await api.saveCsv(366 * 86_400);
      toast("info", t("settings.csv.saved", { path }));
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
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
          icon={<FileSpreadsheet />}
          tone="green"
          label={t("settings.csv")}
          hint={t("settings.csv.hint")}
        >
          <Button onClick={saveCsv}>{t("settings.csv.short")}</Button>
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
      </Group>
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


/** Problems pushed to the phone through a Telegram bot (until Erebor has its own mobile app). */
function NotifyGroup({ c, save }: Props) {
  const t = useT();
  const toast = useApp((s) => s.toast);
  const n = c.notify;
  const set = (p: Partial<typeof n>) => save({ ...c, notify: { ...n, ...p } });
  const test = async () => {
    try {
      await api.testNotify();
      toast("info", t("notify.sent"));
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    }
  };
  return (
    <Group
      title={t("notify.group")}
      aside={
        <Button size="sm" variant="ghost" onClick={test}>
          {t("notify.test")}
        </Button>
      }
    >
      <Item icon={<Send />} tone="blue" label="Telegram" hint={t("notify.hint")}>
        <div className="flex gap-2">
          <TextField
            secret
            label={t("notify.telegramToken")}
            value={n.telegramToken}
            placeholder={t("notify.telegramToken")}
            onCommit={(telegramToken) => set({ telegramToken })}
          />
          <TextField
            label={t("notify.telegramChat")}
            value={n.telegramChat}
            placeholder={t("notify.telegramChat")}
            onCommit={(telegramChat) => set({ telegramChat })}
          />
        </div>
      </Item>
    </Group>
  );
}

