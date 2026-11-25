import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { Download, RefreshCw, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, inApp } from "../lib/api";
import { useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { Button } from "./ui";

type Phase = "idle" | "downloading" | "ready" | "installing" | "failed";

const HOUR = 60 * 60 * 1000;

/** `check()` that gives up after `ms`: a slow network never holds the app at launch. */
const checkWithin = (ms: number) =>
  Promise.race([
    check(),
    new Promise<null>((r) => setTimeout(() => r(null), ms)),
  ]);

/**
 * Self-update from signed GitHub releases, done in the open:
 * - at launch, an update found right away is downloaded and applied behind a full-screen
 *   progress view, then Erebor restarts (like Discord);
 * - while running, it checks every hour, downloads quietly, then shows a card with the
 *   release notes and "Update now", plus a system notification (useful from the tray).
 * Mining interrupted by an update starts again by itself in the new version.
 */
export function Updater() {
  const t = useT();
  const mining = useApp(
    (s) =>
      s.status?.state === "running" ||
      s.status?.state === "paused" ||
      !!s.status?.held,
  );
  const [update, setUpdate] = useState<Update | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState<number | null>(null);
  const [later, setLater] = useState(false);
  const [notes, setNotes] = useState(false);
  const found = useRef(false);

  const download = async (u: Update) => {
    let total = 0;
    let got = 0;
    await u.download((ev) => {
      if (ev.event === "Started") total = ev.data.contentLength ?? 0;
      if (ev.event === "Progress") {
        got += ev.data.chunkLength;
        if (total > 0) setProgress(Math.min(1, got / total));
      }
    });
  };

  const install = async (u: Update) => {
    setPhase("installing");
    try {
      await api.prepareUpdate().catch(() => {});
      await u.install();
      await relaunch();
    } catch {
      setPhase("failed");
    }
  };

  const notify = async (version: string) => {
    try {
      let ok = await isPermissionGranted();
      if (!ok) ok = (await requestPermission()) === "granted";
      if (ok)
        sendNotification({
          title: t("update.notify.title", { v: version }),
          body: t("update.notify.body"),
        });
    } catch {
      // notifications unavailable: the card in the app is enough
    }
  };

  useEffect(() => {
    if (!inApp()) return;
    // Launch: apply right away, before the app is in use.
    void (async () => {
      try {
        const u = await checkWithin(8000);
        if (!u) return;
        found.current = true;
        setUpdate(u);
        setPhase("downloading");
        await download(u);
        await install(u);
      } catch {
        found.current = false; // offline or failed: the hourly check tries again
        setPhase("idle");
      }
    })();
    const id = setInterval(async () => {
      if (found.current) return;
      try {
        const u = await check();
        if (!u) return;
        found.current = true;
        await u.download();
        setUpdate(u);
        setPhase("ready");
        void notify(u.version);
      } catch {
        found.current = false;
      }
    }, HOUR);
    return () => clearInterval(id);
  }, []);

  if (!update || phase === "idle") return null;

  if (phase === "downloading" || phase === "installing")
    return (
      <div
        className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-6 bg-bg px-8 text-center"
        role="status"
        aria-live="polite"
      >
        <RefreshCw className="size-9 text-accent motion-safe:animate-spin" />
        <div>
          <h1 className="font-display text-2xl tracking-tight">
            {t("update.title", { v: update.version })}
          </h1>
          <p className="mt-2 text-sm text-muted">
            {phase === "downloading"
              ? t("update.downloading")
              : t("update.installing")}
          </p>
        </div>
        <div className="h-1.5 w-64 overflow-hidden rounded-full bg-surface-2">
          <div
            className="h-full w-full rounded-full bg-accent transition-transform duration-300 ease-[var(--ease)]"
            style={{
              transform: `translateX(${
                phase === "installing" ? 0 : Math.round((progress ?? 0.05) * 100) - 100
              }%)`,
            }}
          />
        </div>
        <p className="text-xs text-faint">{t("update.keepOpen")}</p>
      </div>
    );

  if (phase === "failed")
    return (
      <div
        className="flex items-center justify-between gap-3 border-b border-accent/30 bg-accent/10 px-6 py-2 text-xs text-accent"
        role="status"
      >
        <span>
          {t("update.failed")} (v{update.version})
        </span>
        <button
          className="shrink-0 rounded-lg border border-accent/40 px-2.5 py-1 font-medium hover:bg-accent/15"
          onClick={() => void install(update)}
        >
          {t("update.button")}
        </button>
      </div>
    );

  // Ready: a card with the notes, or a small pill once put off.
  if (later)
    return (
      <div className="flex justify-end px-6 pt-3">
        <button
          className="rounded-full border border-accent/30 bg-accent/10 px-3 py-1 text-2xs font-medium text-accent hover:bg-accent/15"
          onClick={() => setLater(false)}
        >
          {t("update.pill", { v: update.version })}
        </button>
      </div>
    );
  return (
    <div
      className="fixed right-5 bottom-5 z-40 w-80 rounded-2xl border border-line bg-surface p-4 shadow-xl"
      role="status"
    >
      <div className="flex items-start gap-3">
        <Download className="mt-0.5 size-5 shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium">
            {t("update.notify.title", { v: update.version })}
          </p>
          <p className="mt-1 text-xs leading-relaxed text-faint">
            {mining ? t("update.whileMining") : t("update.restart")}
          </p>
          {update.body && (
            <button
              type="button"
              className="mt-2 text-xs text-accent hover:underline"
              onClick={() => setNotes(!notes)}
            >
              {t(notes ? "update.notes.hide" : "update.notes")}
            </button>
          )}
          {notes && update.body && (
            <p className="mt-2 max-h-40 overflow-y-auto text-xs leading-relaxed whitespace-pre-wrap text-muted">
              {update.body}
            </p>
          )}
          <div className="mt-3 flex gap-2">
            <Button variant="primary" onClick={() => void install(update)}>
              {t("update.now")}
            </Button>
            <Button onClick={() => setLater(true)}>{t("update.later")}</Button>
          </div>
        </div>
        <button
          type="button"
          aria-label={t("update.later")}
          onClick={() => setLater(true)}
          className="rounded-full p-1 text-faint hover:bg-surface-2 hover:text-fg"
        >
          <X className="size-4" />
        </button>
      </div>
    </div>
  );
}
