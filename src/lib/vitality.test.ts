import { describe, expect, it } from "vitest";
import { retrievability, vitality } from "./vitality";

const DAY = 86_400_000;
const NOW = Date.UTC(2026, 10, 2, 12);

describe("retrievability", () => {
  it("starts at full recall and reaches 90% after one stability", () => {
    expect(retrievability(0, 5)).toBe(1);
    expect(retrievability(5, 5)).toBeCloseTo(0.9, 9);
    expect(retrievability(20, 5)).toBeLessThan(0.9);
  });
});

describe("vitality", () => {
  it("treats a page never reviewed nor scheduled as a seed", () => {
    expect(vitality({ lastReview: null, nextReview: null }, NOW)).toEqual({ level: "seed", recall: null });
  });

  it("moves from thriving to healthy to thirsty to wilting as FSRS recall decays", () => {
    const meta = (elapsedDays: number) => ({
      lastReview: NOW - elapsedDays * DAY,
      nextReview: NOW - elapsedDays * DAY + 10 * DAY,
      ext: { review: { count: 3, log: [], stability: 10 } },
    });
    expect(vitality(meta(1), NOW).level).toBe("thriving");
    expect(vitality(meta(8), NOW).level).toBe("healthy");
    expect(vitality(meta(11), NOW).level).toBe("thirsty");
    expect(vitality(meta(120), NOW).level).toBe("wilting");
  });

  it("uses the current interval as stability when FSRS has not run", () => {
    const due = { lastReview: NOW - 4 * DAY, nextReview: NOW - DAY };
    const v = vitality(due, NOW);
    expect(v.level).toBe("thirsty");
    expect(v.recall).toBeCloseTo(retrievability(4, 3), 9);
  });

  it("judges a scheduled page with no review yet by how overdue it is", () => {
    expect(vitality({ lastReview: null, nextReview: NOW + DAY }, NOW).level).toBe("thriving");
    expect(vitality({ lastReview: null, nextReview: NOW - DAY }, NOW).level).toBe("thirsty");
    expect(vitality({ lastReview: null, nextReview: NOW - 5 * DAY }, NOW).level).toBe("wilting");
  });
});
