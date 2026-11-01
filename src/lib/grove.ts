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
