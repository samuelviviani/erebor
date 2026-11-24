import { Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorText, type CoinOption, type Preset } from "../lib/api";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { CoinIcon } from "./CoinIcon";
import { ConfirmDialog } from "./ui";

/** Coin options, refreshed while mounted (measured speeds and estimates change). */
export function useCoinOptions() {
  const config = useApp((s) => s.config);
  const [options, setOptions] = useState<CoinOption[]>([]);
  useEffect(() => {
    const load = () =>
      api
        .coinOptions()
        .then(setOptions)
        .catch(() => {});
    load();
    const id = setInterval(load, 10_000);
    return () => clearInterval(id);
  }, [
    config?.wallet,
    config?.srbminer.path,
    config?.lolminer.path,
    config?.cpuminer.path,
    config?.miner.path,
  ]);
  return options;
}

/**
 * Changing coin restarts the miner: always ask first, in plain words.
 * Returns `ask(id)` and the dialog element to render.
 */
export function useCoinChange() {
  const t = useT();
  const { config, saveConfig, toast, setCoinBusy } = useApp();
  const [pending, setPending] = useState<{ id: string; name: string } | null>(
    null,
  );
  const lang = config?.language ?? "en";

  const ask = (id: string, name: string) => {
    if (!config || id === config.coin) return;
    setPending({ id, name });
  };
  const confirm = async () => {
    const p = pending;
    setPending(null);
    if (!p || !config) return;
    setCoinBusy(p.name);
    try {
      await saveConfig({ ...config, coin: p.id });
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    } finally {
      setCoinBusy(null);
    }
  };
  const auto = pending?.id === "AUTO";
  const dialog = (
    <ConfirmDialog
      open={pending != null}
      icon={
        auto ? (
          <span className="flex size-10 items-center justify-center rounded-2xl bg-accent-soft text-accent">
            <Sparkles className="size-5" />
          </span>
        ) : pending ? (
          <CoinIcon id={pending.id} size={40} />
        ) : undefined
      }
      title={
        auto
          ? t("confirm.auto.title")
          : t("confirm.coin.title", { c: pending?.name ?? "" })
      }
      text={
        auto
          ? t("confirm.auto.body")
          : t("confirm.coin.body", { c: pending?.name ?? "" })
      }
      confirmLabel={auto ? t("confirm.auto.ok") : t("confirm.coin.ok")}
      cancelLabel={t("confirm.cancel")}
      onConfirm={confirm}
      onCancel={() => setPending(null)}
    />
  );
  return { ask, dialog };
}

/**
 * Performance mode: applied at once, also while mining (the miner restarts and the
 * status bar says so). Returns the setter to pass to the selector.
 */
export function usePresetChange() {
  const { config, saveConfig, toast } = useApp();
  const lang = config?.language ?? "en";
  return async (preset: Preset) => {
    if (!config || preset === config.performance.preset) return;
    try {
      await saveConfig({
        ...config,
        performance: { ...config.performance, preset },
      });
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    }
  };
}
