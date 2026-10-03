// Desktop notifications for pages that have just become due for review.
// `notifyDue` is called with the current review queue after every refresh; a
// page is announced once per scheduled time, so rescheduling it (a new
// `nextReview`) makes it eligible again.

import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { t } from "./i18n.svelte";
import { prefs } from "./prefs.svelte";
import type { PageMeta } from "./types";

/** Most page titles listed in one notification body. */
const MAX_TITLES = 3;

/** Pages already announced (or already due when they were first seen). */
const announced = new Set<string>();

const keyOf = (p: PageMeta) => `${p.id}@${p.nextReview}`;

/** The pages in `due` not announced before. `due` becomes the new announced set,
 *  so a page that leaves the queue and comes back is announced again. */
export function takeFresh(due: PageMeta[]): PageMeta[] {
  const fresh = due.filter((p) => !announced.has(keyOf(p)));
  announced.clear();
  for (const p of due) announced.add(keyOf(p));
  return fresh;
}

export function describe(fresh: PageMeta[]): { title: string; body: string } {
  const titles = fresh.slice(0, MAX_TITLES).map((p) => p.title || t("common.untitled"));
  const extra = fresh.length - titles.length;
  if (extra > 0) titles.push(t("notify.more", { count: extra }));
  return { title: t("sidebar.reviewDue", { count: fresh.length }), body: titles.join("\n") };
}

let permission: Promise<boolean> | null = null;

/** Ask the OS once per session; a refusal is remembered until the app restarts. */
function allowed(): Promise<boolean> {
  permission ??= isPermissionGranted()
    .then((ok) => ok || requestPermission().then((r) => r === "granted"))
    .catch(() => false);
  return permission;
}

/** Announce pages of `due` that were not announced before. Pages found due on
 *  the first call (already overdue at launch) are announced together. */
export async function notifyDue(due: PageMeta[]): Promise<void> {
  const fresh = takeFresh(due);
  if (fresh.length === 0 || !prefs.reviewNotify) return;
  try {
    if (await allowed()) sendNotification(describe(fresh));
  } catch (e) {
    console.error(e);
  }
}
