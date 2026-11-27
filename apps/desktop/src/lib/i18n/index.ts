import { useApp } from "../store";
import { en } from "./en";
import { it, type Key } from "./it";

export type { Key };

export const dictionaries = { it, en };

export function translate(lang: "it" | "en", key: string, vars?: Record<string, string | number>) {
  const dict = dictionaries[lang] as Record<string, string>;
  let s = dict[key] ?? (it as Record<string, string>)[key] ?? key;
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.replaceAll(`{${k}}`, String(v));
  return s;
}

/** Translate an error code from the backend; unknown messages are shown verbatim. */
export function translateError(lang: "it" | "en", code: string) {
  const key = `err.${code}`;
  const s = translate(lang, key);
  return s === key ? code : s;
}

/** Number and date formatting for each UI language. */
export const LOCALES = { it: "it-IT", en: "en-US" } as const;

export function useLocale() {
  return useApp((s) => LOCALES[s.config?.language ?? "en"] ?? "en-US");
}

export function useT() {
  const lang = useApp((s) => s.config?.language ?? "en");
  return (key: Key | string, vars?: Record<string, string | number>) => translate(lang, key, vars);
}
