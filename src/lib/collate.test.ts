import { describe, expect, it } from "vitest";
import { compareTitles, topN } from "./collate";

describe("compareTitles", () => {
  it("ignores case and accents and orders digit runs numerically", () => {
    expect(compareTitles("apple", "Banana")).toBeLessThan(0);
    expect(compareTitles("école", "Ecole")).toBe(0);
    expect(compareTitles("Page 2", "page 10")).toBeLessThan(0);
  });
});

describe("topN", () => {
  const num = (a: number, b: number) => a - b;
  it("matches a full sort's prefix", () => {
    const data = [9, 3, 7, 1, 8, 2, 2, 6, 5, 4, 0];
    for (const n of [0, 1, 3, 8, 20]) {
      expect(topN(data, n, num)).toEqual([...data].sort(num).slice(0, n));
    }
  });
  it("does not mutate its input", () => {
    const data = [3, 1, 2];
    topN(data, 2, num);
    expect(data).toEqual([3, 1, 2]);
  });
});
