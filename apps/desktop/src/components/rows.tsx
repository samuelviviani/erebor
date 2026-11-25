import { type ReactNode } from "react";
import type { AppConfig } from "../lib/api";
import { useApp } from "../lib/store";
import { compactInput, cx, Explain, type HelpTerm } from "./ui";

/**
 * Settings-style rows shared by Settings, Energy and Computer: a titled group, one row per
 * option (label, one short line of help, the control on the right) and small inputs.
 */
export const DAYS = {
  it: ["Lun", "Mar", "Mer", "Gio", "Ven", "Sab", "Dom"],
  en: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
};


export type Props = { c: AppConfig; save: (next: AppConfig) => Promise<void> };

/** Small inputs for short values (time, price, watts): never a full-width box. */
export const compact = compactInput;

/** A titled block of rows. `aside`: a live reading or an action on the heading's right. */
export function Group({
  title,
  help,
  aside,
  children,
}: {
  title?: string;
  help?: HelpTerm;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section>
      {title && (
        <div className="mb-2.5 flex min-h-7 items-center justify-between gap-3 px-1">
          <h2 className="flex items-center gap-1.5 text-sm font-semibold text-fg">
            {title}
            {help && <Explain term={help} align="left" />}
          </h2>
          {aside && <div className="text-xs text-faint">{aside}</div>}
        </div>
      )}
      <div className="divide-y divide-line overflow-hidden rounded-2xl border border-line bg-surface shadow-[var(--highlight)]">
        {children}
      </div>
    </section>
  );
}

/** One colour per meaning, for the bare glyph at the start of a row (no tile behind it). */
export const TONES = {
  blue: "text-accent",
  green: "text-go",
  orange: "text-perf",
  red: "text-stop",
  yellow: "text-hold",
  indigo: "text-[#5e5ce6]",
  violet: "text-[#a855f7]",
  teal: "text-[#14b8a6]",
  pink: "text-[#ec4899]",
  gray: "text-faint",
} as const;
export type Tone = keyof typeof TONES;

/** One row: icon, what it is (and at most one short line of help), the control on the right. */
export function Item({
  icon,
  tone = "gray",
  label,
  hint,
  children,
}: {
  icon?: ReactNode;
  tone?: Tone;
  label: string;
  hint?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="flex min-h-13 items-center gap-3.5 px-4 py-2.5">
      {icon && (
        <span
          aria-hidden
          className={cx("flex w-5 shrink-0 justify-center [&>svg]:size-[18px] [&>svg]:stroke-[2]", TONES[tone])}
        >
          {icon}
        </span>
      )}
      <span className="min-w-0 flex-1">
        <span className="block text-sm text-fg">{label}</span>
        {hint && (
          <span className="mt-0.5 block text-xs leading-relaxed text-faint">
            {hint}
          </span>
        )}
      </span>
      {children && <div className="shrink-0">{children}</div>}
    </div>
  );
}

/**
 * What an option opens up once it is on: its fields, under the row that turned it on and
 * lined up with that row's text (past the icon), so they read as part of it.
 */
export function Sub({ children }: { children: ReactNode }) {
  return (
    <div className="space-y-4 bg-bg/40 py-4 pr-4 pl-[50px] text-sm">{children}</div>
  );
}

/** One field inside a `Sub`: its name on the left, the control and a plain hint on the right. */
export function SubField({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="grid gap-x-4 gap-y-1.5 sm:grid-cols-[8.5rem_minmax(0,1fr)]">
      <span className="text-sm text-muted sm:pt-2">{label}</span>
      <div className="min-w-0">
        {children}
        {hint && <p className="mt-1.5 text-xs leading-relaxed text-faint">{hint}</p>}
      </div>
    </div>
  );
}

export function StepButton({
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
      className="press flex size-8 items-center justify-center rounded-full border border-line-strong text-base text-muted transition-colors hover:bg-surface-2 hover:text-fg disabled:opacity-40"
    >
      {label}
    </button>
  );
}

/** A short number with its unit around it, e.g. "€ [0,25] /kWh". Saved when leaving the field. */
export function NumberField({
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
    <label className="inline-flex h-9 items-center gap-1.5 rounded-xl border border-line-strong bg-bg/60 px-2.5 text-sm transition-[border-color,box-shadow] duration-200 focus-within:border-accent focus-within:shadow-[0_0_0_4px_var(--accent-soft)]">
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

export const MONTHS = {
  it: ["Gen", "Feb", "Mar", "Apr", "Mag", "Giu", "Lug", "Ago", "Set", "Ott", "Nov", "Dic"],
  en: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
};

/** On/off chips for days or months: tinted when on, outlined when off. */
export function Toggles({
  labels,
  value,
  onChange,
}: {
  labels: string[];
  value: boolean[];
  onChange: (v: boolean[]) => void;
}) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {labels.map((d, i) => (
        <button
          key={i}
          type="button"
          aria-pressed={value[i]}
          onClick={() => onChange(value.map((v, j) => (j === i ? !v : v)))}
          className={cx(
            "press h-8 min-w-11 rounded-full border px-2.5 text-xs font-medium transition-colors",
            value[i]
              ? "border-accent/30 bg-accent-soft text-accent"
              : "border-line text-faint hover:border-line-strong hover:text-fg",
          )}
        >
          {d}
        </button>
      ))}
    </div>
  );
}

/** A short text saved when leaving the field. */
export function TextField({
  label,
  value,
  onCommit,
  placeholder,
  secret,
  wide,
}: {
  label: string;
  value: string;
  onCommit: (v: string) => void;
  placeholder?: string;
  secret?: boolean;
  wide?: boolean;
}) {
  return (
    <input
      key={value}
      aria-label={label}
      type={secret ? "password" : "text"}
      className={cx(compact, wide ? "w-full" : "w-40")}
      spellCheck={false}
      autoComplete="off"
      placeholder={placeholder}
      defaultValue={value}
      onBlur={(e) => {
        if (e.target.value.trim() !== value) onCommit(e.target.value.trim());
      }}
    />
  );
}

