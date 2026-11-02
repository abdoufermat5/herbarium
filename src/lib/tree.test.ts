import { describe, expect, it } from "vitest";
import { growTree, isGrown, treeProgress, treeStage, type TreeDetail } from "./tree";

const TOTALS = [1, 2, 3, 7, 40, 500];
/** Grown and never hidden again: the sprout parts fall away, so they are not additive. */
const persistent = (seed: string, total: number, read: number, detail: TreeDetail = "full") => {
  const p = treeProgress(read, total);
  return new Set(growTree(seed, total, detail).elements.filter((e) => e.hideAt === undefined && isGrown(e, p)).map((e) => e.i));
};

describe("growTree", () => {
  it("is deterministic per seed and differs between seeds", () => {
    const a = growTree("path-a", 7, "full");
    expect(JSON.parse(JSON.stringify(growTree("path-a", 7, "full")))).toEqual(JSON.parse(JSON.stringify(a)));
    const shapes = new Set(["path-a", "path-b", "path-c", "path-d", "path-e"].map((s) => growTree(s, 7).elements.map((e) => e.d).join("|")));
    expect(shapes.size).toBe(5);
  });

  it("is additive apart from the sprout parts, which hide at hideAt", () => {
    for (const detail of ["full", "mini"] as const) {
      for (const total of [3, 7, 40]) {
        let prev = persistent("additive", total, 0, detail);
        for (let read = 1; read <= total; read++) {
          const next = persistent("additive", total, read, detail);
          for (const i of prev) expect(next.has(i)).toBe(true);
          prev = next;
        }
      }
    }
  });

  it("stays within the element budgets", () => {
    for (const total of TOTALS) {
      expect(growTree("budget", total, "full").elements.length).toBeLessThanOrEqual(400);
      expect(growTree("budget", total, "mini").elements.length).toBeLessThanOrEqual(24);
    }
  });

  it("gives every page something to grow when total <= 40", () => {
    for (const seed of ["a", "b", "herbarium", "x-17"]) {
      for (const total of [1, 2, 3, 5, 7, 12, 25, 40]) {
        const owned = new Set(growTree(seed, total, "full").elements.map((e) => e.page));
        for (let i = 0; i < total; i++) expect(owned.has(i)).toBe(true);
      }
    }
  });

  it("starts as a seed with a sprout and ends mature with blossoms", () => {
    const m = growTree("life", 5, "full");
    const at0 = m.elements.filter((e) => isGrown(e, 0));
    expect(at0.some((e) => e.kind === "seed")).toBe(true);
    expect(at0.some((e) => e.kind === "branch")).toBe(true);
    expect(at0.some((e) => e.kind === "petals" || e.kind === "sun")).toBe(false);
    // The sprout parts are the only things that hide again: everything else is
    // grown at full progress, and the seed and seed leaves are gone by then.
    const sprouts = m.elements.filter((e) => e.hideAt !== undefined);
    expect(sprouts.length).toBeGreaterThan(0);
    for (const e of m.elements) expect(isGrown(e, 1)).toBe(e.hideAt === undefined);
  });

  it("never leaves a visible branch bare at 20% progress", () => {
    for (const seed of ["life", "Networking basics", "Rust from the ground up", "x-17"]) {
      for (const total of [3, 5, 8, 12, 40]) {
        const m = growTree(seed, total, "full");
        const vis = m.elements.filter((e) => isGrown(e, 0.2));
        const limbsWithLeaves = new Set(vis.filter((e) => e.kind === "leaf").map((e) => e.limb));
        for (const e of vis) if (e.kind === "branch" && e.depth >= 2) expect(limbsWithLeaves.has(e.limb)).toBe(true);
      }
    }
  });

  it("holds blossoms and the sun back until maturity", () => {
    for (const detail of ["full", "mini"] as const) {
      const late = growTree("bloom", 9, detail).elements.filter((e) => ["petals", "heart", "sun"].includes(e.kind));
      expect(late.length).toBeGreaterThan(0);
      for (const e of late) expect(e.threshold).toBe(1);
      expect(isGrown(late[0], 8 / 9)).toBe(false);
    }
  });

  it("memoizes", () => {
    expect(growTree("memo", 12, "full")).toBe(growTree("memo", 12, "full"));
  });

  it("generates within the time budget", () => {
    let t0 = performance.now();
    growTree("timing-cold", 500, "full");
    expect(performance.now() - t0).toBeLessThan(15);
    t0 = performance.now();
    growTree("timing-cold", 500, "full");
    expect(performance.now() - t0).toBeLessThan(0.2);
  });
});

describe("treeStage", () => {
  it("maps progress to stages", () => {
    expect(treeStage(0, 0)).toBe("seed");
    expect(treeStage(0, 8)).toBe("seed");
    expect(treeStage(1, 8)).toBe("sprout");
    expect(treeStage(1, 4)).toBe("sapling"); // exactly 0.25
    expect(treeStage(2, 4)).toBe("sapling");
    expect(treeStage(3, 5)).toBe("young"); // exactly 0.6
    expect(treeStage(4, 5)).toBe("young");
    expect(treeStage(5, 5)).toBe("mature");
    expect(treeStage(9, 5)).toBe("mature");
  });
});
