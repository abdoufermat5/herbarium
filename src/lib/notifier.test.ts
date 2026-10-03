import { beforeEach, describe, expect, it } from "vitest";
import { takeFresh } from "./notifier";
import type { PageMeta } from "./types";

function due(id: string, nextReview: number): PageMeta {
  return { schemaVersion: 1, id, title: id, tags: [], folder: null, note: "", createdAt: 1,
    updatedAt: 1, intervalMinutes: 1, nextReview, lastReview: null, allowCdn: false };
}
beforeEach(() => takeFresh([]));

describe("review notification deduplication", () => {
  it("announces only newly due pages on a repeated queue refresh", () => {
    const first = due("first", 100);
    const second = due("second", 200);
    expect(takeFresh([first]).map((p) => p.id)).toEqual(["first"]);
    expect(takeFresh([first, second]).map((p) => p.id)).toEqual(["second"]);
    expect(takeFresh([first, second])).toEqual([]);
  });

  it("announces a new scheduled occurrence and a page that leaves then reenters the queue", () => {
    takeFresh([due("same", 100)]);
    expect(takeFresh([due("same", 300)]).map((p) => p.nextReview)).toEqual([300]);
    takeFresh([]);
    expect(takeFresh([due("same", 300)]).map((p) => p.id)).toEqual(["same"]);
  });
});
