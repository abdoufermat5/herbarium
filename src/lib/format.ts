// Small formatting helpers shared across views.

export const isMac =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** "⌘K" on macOS, "Ctrl K" elsewhere. */
export function modKey(key = "K"): string {
  return isMac ? `⌘${key}` : `Ctrl ${key}`;
}

export function fmtDate(ms: number | null | undefined): string {
  if (!ms) return "";
  return new Date(ms).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

/** "Sep 14" — no year, for compact badges. */
export function fmtDay(ms: number | null | undefined): string {
  if (!ms) return "";
  return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function timeAgo(ms: number | null | undefined): string {
  if (!ms) return "";
  const diff = Date.now() - ms;
  if (diff < 60_000) return "just now";
  const mins = Math.floor(diff / 60_000);
  if (mins < 60) return `${mins} min ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days} day${days === 1 ? "" : "s"} ago`;
  return fmtDate(ms);
}

export function plural(n: number, word: string, pluralWord?: string): string {
  return `${n} ${n === 1 ? word : (pluralWord ?? word + "s")}`;
}

export function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export interface DueInfo {
  /** Human label, e.g. "Due today", "2 days overdue", "Due in 5 days". */
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
      label: overdueDays >= 1 ? `${plural(overdueDays, "day")} overdue` : "Due today",
      hot: true,
      overdue: true,
      days: 0,
    };
  }
  const days = Math.ceil(diff / 86_400_000);
  return {
    label: days === 1 ? "Due tomorrow" : `Due in ${plural(days, "day")}`,
    hot: false,
    overdue: false,
    days,
  };
}
