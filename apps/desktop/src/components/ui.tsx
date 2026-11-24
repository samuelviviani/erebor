import { CircleHelp, Info, Loader2, X } from "lucide-react";
import {
  useEffect,
  useId,
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
  return (
    <section
      className={cx("rounded-2xl border border-line bg-surface p-5", className)}
    >
      {children}
    </section>
  );
}

export function CardTitle({
  children,
  action,
  help,
  icon,
}: {
  children: ReactNode;
  action?: ReactNode;
  help?: HelpTerm;
  icon?: ReactNode;
}) {
  return (
    <div className="mb-4 flex items-center justify-between gap-3">
      <h2 className="flex items-center gap-2 text-[15px] font-medium text-fg">
        {icon && <span className="text-faint [&>svg]:size-[18px]">{icon}</span>}
        {children}
        {help && <Explain term={help} />}
      </h2>
      {action}
    </div>
  );
}

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
  | "estimate";

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
    left: "left-0",
    right: "right-0",
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
          "inline-flex rounded-full text-faint transition-colors hover:text-accent",
          open && "text-accent",
        )}
      >
        <CircleHelp className="size-4" aria-hidden />
      </button>
      {open && (
        <span
          role="dialog"
          aria-label={t(`help.${term}.title`)}
          className={cx(
            "pop-in shadow-soft absolute top-6 z-40 w-72 rounded-xl border border-line bg-surface p-4 text-left text-[13px] leading-relaxed font-normal tracking-normal text-muted normal-case",
            pos,
          )}
        >
          <span className="font-display mb-1.5 block text-base text-fg">
            {t(`help.${term}.title`)}
          </span>
          {t(`help.${term}.body`)}
        </span>
      )}
    </span>
  );
}

type Variant = "primary" | "secondary" | "ghost" | "danger";

export function Button({
  variant = "secondary",
  loading,
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  loading?: boolean;
}) {
  const styles: Record<Variant, string> = {
    primary:
      "bg-accent-solid text-accent-fg shadow-[0_6px_20px_-8px] shadow-accent-solid/70 hover:brightness-110",
    secondary: "border border-line bg-surface text-fg hover:bg-surface-2",
    ghost: "text-muted hover:text-fg hover:bg-surface-2",
    danger: "border border-danger/40 text-danger hover:bg-danger/10",
  };
  return (
    <button
      type="button"
      className={cx(
        "inline-flex h-9 items-center justify-center gap-2 rounded-xl px-4 text-[13px] font-medium transition-[color,background-color,border-color,transform,box-shadow] duration-200 active:scale-[0.97] disabled:cursor-not-allowed disabled:opacity-50",
        styles[variant],
        className,
      )}
      disabled={rest.disabled || loading}
      {...rest}
    >
      {loading && <Loader2 className="size-4 animate-spin" aria-hidden />}
      {children}
    </button>
  );
}

/** Small info icon with an accessible tooltip (also readable by screen readers). */
export function Tip({ text }: { text: string }) {
  return (
    <span className="group relative inline-flex align-middle">
      <span
        tabIndex={0}
        aria-label={text}
        className="inline-flex text-faint hover:text-muted focus:text-muted"
      >
        <Info className="size-3.5" aria-hidden />
      </span>
      <span
        role="tooltip"
        className="pointer-events-none absolute top-5 left-1/2 z-30 w-64 -translate-x-1/2 rounded-lg border border-line bg-surface-2 p-2.5 text-xs leading-relaxed font-normal tracking-normal text-fg normal-case opacity-0 shadow-lg transition-opacity group-focus-within:opacity-100 group-hover:opacity-100"
      >
        {text}
      </span>
    </span>
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
    neutral: "bg-surface-2 text-muted",
    ok: "bg-accent-soft text-accent",
    warn: "bg-warn/15 text-warn",
    err: "bg-danger/15 text-danger",
  }[tone];
  return (
    <span
      className={cx(
        "inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-xs font-medium",
        t,
      )}
    >
      {children}
    </span>
  );
}

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
    <label
      className={cx(
        "flex items-start justify-between gap-4 py-2",
        disabled && "opacity-50",
      )}
    >
      <span className="min-w-0">
        <span className="block text-sm">{label}</span>
        {description && (
          <span className="mt-0.5 block text-xs text-faint">{description}</span>
        )}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={cx(
          "relative mt-0.5 h-5 w-9 shrink-0 rounded-full transition-colors",
          checked ? "bg-accent" : "bg-line",
        )}
      >
        <span
          className={cx(
            "absolute top-0.5 left-0.5 size-4 rounded-full bg-white shadow transition-transform",
            checked && "translate-x-4",
          )}
        />
      </button>
    </label>
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
      <span className="mb-1.5 block text-[13px] text-muted">{label}</span>
      {children}
      {hint && <span className="mt-1.5 block text-xs text-faint">{hint}</span>}
    </label>
  );
}

