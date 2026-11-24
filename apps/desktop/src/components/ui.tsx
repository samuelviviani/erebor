import NumberFlow from "@number-flow/react";
import { CircleHelp, Loader2, X } from "lucide-react";
import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type ReactNode,
} from "react";
import { useT } from "../lib/i18n";

export function cx(...c: (string | false | null | undefined)[]) {
  return c.filter(Boolean).join(" ");
}

export function Card({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return <section className={cx("panel p-5", className)}>{children}</section>;
}

export function CardTitle({
  children,
  action,
  help,
  icon,
  sub,
}: {
  children: ReactNode;
  action?: ReactNode;
  help?: HelpTerm;
  icon?: ReactNode;
  sub?: ReactNode;
}) {
  return (
    <div className="mb-4 flex items-start justify-between gap-3">
      <div className="min-w-0">
        <h2 className="flex items-center gap-2 text-[15px] font-semibold text-fg">
          {icon && (
            <IconTile className="size-7 rounded-lg [&>svg]:size-4">{icon}</IconTile>
          )}
          {children}
          {help && <Explain term={help} />}
        </h2>
        {sub && <p className="mt-1 text-xs leading-relaxed text-faint">{sub}</p>}
      </div>
      {action}
    </div>
  );
}

/** Tinted square behind an icon: the same "chip" everywhere (settings, modals, lists). */
export function IconTile({
  children,
  tone = "accent",
  className,
}: {
  children: ReactNode;
  tone?: Tone;
  className?: string;
}) {
  return (
    <span
      className={cx(
        "flex size-9 shrink-0 items-center justify-center rounded-xl [&>svg]:size-[18px]",
        TONE_TILE[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

/** Semantic tones: one colour per meaning, used by tiles, badges and notices. */
export type Tone =
  | "accent"
  | "go"
  | "warn"
  | "danger"
  | "perf"
  | "violet"
  | "teal"
  | "neutral";
const TONE_TILE: Record<Tone, string> = {
  accent: "bg-accent/14 text-accent",
  go: "bg-go/14 text-go",
  warn: "bg-warn/14 text-warn",
  danger: "bg-danger/14 text-danger",
  perf: "bg-perf/14 text-perf",
  violet: "bg-[#a78bfa]/14 text-[#a78bfa]",
  teal: "bg-[#2dd4bf]/14 text-[#14b8a6]",
  neutral: "bg-surface-3 text-muted",
};

/** Glossary terms explained in plain language (texts in i18n: `help.<term>.title/.body`). */
export type HelpTerm =
  | "earnings"
  | "coin"
  | "pool"
  | "wallet"
  | "kryptex"
  | "gpu"
  | "auto"
  | "temp"
  | "power"
  | "miner"
  | "mode"
  | "btc"
  | "mined"
  | "estimate"
  | "ownWallets"
  | "poolChoice"
  | "backupPool";

/**
 * "?" button that opens a short, plain-language explanation. Click, Enter or Space
 * opens it; Escape or a click outside closes it.
 */
export function Explain({
  term,
  align = "center",
}: {
  term: HelpTerm;
  align?: "center" | "left" | "right";
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) =>
      ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);
  const pos = {
    center: "left-1/2 -translate-x-1/2",
    left: "-left-2",
    right: "-right-2",
  }[align];
  return (
    <span ref={ref} className="relative inline-flex align-middle font-sans">
      <button
        type="button"
        aria-label={`${t("help.what")} ${t(`help.${term}.title`)}`}
        aria-expanded={open}
        onClick={(e) => {
          e.stopPropagation();
          setOpen(!open);
        }}
        className={cx(
          "inline-flex rounded-full text-faint transition-colors duration-200 hover:text-accent",
          open && "text-accent",
        )}
      >
        <CircleHelp className="size-[15px]" aria-hidden />
      </button>
      {open && (
        <span
          role="dialog"
          aria-label={t(`help.${term}.title`)}
          className={cx(
            "pop-in glass absolute top-7 z-40 w-76 rounded-2xl p-4 text-left text-[13px] leading-relaxed font-normal tracking-normal text-muted normal-case",
            pos,
          )}
        >
          <span className="mb-1.5 block text-[15px] font-semibold text-fg">
            {t(`help.${term}.title`)}
          </span>
          {t(`help.${term}.body`)}
        </span>
      )}
    </span>
  );
}

type Variant = "primary" | "secondary" | "ghost" | "danger" | "go" | "stop";
type Size = "sm" | "md" | "lg";

/** The one button: pill-shaped, three sizes, a spring on press. */
export function Button({
  variant = "secondary",
  size = "md",
  loading,
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  size?: Size;
  loading?: boolean;
}) {
  const styles: Record<Variant, string> = {
    primary:
      "bg-gradient-to-b from-accent to-accent-solid text-accent-fg shadow-[inset_0_1px_0_rgb(255_255_255/0.22),0_8px_20px_-8px_var(--accent-solid)] hover:brightness-110",
    secondary:
      "border border-line-strong bg-surface-2 text-fg shadow-[var(--highlight)] hover:bg-surface-3",
    ghost: "text-muted hover:bg-surface-2 hover:text-fg",
    danger: "border border-danger/35 bg-danger/8 text-danger hover:bg-danger/15",
    go: "bg-go/14 text-go hover:bg-go/22",
    stop: "bg-stop/12 text-stop hover:bg-stop/20",
  };
  const sizes: Record<Size, string> = {
    sm: "h-8 px-3 text-xs [&>svg]:size-3.5",
    md: "h-9 px-4 text-[13px] [&>svg]:size-4",
    lg: "h-12 px-6 text-[15px] [&>svg]:size-[18px]",
  };
  return (
    <button
      type="button"
      className={cx(
        "inline-flex shrink-0 items-center justify-center gap-2 rounded-full font-medium whitespace-nowrap transition-[color,background-color,border-color,transform,filter,box-shadow] duration-200 ease-[var(--ease)] active:scale-[0.98] disabled:pointer-events-none disabled:opacity-45",
        styles[variant],
        sizes[size],
        className,
      )}
      disabled={rest.disabled || loading}
      {...rest}
    >
      {loading && <Loader2 className="animate-spin" aria-hidden />}
      {children}
    </button>
  );
}

export function Badge({
  children,
  tone = "neutral",
}: {
  children: ReactNode;
  tone?: "neutral" | "ok" | "warn" | "err";
}) {
  const t = {
    neutral: "bg-surface-3 text-muted",
    ok: "bg-go/12 text-go",
    warn: "bg-warn/14 text-warn",
    err: "bg-danger/14 text-danger",
    }[tone];
  return (
    <span
      className={cx(
        "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold [&>svg]:size-3",
        t,
      )}
    >
      {children}
    </span>
  );
}

/** Bare switch (the row around it shows the label). The only switch in the app. */
export function Switch({
  label,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cx(
        "relative block h-[26px] w-[44px] shrink-0 rounded-full transition-colors duration-300 disabled:opacity-45",
        checked
          ? "bg-go shadow-[inset_0_1px_2px_rgb(0_0_0/0.15)]"
          : "bg-surface-3 shadow-[inset_0_0_0_1px_var(--border-strong)]",
      )}
    >
      <span
        className={cx(
          "absolute top-[3px] left-[3px] size-5 rounded-full bg-white shadow-[0_2px_6px_rgb(0_0_0/0.3)] transition-transform duration-300 ease-[var(--spring)]",
          checked && "translate-x-[18px]",
        )}
      />
    </button>
  );
}

/** Switch with its label and description on the left. */
export function Toggle({
  checked,
  onChange,
  label,
  description,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  description?: string;
  disabled?: boolean;
}) {
  return (
    <div
      className={cx(
        "flex items-center justify-between gap-4 py-2",
        disabled && "opacity-50",
      )}
    >
      <span className="min-w-0">
        <span className="block text-sm">{label}</span>
        {description && (
          <span className="mt-0.5 block text-xs leading-relaxed text-faint">
            {description}
          </span>
        )}
      </span>
      <Switch
        label={label}
        checked={checked}
        onChange={onChange}
        disabled={disabled}
      />
    </div>
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-[13px] font-medium text-muted">{label}</span>
      {children}
      {hint && <span className="mt-1.5 block text-xs leading-relaxed text-faint">{hint}</span>}
    </label>
  );
}

export const inputClass =
  "h-11 w-full rounded-xl border border-line-strong bg-bg/60 px-3.5 text-sm text-fg shadow-[inset_0_1px_2px_rgb(0_0_0/0.12)] transition-[border-color,box-shadow] duration-200 placeholder:text-faint focus:border-accent focus:shadow-[0_0_0_4px_var(--accent-soft)] focus:outline-none disabled:opacity-60 aria-[invalid=true]:border-danger/60";

/** Short inputs (time, price, watts): never a full-width box. */
export const compactInput =
  "h-9 rounded-xl border border-line-strong bg-bg/60 px-2.5 text-sm text-fg transition-[border-color,box-shadow] duration-200 focus:border-accent focus:shadow-[0_0_0_4px_var(--accent-soft)] focus:outline-none";

/**
 * Segmented control with a thumb that slides to the chosen option (measured, so labels of
 * any width work). Arrow keys move the choice like a native radio group.
 */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
  size = "md",
}: {
  value: T;
  /** `title`: accessible name when the option shows only an icon. */
  options: { value: T; label: string; icon?: ReactNode; title?: string }[];
  onChange: (v: T) => void;
  label: string;
  size?: "sm" | "md";
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const [thumb, setThumb] = useState<{ x: number; w: number } | null>(null);
  const idx = options.findIndex((o) => o.value === value);
  const labels = options.map((o) => o.label).join("|");
  useLayoutEffect(() => {
    const measure = () => {
      const el = refs.current[idx];
      setThumb(el ? { x: el.offsetLeft, w: el.offsetWidth } : null);
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [idx, labels]);
  const move = (i: number) => {
    const n = options.length;
    const next = (i + n) % n;
    refs.current[next]?.focus();
    onChange(options[next].value);
  };
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="relative inline-flex max-w-full rounded-full border border-line bg-bg/60 p-[3px] shadow-[inset_0_1px_2px_rgb(0_0_0/0.15)]"
    >
      {thumb && thumb.w > 0 && (
        <span
          aria-hidden
          className="absolute top-[3px] bottom-[3px] left-0 rounded-full border border-line-strong bg-surface-3 shadow-[var(--highlight),0_2px_8px_rgb(0_0_0/0.2)] transition-[transform,width] duration-300 ease-[var(--ease)]"
          style={{ width: thumb.w, transform: `translateX(${thumb.x}px)` }}
        />
      )}
      {options.map((o, i) => (
        <button
          key={o.value}
          ref={(el) => {
            refs.current[i] = el;
          }}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          aria-label={o.title}
          title={o.title}
          tabIndex={i === idx || (idx < 0 && i === 0) ? 0 : -1}
          onClick={() => onChange(o.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowRight" || e.key === "ArrowDown") (e.preventDefault(), move(i + 1));
            if (e.key === "ArrowLeft" || e.key === "ArrowUp") (e.preventDefault(), move(i - 1));
          }}
          className={cx(
            "relative z-10 inline-flex items-center gap-1.5 rounded-full font-medium whitespace-nowrap transition-colors duration-200 [&>svg]:size-3.5",
            size === "sm" ? "h-7 px-3 text-xs" : "h-8 px-3.5 text-[13px]",
            value === o.value ? "text-fg" : "text-muted hover:text-fg",
          )}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** Language flags as inline SVG (emoji flags do not render on Windows). */
export function Flag({ lang }: { lang: "en" | "it" }) {
  const id = useId();
  const cls =
    "!h-3.5 !w-5 shrink-0 rounded-[3px] shadow-[0_0_0_0.5px_rgb(0_0_0/0.25)]";
  if (lang === "it")
    return (
      <svg viewBox="0 0 3 2" className={cls} aria-hidden>
        <rect width="1" height="2" fill="#009246" />
        <rect x="1" width="1" height="2" fill="#fff" />
        <rect x="2" width="1" height="2" fill="#ce2b37" />
      </svg>
    );
  return (
    <svg
      viewBox="0 0 60 30"
      preserveAspectRatio="xMidYMid slice"
      className={cls}
      aria-hidden
    >
      <clipPath id={`${id}t`}>
        <path d="M30,15 h30 v15 z v15 h-30 z h-30 v-15 z v-15 h30 z" />
      </clipPath>
      <rect width="60" height="30" fill="#012169" />
      <path d="M0,0 L60,30 M60,0 L0,30" stroke="#fff" strokeWidth="6" />
      <path
        d="M0,0 L60,30 M60,0 L0,30"
        clipPath={`url(#${id}t)`}
        stroke="#C8102E"
        strokeWidth="4"
      />
      <path d="M30,0 v30 M0,15 h60" stroke="#fff" strokeWidth="10" />
      <path d="M30,0 v30 M0,15 h60" stroke="#C8102E" strokeWidth="6" />
    </svg>
  );
}

export function Progress({ value }: { value: number }) {
  return (
    <div
      className="h-1.5 w-full overflow-hidden rounded-full bg-surface-3"
      role="progressbar"
      aria-valuenow={Math.round(value * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div
        className="bar-live h-full rounded-full transition-[width] duration-500 ease-[var(--ease)]"
        style={{ width: `${Math.max(3, Math.round(value * 100))}%` }}
      />
    </div>
  );
}

export function Empty({
  icon,
  children,
}: {
  icon?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-10 text-center text-sm text-faint">
      {icon && <IconTile tone="neutral">{icon}</IconTile>}
      {children}
    </div>
  );
}

export function Notice({
  tone = "neutral",
  icon,
  children,
}: {
  tone?: "neutral" | "ok" | "warn" | "err";
  icon?: ReactNode;
  children: ReactNode;
}) {
  const t = {
    neutral: "border-line bg-surface-2/70 text-muted",
    ok: "border-accent/25 bg-accent-soft text-fg",
    warn: "border-warn/25 bg-warn/8 text-fg",
    err: "border-danger/25 bg-danger/8 text-fg",
  }[tone];
  const ic = {
    neutral: "text-faint",
    ok: "text-accent",
    warn: "text-warn",
    err: "text-danger",
  }[tone];
  return (
    <div
      className={cx(
        "flex gap-3 rounded-2xl border px-4 py-3 text-[13px] leading-relaxed",
        t,
      )}
    >
      {icon && <span className={cx("mt-0.5 shrink-0 [&>svg]:size-4", ic)}>{icon}</span>}
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

export function Row({
  label,
  children,
}: {
  label: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-line py-3 text-sm last:border-0">
      <span className="text-muted">{label}</span>
      <span className="num min-w-0 truncate text-right">{children}</span>
    </div>
  );
}

/** Accessible modal sheet (native <dialog>): Escape closes, focus stays inside, blurred backdrop. */
export function Modal({
  open,
  onClose,
  title,
  icon,
  children,
  actions,
  closeLabel,
  wide,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  icon?: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
  closeLabel: string;
  wide?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const d = ref.current;
    if (!d || !open || d.open) return;
    // Real modal (backdrop, focus trap) where supported; plain open elsewhere (tests).
    if (typeof d.showModal === "function") d.showModal();
    else d.setAttribute("open", "");
  }, [open]);
  if (!open) return null;
  return (
    <dialog
      ref={ref}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
      aria-labelledby={titleId}
      className={cx(
        "sheet fixed inset-0 z-50 m-auto max-h-[calc(100vh-48px)] overflow-hidden rounded-[26px] border border-line-strong bg-surface p-0 text-fg shadow-[var(--highlight),var(--shadow-lg)]",
        wide
          ? "w-[min(600px,calc(100vw-32px))]"
          : "w-[min(440px,calc(100vw-32px))]",
      )}
    >
      <div className="flex max-h-[calc(100vh-50px)] flex-col">
        <div className="flex items-start gap-4 px-6 pt-6">
          {icon && <div className="shrink-0">{icon}</div>}
          <div className="min-w-0 flex-1 self-center">
            <h2 id={titleId} className="font-display text-xl leading-snug">
              {title}
            </h2>
          </div>
          <button
            type="button"
            aria-label={closeLabel}
            onClick={onClose}
            className="-mt-1 -mr-2 flex size-8 shrink-0 items-center justify-center rounded-full text-faint transition-colors hover:bg-surface-2 hover:text-fg"
          >
            <X className="size-4" />
          </button>
        </div>
        {children && (
          <div className="mt-3 min-h-0 overflow-y-auto px-6 pb-1 text-sm leading-relaxed text-muted">
            {children}
          </div>
        )}
        {actions && (
          <div className="mt-5 flex flex-wrap justify-end gap-2 border-t border-line bg-bg/30 px-6 py-4">
            {actions}
          </div>
        )}
        {!actions && <div className="h-6" />}
      </div>
    </dialog>
  );
}

export function ConfirmDialog({
  open,
  title,
  text,
  icon,
  confirmLabel,
  cancelLabel,
  danger,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title?: ReactNode;
  text: ReactNode;
  icon?: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal
      open={open}
      onClose={onCancel}
      title={title ?? text}
      icon={icon}
      closeLabel={cancelLabel}
      actions={
        <>
          <Button variant="ghost" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button variant={danger ? "danger" : "primary"} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      {title ? text : null}
    </Modal>
  );
}

/**
 * A figure whose digits roll to each new value (NumberFlow). Same formatting as
 * `money()`/`btcFull()`; respects reduced motion.
 */
export function Ticker({
  value,
  locale,
  currency,
  decimals,
  suffix,
}: {
  value: number;
  locale: string;
  /** ISO code ("EUR"): formats as money, 4 decimals below 1 like `money()`. */
  currency?: string;
  /** Fixed decimals (e.g. 8 for BTC). */
  decimals?: number;
  suffix?: string;
}) {
  const digits =
    decimals ?? (currency ? (Math.abs(value) < 1 ? 4 : 2) : undefined);
  return (
    <NumberFlow
      value={value}
      locales={locale}
      suffix={suffix}
      format={{
        ...(currency ? { style: "currency" as const, currency } : {}),
        minimumFractionDigits: currency ? 2 : digits,
        maximumFractionDigits: digits,
      }}
    />
  );
}
