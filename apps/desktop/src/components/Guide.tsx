import { Activity, Bitcoin, CircleHelp, Power, Sparkles, X, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { Logo } from "./Layout";
import { Button, cx } from "./ui";

/** Each step: a picture (icons on a soft gradient) and two lines of text. */
const STEPS: LucideIcon[][] = [[], [Sparkles, Bitcoin], [Activity, Power, CircleHelp]];
const SEEN = "erebor.guideSeen";

/** Beginner tour: shown once, on the first launch after setup. */
export function Guide() {
  const t = useT();
  const { guide, setGuide } = useApp();
  const [i, setI] = useState(0);
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    try {
      if (!localStorage.getItem(SEEN)) setGuide(true);
    } catch {
      /* storage unavailable: no tour rather than a tour on every launch */
    }
  }, [setGuide]);

  useEffect(() => {
    const d = ref.current;
    if (!d || !guide || d.open) return;
    if (typeof d.showModal === "function") d.showModal();
    else d.setAttribute("open", "");
    d.focus(); // the sheet, not the close button: no focus ring at open
  }, [guide]);

  const close = () => {
    setGuide(false);
    setI(0);
    try {
      localStorage.setItem(SEEN, "1");
    } catch {
      /* ignore */
    }
  };
  if (!guide) return null;
  const last = i === STEPS.length - 1;
  const icons = STEPS[i];

  return (
    <dialog
      ref={ref}
      onCancel={(e) => {
        e.preventDefault();
        close();
      }}
      aria-labelledby="guide-title"
      tabIndex={-1}
      className="pop-in shadow-soft outline-none fixed inset-0 z-50 m-auto w-[min(480px,calc(100vw-32px))] overflow-hidden rounded-3xl border border-line bg-surface p-0 text-fg backdrop:bg-black/60"
    >
      <div className="relative flex h-48 items-center justify-center overflow-hidden bg-gradient-to-br from-accent/25 via-accent/5 to-transparent">
        <div aria-hidden className="absolute -right-16 -bottom-20 size-64 rounded-full border border-accent/15" />
        <div aria-hidden className="absolute -right-6 -bottom-10 size-40 rounded-full border border-accent/20" />
        <button type="button" aria-label={t("close")} onClick={close} className="absolute top-3 right-3 rounded-full p-1.5 text-faint hover:bg-surface-2 hover:text-fg">
          <X className="size-4" />
        </button>
        <div key={i} className="page-in flex items-center gap-4">
          {icons.length === 0 ? (
            <Logo className="h-20 w-auto drop-shadow-lg" />
          ) : (
            icons.map((Icon, k) => (
              <span key={k} className={cx("flex items-center justify-center rounded-2xl border border-line bg-surface shadow-soft", k === 0 ? "size-16 text-accent" : "size-12 text-muted")}>
                <Icon className={k === 0 ? "size-7" : "size-5"} />
              </span>
            ))
          )}
        </div>
      </div>

      <div key={`t${i}`} className="page-in px-7 pt-6">
        <h2 id="guide-title" className="font-display text-2xl tracking-tight">
          {t(`tour.${i + 1}.title`)}
        </h2>
        <p className="mt-2 text-base leading-relaxed text-muted">{t(`tour.${i + 1}.body`)}</p>
      </div>

      <div className="flex items-center gap-3 px-7 pt-6 pb-6">
        <div className="flex gap-1.5" aria-label={t("tour.step", { n: i + 1, t: STEPS.length })}>
          {STEPS.map((_, k) => (
            <span key={k} className={cx("h-1.5 rounded-full transition-[width,background-color] duration-300 ease-[var(--ease)]", k === i ? "w-6 bg-accent" : "w-1.5 bg-line")} />
          ))}
        </div>
        <div className="ml-auto flex gap-2">
          {i === 0 ? (
            <Button variant="ghost" onClick={close}>
              {t("tour.skip")}
            </Button>
          ) : (
            <Button variant="ghost" onClick={() => setI(i - 1)}>
              {t("tour.back")}
            </Button>
          )}
          <Button variant="primary" className="px-5" onClick={() => (last ? close() : setI(i + 1))}>
            {last ? t("tour.done") : t("tour.next")}
          </Button>
        </div>
      </div>
    </dialog>
  );
}