export const inputClass =
  "h-10 w-full rounded-xl border border-line bg-bg px-3.5 text-sm text-fg placeholder:text-faint focus:border-accent focus:outline-none disabled:opacity-60";

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  /** `title`: accessible name when the option shows only an icon. */
  options: { value: T; label: string; icon?: ReactNode; title?: string }[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex flex-wrap rounded-xl border border-line bg-bg p-0.5"
    >
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          aria-label={o.title}
          title={o.title}
          onClick={() => onChange(o.value)}
          className={cx(
            "inline-flex h-8 items-center gap-1.5 rounded-[10px] px-3.5 text-xs font-medium transition-colors [&>svg]:size-3.5",
            value === o.value
              ? "bg-surface-2 text-fg shadow-sm"
              : "text-muted hover:text-fg",
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
    "!h-3.5 !w-5 shrink-0 rounded-[2px] shadow-[0_0_0_0.5px_rgb(0_0_0/0.25)]";
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
      className="h-1.5 w-full overflow-hidden rounded-full bg-line"
      role="progressbar"
      aria-valuenow={Math.round(value * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div
        className="h-full rounded-full bg-accent transition-[width]"
        style={{ width: `${Math.round(value * 100)}%` }}
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
    <div className="flex flex-col items-center justify-center gap-2 py-10 text-center text-sm text-faint">
      {icon}
      {children}
    </div>
  );
}

export function Notice({
  tone = "neutral",
  children,
}: {
  tone?: "neutral" | "ok" | "warn" | "err";
  children: ReactNode;
}) {
  const t = {
    neutral: "border-line bg-surface-2 text-muted",
    ok: "border-accent/30 bg-accent-soft text-fg",
    warn: "border-warn/30 bg-warn/10 text-fg",
    err: "border-danger/30 bg-danger/10 text-fg",
  }[tone];
  return (
    <div
      className={cx(
        "rounded-xl border px-4 py-3 text-[13px] leading-relaxed",
        t,
      )}
    >
      {children}
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
    <div className="flex items-baseline justify-between gap-4 border-b border-line py-2.5 text-sm last:border-0">
      <span className="text-muted">{label}</span>
      <span className="num min-w-0 truncate text-right">{children}</span>
    </div>
  );
}

/** Accessible modal (native <dialog>): Escape closes, focus stays inside. */
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
      aria-labelledby="modal-title"
      className={cx(
        "pop-in shadow-soft fixed inset-0 z-50 m-auto rounded-2xl border border-line bg-surface p-6 text-fg backdrop:bg-black/60",
        wide
          ? "w-[min(560px,calc(100vw-32px))]"
          : "w-[min(440px,calc(100vw-32px))]",
      )}
    >
      <div className="flex items-start gap-3">
        {icon && <div className="mt-0.5 shrink-0">{icon}</div>}
        <div className="min-w-0 flex-1">
          <h2 id="modal-title" className="font-display text-xl leading-snug">
            {title}
          </h2>
          {children && (
            <div className="mt-2 text-sm leading-relaxed text-muted">
              {children}
            </div>
          )}
        </div>
        <button
          type="button"
          aria-label={closeLabel}
          onClick={onClose}
          className="-mt-1 -mr-1 rounded-lg p-1 text-faint hover:bg-surface-2 hover:text-fg"
        >
          <X className="size-4" />
        </button>
      </div>
      {actions && (
        <div className="mt-6 flex flex-wrap justify-end gap-2">{actions}</div>
      )}
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

/** Range input that shows the live value and commits on release. */
export function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  unit,
  onCommit,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  unit: string;
  onCommit: (v: number) => void;
}) {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  const commit = () => v !== value && onCommit(v);
  return (
    <label className="block">
      <span className="mb-2 flex justify-between text-[13px] text-muted">
        <span>{label}</span>
        <span className="num text-fg">
          {v}
          {unit}
        </span>
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={v}
        onChange={(e) => setV(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
        className="w-full accent-[var(--accent)]"
      />
    </label>
  );
}
