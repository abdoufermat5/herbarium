import { describe, expect, it } from "vitest";
import { buildOffsets, rowAt, rowRange, stepIndex } from "./windowing";

describe("row offsets", () => {
  it("sums measured heights, estimates the rest and adds the gap after each row", () => {
    const o = buildOffsets(4, [100, 0, 50], 80, 10);
    expect(Array.from(o)).toEqual([0, 110, 200, 260, 350]);
  });

  it("finds the row at a position and clamps outside the list", () => {
    const o = buildOffsets(4, [100, 100, 100, 100], 0, 0);
    expect(rowAt(o, -5)).toBe(0);
    expect(rowAt(o, 0)).toBe(0);
    expect(rowAt(o, 99)).toBe(0);
    expect(rowAt(o, 100)).toBe(1);
    expect(rowAt(o, 9999)).toBe(3);
    expect(rowAt(buildOffsets(0, [], 10, 0), 50)).toBe(0);
  });

  it("returns the rows a band touches", () => {
    const o = buildOffsets(10, [], 100, 0);
    expect(rowRange(o, 250, 520)).toEqual({ first: 2, last: 5 });
  });
});

describe("stepIndex", () => {
  it("moves along rows and columns", () => {
    expect(stepIndex(4, "right", 3, 10)).toBe(5);
    expect(stepIndex(4, "left", 3, 10)).toBe(3);
    expect(stepIndex(4, "down", 3, 10)).toBe(7);
    expect(stepIndex(4, "up", 3, 10)).toBe(1);
  });
  it("stops at the edges and lands on a short last row's end", () => {
    expect(stepIndex(0, "left", 3, 10)).toBeNull();
    expect(stepIndex(9, "right", 3, 10)).toBeNull();
    expect(stepIndex(1, "up", 3, 10)).toBeNull();
    expect(stepIndex(7, "down", 3, 10)).toBe(9); // row 2 -> short row 3
    expect(stepIndex(9, "down", 3, 10)).toBeNull();
    expect(stepIndex(2, "down", 1, 3)).toBeNull();
  });
});
