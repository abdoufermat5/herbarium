// Small formatting helpers shared across views.

import { i18n, t, LOCALES } from "./i18n.svelte";

const isMac =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** "⌘K" on macOS, "Ctrl K" elsewhere. */
export function modKey(key = "K"): string {
  return isMac ? `⌘${key}` : `Ctrl ${key}`;
}

export function fmtDate(ms: number | null | undefined): string {
  if (!ms) return "";
  return new Date(ms).toLocaleDateString(LOCALES[i18n.locale].bcp47, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

/** Date with the time of day, for review moments that can be minutes apart. */
export function fmtDateTime(ms: number | null | undefined): string {
  if (!ms) return "";
  return new Date(ms).toLocaleString(LOCALES[i18n.locale].bcp47, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

export function timeAgo(ms: number | null | undefined): string {
  if (!ms) return "";
  const diff = Date.now() - ms;
  if (diff < 60_000) return t("time.justNow");
  const mins = Math.floor(diff / 60_000);
  if (mins < 60) return t("time.minAgo", { n: mins });
  const hours = Math.floor(mins / 60);
  if (hours < 24) return t("time.hoursAgo", { n: hours });
  const days = Math.floor(hours / 24);
  if (days < 7) return t("time.daysAgo", { count: days });
  return fmtDate(ms);
}

/** "3 pages" in the active language. `word` is a `unit.*` key. */
export function plural(n: number, word: "page" | "file" | "minute" | "hour" | "day"): string {
  return t(`unit.${word}`, { count: n });
}

export function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/* ---------------------------------------------------------------- durations
 * Review intervals are stored in minutes and shown in whatever unit divides
 * them evenly, so 90 is "90 minutes", 120 is "2 hours" and 4320 is "3 days". */

export type DurationUnit = "minute" | "hour" | "day";
export const DURATION_UNITS: DurationUnit[] = ["minute", "hour", "day"];
export const UNIT_MINUTES: Record<DurationUnit, number> = { minute: 1, hour: 60, day: 1440 };

/** The largest unit that divides `minutes` evenly. */
export function splitDuration(minutes: number): { n: number; unit: DurationUnit } {
  if (minutes % UNIT_MINUTES.day === 0) return { n: minutes / UNIT_MINUTES.day, unit: "day" };
  if (minutes % UNIT_MINUTES.hour === 0) return { n: minutes / UNIT_MINUTES.hour, unit: "hour" };
  return { n: minutes, unit: "minute" };
}

/** What to display for a duration: the exact unit when one divides it, and
 * otherwise (intervals computed by adaptive review, e.g. 1862 minutes) whole
 * days from one day up. Editing still uses `splitDuration`, which never rounds. */
function displayDuration(minutes: number): { n: number; unit: DurationUnit } {
  const exact = splitDuration(minutes);
  if (exact.unit !== "minute" || minutes < UNIT_MINUTES.day) return exact;
  return { n: Math.round(minutes / UNIT_MINUTES.day), unit: "day" };
}

/** "30 minutes", "2 hours", "3 days". */
export function fmtDuration(minutes: number): string {
  const { n, unit } = displayDuration(minutes);
  return plural(n, unit);
}

/** Compact form for buttons: "30m", "2h", "3d". */
export function fmtDurationShort(minutes: number): string {
  const { n, unit } = displayDuration(minutes);
  return t(`duration.short.${unit}`, { n });
}

export interface DueInfo {
  /** Human label, e.g. "Due now", "2 days overdue", "Due in 45 minutes" (localized). */
  label: string;
  /** True when the page is due now or overdue. */
  hot: boolean;
  overdue: boolean;
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export function dueInfo(next: number | null | undefined): DueInfo | null {
  if (next == null) return null;
  const diff = next - Date.now();
  if (diff <= 0) {
    const late = -diff;
    const label =
      late >= DAY
        ? t("due.overdue", { days: plural(Math.floor(late / DAY), "day") })
        : late >= HOUR
          ? t("due.today")
          : t("due.now");
    return { label, hot: true, overdue: true };
  }
  const minutes = Math.ceil(diff / MINUTE);
  if (minutes < 60) return { label: t("due.in", { when: plural(minutes, "minute") }), hot: false, overdue: false };
  if (minutes < 24 * 60) {
    return { label: t("due.in", { when: plural(Math.ceil(minutes / 60), "hour") }), hot: false, overdue: false };
  }
  const days = Math.ceil(minutes / (24 * 60));
  return {
    label: days === 1 ? t("due.tomorrow") : t("due.in", { when: plural(days, "day") }),
    hot: false,
    overdue: false,
  };
}
