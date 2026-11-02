// Reading paths as trees: how far each path has grown, and whether its
// "fully grown" moment has already been celebrated (once per path and vault,
// wherever it happened first: the reader or the path view).
import { app } from "./state.svelte";

export interface PathGrowth {
  /** Pages of the path opened so far (still in the vault). */
  read: number;
  /** Pages of the path still in the vault. */
  total: number;
  /** Every page has been read (an empty path never is). */
  mature: boolean;
  /** The first unread page in reading order, or null when none is left. */
  next: string | null;
}

export function pathGrowth(pathId: string): PathGrowth {
  const path = app.paths.find((p) => p.id === pathId);
  const progress = app.pathProgress[pathId];
  const read = progress?.read.length ?? 0;
  const total = progress?.total ?? path?.pages.length ?? 0;
  const done = new Set(progress?.read ?? []);
  const present = new Set(app.library.map((p) => p.id));
  const next = path?.pages.find((id) => !done.has(id) && present.has(id)) ?? null;
  return { read, total, mature: total > 0 && read >= total, next };
}

/**
 * A readable, unique id for a new reading path, exactly as the backend builds
 * it (`builtin/paths.rs::new_id`): lowercase ASCII words joined by `-`, at most
 * 48 characters, then `-2`, `-3`, … until it is free.
 */
export function pathSlug(name: string, takenIds: readonly string[] = []): string {
  let base = "";
  for (const c of name) {
    const code = c.codePointAt(0)!;
    if ((code >= 48 && code <= 57) || (code >= 65 && code <= 90) || (code >= 97 && code <= 122)) {
      base += c.toLowerCase();
    } else if (base.length > 0 && !base.endsWith("-")) {
      base += "-";
    }
  }
  // The backend trims before truncating, so a 48th character may be a dash.
  const slug = base.replace(/-+$/, "").slice(0, 48) || "path";
  const taken = new Set(takenIds);
  if (!taken.has(slug)) return slug;
  let id = slug;
  let n = 2;
  while (taken.has(id)) {
    id = `${slug}-${n}`;
    n += 1;
  }
  return id;
}

function key(pathId: string): string {
  return `herbarium.grown:${app.config?.vaultPath ?? ""}:${pathId}`;
}

export function wasCelebrated(pathId: string): boolean {
  try {
    return localStorage.getItem(key(pathId)) === "1";
  } catch {
    return false;
  }
}

export function markCelebrated(pathId: string): void {
  try {
    localStorage.setItem(key(pathId), "1");
  } catch {
    // Storage refused: the moment may play again, which is harmless.
  }
}
