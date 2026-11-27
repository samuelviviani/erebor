import { Flame, SunMedium, Thermometer } from "lucide-react";
import { Item, MONTHS, NumberField, Sub, SubField, TextField, Toggles, type Props } from "./rows";
import { cx, Switch as Sw } from "./ui";
import { type Solar, type Thermostat } from "../lib/api";
import { useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/** Starting points for common meters: the user completes address and sensor. */
const METERS: { label: string; solar: Partial<Solar> }[] = [
  {
    label: "Home Assistant",
    solar: {
      url: "http://homeassistant.local:8123/api/states/sensor.",
      field: "state",
      exportPositive: false,
    },
  },
  {
    label: "Shelly Pro 3EM",
    solar: {
      url: "http://192.168.1.50/rpc/EM.GetStatus?id=0",
      field: "total_act_power",
      exportPositive: false,
      token: "",
    },
  },
  {
    label: "Shelly EM",
    solar: {
      url: "http://192.168.1.50/emeter/0",
      field: "power",
      exportPositive: false,
      token: "",
    },
  },
];

/** A reading from a device on the home network: a coloured dot and one plain sentence. */
function Reading({ error, text }: { error: string | null | undefined; text: string }) {
  const t = useT();
  return (
    <p
      role="status"
      className={cx("flex items-center gap-2 text-xs", error ? "text-stop" : "text-muted")}
    >
      <span
        aria-hidden
        className={cx("size-2 shrink-0 rounded-full", error ? "bg-stop" : "bg-go")}
      />
      {error ? t("energy.solar.error", { e: error }) : text}
    </p>
  );
}

/** Where the sensor is and how to read it: the same three fields for meter and thermometer. */
function SensorFields({
  url,
  field,
  token,
  urlLabel,
  urlHint,
  set,
}: {
  url: string;
  field: string;
  token: string;
  urlLabel: string;
  urlHint: string;
  set: (p: { url?: string; field?: string; token?: string }) => void;
}) {
  const t = useT();
  return (
    <>
      <SubField label={urlLabel} hint={urlHint}>
        <TextField
          wide
          label={urlLabel}
          value={url}
          placeholder="http://…"
          onCommit={(url) => set({ url })}
        />
      </SubField>
      <SubField label={t("energy.field.label")} hint={t("energy.field.hint")}>
        <TextField
          label={t("energy.field.label")}
          value={field}
          placeholder="state"
          onCommit={(field) => set({ field })}
        />
      </SubField>
      <SubField label={t("energy.token.label")} hint={t("energy.token.hint")}>
        <TextField
          wide
          secret
          label={t("energy.token.label")}
          value={token}
          placeholder={t("energy.token.placeholder")}
          onCommit={(token) => set({ token })}
        />
      </SubField>
    </>
  );
}

/** Solar surplus from a meter: energy that would be exported costs only its feed-in value. */
export function SolarRows({ c, save, cur }: Props & { cur: string }) {
  const t = useT();
  const exportW = useApp((s) => s.status?.solarExportW);
  const error = useApp((s) => s.status?.solarError);
  const sol = c.energy.solar;
  const set = (p: Partial<Solar>) =>
    save({ ...c, energy: { ...c.energy, solar: { ...sol, ...p } } });
  return (
    <>
      <Item
        icon={<SunMedium />}
        tone="yellow"
        label={t("energy.solar")}
        hint={t("energy.solar.hint")}
      >
        <Sw
          label={t("energy.solar")}
          checked={sol.enabled}
          onChange={(enabled) =>
            set(
              enabled && !sol.url
                ? { enabled, ...METERS[0].solar }
                : { enabled },
            )
          }
        />
      </Item>
      {sol.enabled && (
        <Sub>
          <SubField label={t("energy.solar.meter")} hint={t("energy.solar.meter.hint")}>
            <div className="flex flex-wrap gap-1.5">
              {METERS.map((m) => (
                <button
                  key={m.label}
                  type="button"
                  onClick={() => set(m.solar)}
                  aria-pressed={sol.url.startsWith(m.solar.url!.split("sensor.")[0])}
                  className="press h-8 rounded-full border border-line px-3 text-xs text-muted transition-colors hover:border-line-strong hover:text-fg aria-pressed:border-accent/30 aria-pressed:bg-accent-soft aria-pressed:text-accent"
                >
                  {m.label}
                </button>
              ))}
            </div>
          </SubField>
          <SensorFields
            url={sol.url}
            field={sol.field}
            token={sol.token}
            urlLabel={t("energy.solar.url")}
            urlHint={t("energy.solar.url.hint")}
            set={set}
          />
          <SubField label={t("energy.solar.feedIn.short")} hint={t("energy.solar.feedIn")}>
            <NumberField
              label={t("energy.solar.feedIn")}
              prefix={cur}
              suffix="/kWh"
              value={sol.feedInPrice}
              onCommit={(v) => set({ feedInPrice: v ?? 0 })}
            />
          </SubField>
          <SubField label={t("energy.solar.sign")} hint={t("energy.solar.positive")}>
            <Sw
              label={t("energy.solar.positive")}
              checked={sol.exportPositive}
              onChange={(exportPositive) => set({ exportPositive })}
            />
          </SubField>
          <Reading
            error={error}
            text={
              exportW == null
                ? t("energy.solar.waiting")
                : exportW >= 0
                  ? t("energy.solar.exporting", { w: exportW.toFixed(0) })
                  : t("energy.solar.importing", { w: (-exportW).toFixed(0) })
            }
          />
        </Sub>
      )}
    </>
  );
}

/** Winter: the heat of mining replaces other heating, so part of the cost comes back. */
export function Heating({ c, save }: Props) {
  const t = useT();
  const e = c.energy;
  const on = e.heatingMonths.some(Boolean);
  return (
    <>
      <Item
        icon={<Flame />}
        tone="orange"
        label={t("energy.heating")}
        hint={t("energy.heating.hint")}
      >
        <Sw
          label={t("energy.heating")}
          checked={on}
          onChange={(v) =>
            save({
              ...c,
              energy: {
                ...e,
                // Nov–Mar by default; every month can be changed below.
                heatingMonths: Array.from(
                  { length: 12 },
                  (_, m) => v && (m <= 2 || m >= 10),
                ),
                heatingCreditPercent:
                  v && !e.heatingCreditPercent ? 100 : e.heatingCreditPercent,
              },
            })
          }
        />
      </Item>
      {on && (
        <Sub>
          <SubField label={t("energy.months")}>
            <Toggles
              labels={MONTHS[c.language]}
              value={e.heatingMonths}
              onChange={(heatingMonths) =>
                save({ ...c, energy: { ...e, heatingMonths } })
              }
            />
          </SubField>
          <SubField label={t("energy.heating.credit")} hint={t("energy.heating.credit.hint")}>
            <NumberField
              label={t("energy.heating.credit")}
              suffix="%"
              value={e.heatingCreditPercent}
              onCommit={(v) =>
                save({
                  ...c,
                  energy: {
                    ...e,
                    heatingCreditPercent: Math.min(
                      100,
                      Math.max(0, Math.round(v ?? 0)),
                    ),
                  },
                })
              }
            />
          </SubField>
        </Sub>
      )}
    </>
  );
}

/** Room thermometer: mine to heat up to one temperature, wait above it. */
export function ThermostatRows({ c, save }: Props) {
  const t = useT();
  const temp = useApp((s) => s.status?.roomTempC);
  const error = useApp((s) => s.status?.roomError);
  const th = c.energy.thermostat;
  const set = (p: Partial<Thermostat>) =>
    save({ ...c, energy: { ...c.energy, thermostat: { ...th, ...p } } });
  return (
    <>
      <Item
        icon={<Thermometer />}
        tone="red"
        label={t("energy.thermostat")}
        hint={t("energy.thermostat.hint")}
      >
        <Sw
          label={t("energy.thermostat")}
          checked={th.enabled}
          onChange={(enabled) =>
            set(
              enabled && !th.url
                ? {
                    enabled,
                    url: "http://homeassistant.local:8123/api/states/sensor.",
                    field: "state",
                  }
                : { enabled },
            )
          }
        />
      </Item>
      {th.enabled && (
        <Sub>
          {/* The rule first, as a sentence: what the user actually decides. */}
          <SubField
            label={t("energy.thermostat.rule")}
            hint={t("energy.thermostat.rule.hint", { on: th.onBelowC, off: th.offAboveC })}
          >
            <div className="flex flex-wrap items-center gap-2 text-muted">
              {t("energy.thermostat.on")}
              <NumberField
                label={t("energy.thermostat.on")}
                suffix="°C"
                value={th.onBelowC}
                onCommit={(v) => v != null && set({ onBelowC: v })}
              />
              {t("energy.thermostat.off")}
              <NumberField
                label={t("energy.thermostat.off")}
                suffix="°C"
                value={th.offAboveC}
                onCommit={(v) => v != null && set({ offAboveC: v })}
              />
            </div>
          </SubField>
          <SensorFields
            url={th.url}
            field={th.field}
            token={th.token}
            urlLabel={t("energy.thermostat.url")}
            urlHint={t("energy.thermostat.url.hint")}
            set={set}
          />
          <Reading
            error={error}
            text={
              temp == null
                ? t("energy.solar.waiting")
                : t("energy.thermostat.now", { t: temp.toFixed(1) })
            }
          />
        </Sub>
      )}
    </>
  );
}
