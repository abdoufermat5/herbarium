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

/** "3 pages" / "3 pages" in the active language. `word` is a `unit.*` key. */
export function plural(n: number, word: "page" | "day" | "file"): string {
  return t(`unit.${word}`, { count: n });
}

export function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export interface DueInfo {
  /** Human label, e.g. "Due today", "2 days overdue", "Due in 5 days" (localized). */
  label: string;
  /** True when the page is due now or overdue. */
  hot: boolean;
  overdue: boolean;
  days: number;
}

export function dueInfo(next: number | null | undefined): DueInfo | null {
  if (next == null) return null;
  const diff = next - Date.now();
  if (diff <= 0) {
    const overdueDays = Math.floor((Date.now() - next) / 86_400_000);
    return {
      label:
        overdueDays >= 1
          ? t("due.overdue", { days: plural(overdueDays, "day") })
          : t("due.today"),
      hot: true,
      overdue: true,
      days: 0,
    };
  }
  const days = Math.ceil(diff / 86_400_000);
  return {
    label: days === 1 ? t("due.tomorrow") : t("due.inDays", { days: plural(days, "day") }),
    hot: false,
    overdue: false,
    days,
  };
}
