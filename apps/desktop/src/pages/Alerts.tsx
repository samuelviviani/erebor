import { CircleAlert, Info, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { useAgo, useLang } from "../components/fleet";
import { PageTitle } from "../components/Layout";
import { Group, Item, NumberField } from "../components/rows";
import { cx, Empty, Segmented } from "../components/ui";
import { errorText, fleet, type FleetEvent, type FleetSettings } from "../lib/api";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

const ICON = {
  info: <Info className="size-4 text-accent" />,
  warning: <TriangleAlert className="size-4 text-warn" />,
  error: <CircleAlert className="size-4 text-danger" />,
};

/** Text of a device event (alert or operation), in the user's language. */
export function eventText(t: ReturnType<typeof useT>, lang: "en" | "it", e: FleetEvent) {
  const key = `fleet.ev.${e.code}`;
  const base = t(key) === key ? translateError(lang, e.code) : t(key);
  return e.detail ? `${base} · ${translateError(lang, e.detail)}` : base;
}

/** Device alerts and operations, newest first, and when to raise them. */
export default function Alerts() {
  const t = useT();
  const lang = useLang();
  const ago = useAgo();
  const toast = useApp((s) => s.toast);
  const [events, setEvents] = useState<FleetEvent[]>([]);
  const [level, setLevel] = useState<"all" | "warning" | "error">("all");
  const [s, setS] = useState<FleetSettings | null>(null);
  useEffect(() => {
    const load = () => fleet.events(0, 300).then(setEvents).catch(() => {});
    load();
    fleet.settings().then(setS).catch(() => {});
    const i = setInterval(load, 5000);
    return () => clearInterval(i);
  }, []);
  const save = async (next: FleetSettings) => {
    try {
      setS(await fleet.setSettings(next));
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    }
  };
  const shown = events.filter((e) => level === "all" || e.level === level || (level === "warning" && e.level === "error"));
  return (
    <div className="max-w-3xl">
      <PageTitle sub={t("fleet.alerts.sub")}>{t("nav.alerts")}</PageTitle>
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
            <ul className="divide-y divide-line overflow-hidden rounded-2xl border border-line bg-surface shadow-[var(--highlight)]">
              {shown.map((e) => (
                <li key={e.id} className="flex items-start gap-3 px-4 py-2.5 text-sm">
                  <span className="mt-0.5">{ICON[e.level] ?? ICON.info}</span>
                  <span className="min-w-0 flex-1">
                    <span className={cx("block", e.level === "error" && "text-danger")}>{eventText(t, lang, e)}</span>
                    <span className="block text-xs text-faint">{e.deviceName ?? t("fleet.alerts.general")}</span>
                  </span>
                  <span className="num shrink-0 text-xs text-faint">{ago(e.ts)}</span>
                </li>
              ))}
            </ul>
          )}
        </section>

        {s && (
          <Group title={t("fleet.alerts.thresholds")}>
            <Item label={t("fleet.alerts.maxTemp")} hint={t("fleet.alerts.maxTemp.hint")}>
              <NumberField label={t("fleet.alerts.maxTemp")} value={s.maxTempC} suffix="°C" onCommit={(v) => v != null && save({ ...s, maxTempC: Math.min(110, Math.max(50, v)) })} />
            </Item>
            <Item label={t("fleet.alerts.asicMaxTemp")}>
              <NumberField label={t("fleet.alerts.asicMaxTemp")} value={s.asicMaxTempC} suffix="°C" onCommit={(v) => v != null && save({ ...s, asicMaxTempC: Math.min(120, Math.max(50, v)) })} />
            </Item>
            <Item label={t("fleet.alerts.hashDrop")} hint={t("fleet.alerts.hashDrop.hint")}>
              <NumberField label={t("fleet.alerts.hashDrop")} value={s.hashrateDropPercent} suffix="%" onCommit={(v) => v != null && save({ ...s, hashrateDropPercent: Math.min(95, Math.max(5, v)) })} />
            </Item>
            <Item label={t("fleet.alerts.powerRise")}>
              <NumberField label={t("fleet.alerts.powerRise")} value={s.powerRisePercent} suffix="%" onCommit={(v) => v != null && save({ ...s, powerRisePercent: Math.min(500, Math.max(5, v)) })} />
            </Item>
            <Item label={t("fleet.alerts.rejected")}>
              <NumberField label={t("fleet.alerts.rejected")} value={s.rejectedPercent} suffix="%" onCommit={(v) => v != null && save({ ...s, rejectedPercent: Math.min(50, Math.max(1, v)) })} />
            </Item>
            <Item label={t("fleet.alerts.cooldown")} hint={t("fleet.alerts.cooldown.hint")}>
              <NumberField label={t("fleet.alerts.cooldown")} value={s.cooldownMinutes} suffix="min" onCommit={(v) => v != null && save({ ...s, cooldownMinutes: Math.min(1440, Math.max(0, v)) })} />
            </Item>
          </Group>
        )}
      </div>
    </div>
  );
}
