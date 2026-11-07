import { en, type MessageKey, type PluralKey } from "./locales/en";

export type Locale = "en" | "fr";

export const LOCALES: Record<Locale, { name: string; short: string; bcp47: string }> = {
  en: { name: "English", short: "EN", bcp47: "en-US" },
  fr: { name: "Français", short: "FR", bcp47: "fr-FR" },
};

/** English ships in the main bundle (it is also the fallback for missing keys);
 *  other catalogs load on first use. */
const CATALOGS: Partial<Record<Locale, Record<string, string>>> = { en };
const LOADERS: Record<Locale, () => Promise<Record<string, string>>> = {
  en: async () => en,
  fr: () => import("./locales/fr").then((m) => m.fr),
};

/** Make sure a locale's catalog is in memory. */
export async function loadCatalog(locale: Locale): Promise<void> {
  CATALOGS[locale] ??= await LOADERS[locale]();
}
const STORAGE_KEY = "herbarium.locale";

/** The OS language when it is one we ship, else English. */
export function systemLocale(languages: readonly string[] = navigator.languages ?? [navigator.language]): Locale {
  for (const tag of languages) {
    const base = tag?.toLowerCase().split("-")[0];
    if (base === "fr" || base === "en") return base;
  }
  return "en";
}

/** The user's explicit choice, else the OS language. */
function storedLocale(): Locale {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "fr" || v === "en") return v;
  } catch {
    /* fall through to the OS language */
  }
  return systemLocale();
}

export const i18n = $state<{ locale: Locale }>({ locale: storedLocale() });

export type Params = Record<string, string | number>;

/** One `PluralRules` per locale; plural keys run per row in long lists. */
const pluralRules = new Map<Locale, Intl.PluralRules>();

function pluralForm(locale: Locale, count: number): Intl.LDMLPluralRule {
  let rules = pluralRules.get(locale);
  if (!rules) {
    rules = new Intl.PluralRules(LOCALES[locale].bcp47);
    pluralRules.set(locale, rules);
  }
  return rules.select(count);
}

/**
 * Translate a key. Reads `i18n.locale`, so calls inside templates and
 * `$derived` re-run when the language changes. When `params.count` is a
 * number, the `_one` / `_other` variant of the key is used.
 */
export function t(key: MessageKey | PluralKey, params?: Params): string {
  const catalog = CATALOGS[i18n.locale] ?? (en as Record<string, string>);
  let msg: string | undefined;
  if (typeof params?.count === "number") {
    const form = pluralForm(i18n.locale, params.count);
    msg = catalog[`${key}_${form}`] ?? catalog[`${key}_other`];
  }
  msg ??= catalog[key] ?? (en as Record<string, string>)[key];
  if (!params) return msg;
  return msg.replace(/\{(\w+)\}/g, (_, name) => String(params[name] ?? `{${name}}`));
}

/** Switch language. The new catalog loads first, so the interface never shows
 *  raw keys between the choice and the switch. */
export async function setLocale(locale: Locale): Promise<void> {
  await loadCatalog(locale);
  i18n.locale = locale;
  document.documentElement.lang = locale;
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    /* preference just won't persist */
  }
}

export function otherLocale(): Locale {
  return i18n.locale === "en" ? "fr" : "en";
}

export function toggleLocale(): Promise<void> {
  return setLocale(otherLocale());
}

/** Load the starting locale's catalog; call before mounting the UI. */
export async function initLocale(): Promise<void> {
  try {
    await loadCatalog(i18n.locale);
  } catch (e) {
    console.error("Locale failed to load; using English", e);
  }
  document.documentElement.lang = i18n.locale;
}

export type { MessageKey };
