import { CircleAlert, Info, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { alertText } from "../lib/alerts";
import { errorText, fleet, type FleetEvent, type FleetSettings } from "../lib/api";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { useLang } from "./fleet";
import { Group, Item, NumberField } from "./rows";

export const LEVEL_ICON = {
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

export interface Note {
  key: string;
  ts: number;
  level: "info" | "warning" | "error";
  text: string;
  /** This computer, or the device's name. */
  source: string;
}

/**
 * Every notification of the app, newest first: this computer's engine alerts (those the
 * screen doesn't already show) and the other devices' events. The local device's own
 * events are left out: its engine already reported them.
 */
export function useNotes(): Note[] {
  const t = useT();
  const lang = useLang();
  const alerts = useApp((s) => s.status?.alerts);
  const events = useApp((s) => s.events);
  return useMemo(() => {
    const here = t("nav.computer");
    const local = (alerts ?? []).flatMap((a) => {
      const text = alertText(lang, a);
      return text ? [{ key: `a${a.id}`, ts: a.ts, level: a.level, text, source: here }] : [];
    });
    const devices = events
      .filter((e) => e.deviceId !== "local")
      .map((e) => ({ key: `e${e.id}`, ts: e.ts, level: e.level, text: eventText(t, lang, e), source: e.deviceName ?? t("fleet.alerts.general") }));
    return [...local, ...devices].sort((a, b) => b.ts - a.ts);
  }, [alerts, events, lang, t]);
}

/** Warnings and errors not seen yet: the badge in the sidebar. */
export function useUnread() {
  const notes = useNotes();
  const seen = useApp((s) => s.seenTs);
  return notes.filter((n) => n.ts > seen && n.level !== "info").length;
}

/** When a device raises an alert (thresholds of the controller). */
export function DeviceThresholds() {
  const t = useT();
  const lang = useLang();
  const toast = useApp((s) => s.toast);
  const [s, setS] = useState<FleetSettings | null>(null);
  useEffect(() => {
    fleet.settings().then(setS).catch(() => {});
  }, []);
  const save = async (next: FleetSettings) => {
    try {
      setS(await fleet.setSettings(next));
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    }
  };
  if (!s) return null;
  return (
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
  );
}
