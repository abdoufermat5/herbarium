// Page vitality: how fresh a page is in memory, drawn as the health of a plant.
// Uses the FSRS-6 forgetting curve the backend schedules with
// (desktop/crates/herbarium-core/src/builtin/fsrs.rs); pages scheduled without
// FSRS fall back to treating their current interval as stability, which is
// exact for FSRS at the default 0.9 retention and a fair guess otherwise.
import type { PageMeta } from "./types";

/** `seed`: never reviewed nor scheduled. The rest follow recall probability. */
export type VitalityLevel = "seed" | "thriving" | "healthy" | "thirsty" | "wilting";

export interface Vitality {
  level: VitalityLevel;
  /** Estimated probability the page is still recalled now (0..1), when it can be estimated. */
  recall: number | null;
}

const DAY_MS = 86_400_000;
/** FSRS-6 `w20`; the curve's decay is its negative. */
const DECAY = -0.1542;
const FACTOR = Math.pow(0.9, 1 / DECAY) - 1;

/** FSRS power forgetting curve `R(t, S)`, `t` and `S` in days. */
export function retrievability(elapsedDays: number, stabilityDays: number): number {
  if (stabilityDays <= 0) return 0;
  return Math.pow((Math.max(0, elapsedDays) / stabilityDays) * FACTOR + 1, DECAY);
}

/** Recall above this before the due date reads as thriving rather than healthy. */
const THRIVING = 0.95;
/** Once due, recall above this is thirsty; below it the page is wilting. */
const WILTING = 0.75;
/** Without a memory estimate, a page this long overdue counts as wilting. */
const WILTING_OVERDUE_MS = 3 * DAY_MS;

export function vitality(meta: Pick<PageMeta, "lastReview" | "nextReview" | "ext">, now = Date.now()): Vitality {
  const { lastReview, nextReview } = meta;
  if (lastReview == null && nextReview == null) return { level: "seed", recall: null };

  let stability = meta.ext?.review?.stability ?? null;
  if ((stability == null || stability <= 0) && lastReview != null && nextReview != null && nextReview > lastReview) {
    stability = (nextReview - lastReview) / DAY_MS;
  }
  const recall =
    lastReview != null && stability != null && stability > 0
      ? retrievability((now - lastReview) / DAY_MS, stability)
      : null;

  const due = nextReview != null && nextReview <= now;
  if (!due) {
    if (nextReview == null && recall == null) return { level: "healthy", recall };
    return { level: recall == null || recall >= THRIVING ? "thriving" : "healthy", recall };
  }
  if (recall != null) return { level: recall >= WILTING ? "thirsty" : "wilting", recall };
  return { level: now - nextReview! >= WILTING_OVERDUE_MS ? "wilting" : "thirsty", recall };
}
