import { describe, it, expect } from "vitest";
import {
  splitDuration,
  fmtDuration,
  fmtDurationShort,
  plural,
  fmtDate,
  modKey,
  vaultLabels,
  UNIT_MINUTES,
  DURATION_UNITS,
} from "./format";
describe("splitDuration", () => {
  it("divides exact day intervals evenly into days", () => {
    expect(splitDuration(1440)).toEqual({ n: 1, unit: "day" });
    expect(splitDuration(2880)).toEqual({ n: 2, unit: "day" });
    expect(splitDuration(4320)).toEqual({ n: 3, unit: "day" });
  });

  it("divides exact hour intervals evenly into hours", () => {
    expect(splitDuration(60)).toEqual({ n: 1, unit: "hour" });
    expect(splitDuration(120)).toEqual({ n: 2, unit: "hour" });
    expect(splitDuration(180)).toEqual({ n: 3, unit: "hour" });
    expect(splitDuration(1500)).toEqual({ n: 25, unit: "hour" });
  });

  it("retains minute units for non-divisible durations", () => {
    expect(splitDuration(30)).toEqual({ n: 30, unit: "minute" });
    expect(splitDuration(90)).toEqual({ n: 90, unit: "minute" });
    expect(splitDuration(1470)).toEqual({ n: 1470, unit: "minute" });
  });

  it("matches unit minute constants", () => {
    expect(UNIT_MINUTES.minute).toBe(1);
    expect(UNIT_MINUTES.hour).toBe(60);
    expect(UNIT_MINUTES.day).toBe(1440);
    expect(DURATION_UNITS).toEqual(["minute", "hour", "day"]);
  });
});

describe("fmtDuration", () => {
  it("formats minute, hour, and day durations in active language", () => {
    expect(fmtDuration(30)).toBe("30 minutes");
    expect(fmtDuration(60)).toBe("1 hour");
    expect(fmtDuration(120)).toBe("2 hours");
    expect(fmtDuration(1440)).toBe("1 day");
    expect(fmtDuration(2880)).toBe("2 days");
  });
});

describe("fmtDurationShort", () => {
  it("formats compact durations for buttons", () => {
    expect(fmtDurationShort(30)).toBe("30m");
    expect(fmtDurationShort(60)).toBe("1h");
    expect(fmtDurationShort(120)).toBe("2h");
    expect(fmtDurationShort(1440)).toBe("1d");
    expect(fmtDurationShort(2880)).toBe("2d");
  });

  it("rounds computed intervals of a day or more to whole days", () => {
    expect(fmtDurationShort(1862)).toBe("1d");
    expect(fmtDurationShort(3321)).toBe("2d");
    expect(fmtDurationShort(11946)).toBe("8d");
    expect(fmtDuration(1862)).toBe("1 day");
    // Below a day nothing is rounded: a 90-minute preset stays exact.
    expect(fmtDurationShort(90)).toBe("90m");
  });
});

describe("plural", () => {
  it("formats singular and plural counts correctly", () => {
    expect(plural(1, "page")).toBe("1 page");
    expect(plural(3, "page")).toBe("3 pages");
    expect(plural(1, "minute")).toBe("1 minute");
    expect(plural(10, "minute")).toBe("10 minutes");
    expect(plural(1, "hour")).toBe("1 hour");
    expect(plural(2, "hour")).toBe("2 hours");
    expect(plural(1, "day")).toBe("1 day");
    expect(plural(5, "day")).toBe("5 days");
  });
});

describe("fmtDate", () => {
  it("returns empty string for null, undefined, or NaN", () => {
    expect(fmtDate(null)).toBe("");
    expect(fmtDate(undefined)).toBe("");
    expect(fmtDate(NaN)).toBe("");
  });

  it("formats valid timestamps using active locale", () => {
    const timestamp = new Date(2025, 5, 15, 12, 0, 0).getTime();
    expect(fmtDate(timestamp)).toBe("Jun 15, 2025");
  });
});

describe("modKey", () => {
  it("formats keyboard modifier key", () => {
    expect(modKey("K")).toMatch(/^(⌘K|Ctrl K)$/);
    expect(modKey()).toMatch(/^(⌘K|Ctrl K)$/);
  });
});

describe("vaultLabels", () => {
  it("names vaults by folder, adding the parent only where names collide", () => {
    expect(
      vaultLabels(["/home/me/work/Herbarium", "/home/me/Herbarium/", "C:\\Notes\\Plants", "/data/Herbarium"]),
    ).toEqual(["Herbarium (work)", "Herbarium (me)", "Plants", "Herbarium (data)"]);
  });
});
