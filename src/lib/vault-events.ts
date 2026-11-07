// Pure helpers for the backend's `vault-event` push (desktop/src/events.rs):
// fold a batch of page events into a list without copying it per event.

import type { IndexReport, PageMeta, TagCount } from "./types";

/** What the backend pushes after an operation changed the vault. */
export interface VaultEvent {
  /** Core event name: `page.created`, `page.updated`, `page.deleted`, `vault.indexed`, `proposal.created`, `proposal.resolved`. */
  kind: string;
  /** The page (or proposal) the event is about. */
  id?: string | null;
  /** The page's metadata after the change; present on page events. */
  meta?: PageMeta | null;
  /** The re-index outcome; present on `vault.indexed`. */
  report?: IndexReport | null;
}

export type PageEvent = VaultEvent & { kind: "page.created" | "page.updated" | "page.deleted"; meta: PageMeta };

export function isPageEvent(event: VaultEvent): event is PageEvent {
  return (
    !!event.meta &&
    (event.kind === "page.created" || event.kind === "page.updated" || event.kind === "page.deleted")
  );
}

/**
 * Fold `events` (oldest first) into `list`: the last event per page wins. A
 * changed page replaces its entry, a deleted one leaves, and, when `insert`
 * is set, a page the list lacks is appended. Returns `list` itself when
 * nothing changed, so callers can skip the assignment.
 */
export function applyPageEvents(list: PageMeta[], events: PageEvent[], insert: boolean): PageMeta[] {
  if (events.length === 0) return list;
  const last = new Map<string, PageEvent>();
  for (const event of events) last.set(event.meta.id, event);
  let changed = false;
  const next: PageMeta[] = [];
  for (const page of list) {
    const event = last.get(page.id);
    if (!event) {
      next.push(page);
      continue;
    }
    last.delete(page.id);
    if (event.kind === "page.deleted") {
      changed = true;
    } else {
      next.push(event.meta);
      if (event.meta !== page) changed = true;
    }
  }
  if (insert) {
    for (const event of last.values()) {
      if (event.kind === "page.deleted") continue;
      next.push(event.meta);
      changed = true;
    }
  }
  return changed ? next : list;
}

/** Tag usage across `pages`, sorted like the backend (`tags.list`). */
export function tagCounts(pages: PageMeta[]): TagCount[] {
  const counts = new Map<string, number>();
  for (const page of pages) {
    for (const tag of page.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
  }
  return [...counts]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([tag, count]) => ({ tag, count }));
}

/** True when the change can move the due count: the page is new, gone, or its
 *  schedule, folder or tags (review settings can exclude either) differ. */
export function affectsReviewQueue(before: PageMeta | undefined, event: PageEvent): boolean {
  if (event.kind !== "page.updated" || !before) return true;
  const after = event.meta;
  return (
    before.nextReview !== after.nextReview ||
    before.folder !== after.folder ||
    before.tags.length !== after.tags.length ||
    before.tags.some((tag, i) => tag !== after.tags[i])
  );
}

/** True when a changed page sits in a folder the folder list does not have yet. */
export function introducesFolder(folders: string[], event: PageEvent): boolean {
  return event.kind !== "page.deleted" && !!event.meta.folder && !folders.includes(event.meta.folder);
}
