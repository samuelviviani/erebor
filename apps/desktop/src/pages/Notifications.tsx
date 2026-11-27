import { Bell, CalendarDays, Moon, Send } from "lucide-react";
import { useEffect, useState } from "react";
import { DeviceThresholds, LEVEL_ICON, useNotes } from "../components/Alerts";
import { useAgo } from "../components/fleet";
import { PageTitle } from "../components/Layout";
import { Group, Item, TextField, type Props } from "../components/rows";
import { Button, compactInput, cx, Empty, Segmented, Switch as Sw } from "../components/ui";
import { api, errorText, systemNotify, type AppConfig } from "../lib/api";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/**
 * Everything the app had to tell: this computer's problems and the other devices' alerts,
 * newest first; then where they are pushed and, with devices, when they are raised.
 * Opening the page marks them seen (the sidebar badge clears).
 */
export default function Notifications() {
  const t = useT();
  const ago = useAgo();
  const { config, saveConfig, toast, markSeen } = useApp();
  const fleetOn = useApp((s) => s.config?.defaultView === "devices" || (s.fleet?.devices.length ?? 0) > 1);
  const notes = useNotes();
  const [level, setLevel] = useState<"all" | "warning" | "error">("all");
  useEffect(markSeen, [notes.length, markSeen]);
  const c = config!;
  const save = async (next: AppConfig) => {
    try {
      await saveConfig(next);
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    }
  };
  const shown = notes.filter((n) => level === "all" || n.level === level || (level === "warning" && n.level === "error"));
  return (
    <div className="max-w-2xl">
      <PageTitle sub={t("notifications.sub")}>{t("nav.notifications")}</PageTitle>
      <div className="space-y-9">
        <section>
          <div className="mb-3">
            <Segmented
              size="sm"
              label={t("fleet.filter")}
              value={level}
              onChange={setLevel}
              options={[
                { value: "all", label: t("fleet.alerts.all") },
                { value: "warning", label: t("fleet.alerts.problems") },
                { value: "error", label: t("fleet.alerts.errors") },
              ]}
            />
          </div>
          {shown.length === 0 ? (
            <Empty>{t("fleet.alerts.empty")}</Empty>
          ) : (
            <ul className="max-h-[480px] divide-y divide-line overflow-y-auto rounded-2xl border border-line bg-surface shadow-[var(--highlight)]">
              {shown.map((n) => (
                <li key={n.key} className="flex items-start gap-3 px-4 py-2.5 text-sm">
                  <span className="mt-0.5">{LEVEL_ICON[n.level] ?? LEVEL_ICON.info}</span>
                  <span className="min-w-0 flex-1">
                    <span className={cx("block", n.level === "error" && "text-danger")}>{n.text}</span>
                    <span className="block text-xs text-faint">{n.source}</span>
                  </span>
                  <span className="num shrink-0 text-xs text-faint">{ago(n.ts)}</span>
                </li>
              ))}
            </ul>
          )}
        </section>
        <NotifyGroup c={c} save={save} />
        {fleetOn && <DeviceThresholds />}
      </div>
    </div>
  );
}

const hhmm = (m: number) => `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
const minutes = (v: string) => {
  const [h, m] = v.split(":").map(Number);
  return Number.isFinite(h) && Number.isFinite(m) ? h * 60 + m : null;
};

/**
 * Where problems are pushed: system notifications (sent by the app's notifier, also from
 * the tray) and a Telegram bot for the phone. "Test" sends on every channel that is on.
 */
function NotifyGroup({ c, save }: Props) {
  const t = useT();
  const toast = useApp((s) => s.toast);
  const n = c.notify;
  const set = (p: Partial<typeof n>) => save({ ...c, notify: { ...n, ...p } });
  const telegram = !!(n.telegramToken && n.telegramChat);
  const test = async () => {
    try {
      if (!n.desktop && !telegram) throw "notify_not_configured";
      if (n.desktop) await systemNotify("Erebor", t("notify.test.body"));
      if (telegram) await api.testNotify();
      toast("info", t("notify.sent"));
    } catch (e) {
      toast("error", translateError(c.language, errorText(e)));
    }
  };
  const time = (label: string, value: number, key: "quietStart" | "quietEnd") => (
    <input
      type="time"
      aria-label={label}
      className={cx(compactInput, "num w-28")}
      value={hhmm(value)}
      onChange={(e) => {
        const m = minutes(e.target.value);
        if (m != null) set({ [key]: m });
      }}
    />
  );
  return (
    <Group
      title={t("notify.group")}
      aside={
        <Button size="sm" variant="ghost" onClick={test}>
          {t("notify.test")}
        </Button>
      }
    >
      <Item icon={<Bell />} tone="red" label={t("notify.desktop")} hint={t("notify.desktop.hint")}>
        <Sw label={t("notify.desktop")} checked={n.desktop} onChange={(desktop) => set({ desktop })} />
      </Item>
      <Item icon={<Moon />} tone="indigo" label={t("notify.quiet")} hint={t("notify.quiet.hint")}>
        <div className="flex items-center gap-2">
          {n.quiet && (
            <>
              {time(t("notify.quiet"), n.quietStart, "quietStart")}
              <span className="text-faint">–</span>
              {time(t("notify.quiet"), n.quietEnd, "quietEnd")}
            </>
          )}
          <Sw label={t("notify.quiet")} checked={n.quiet} onChange={(quiet) => set({ quiet })} />
        </div>
      </Item>
      <Item icon={<CalendarDays />} tone="green" label={t("notify.monthly")} hint={t("notify.monthly.hint")}>
        <Sw label={t("notify.monthly")} checked={n.monthlySummary} onChange={(monthlySummary) => set({ monthlySummary })} />
      </Item>
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

