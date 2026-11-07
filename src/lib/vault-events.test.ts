import { describe, expect, it } from "vitest";
import type { PageMeta } from "./types";
import { affectsReviewQueue, applyPageEvents, introducesFolder, tagCounts, type PageEvent } from "./vault-events";

function page(id: string, patch: Partial<PageMeta> = {}): PageMeta {
  return { schemaVersion: 1, id, title: id, tags: [], folder: null, note: "", createdAt: 1,
    updatedAt: 1, intervalMinutes: null, nextReview: null, lastReview: null, allowCdn: false, ...patch };
}
const event = (kind: PageEvent["kind"], meta: PageMeta): PageEvent => ({ kind, id: meta.id, meta });

describe("applyPageEvents", () => {
  const list = [page("a"), page("b"), page("c")];

  it("replaces an updated page in place and leaves the others identical", () => {
    const changed = page("b", { title: "B!" });
    const next = applyPageEvents(list, [event("page.updated", changed)], true);
    expect(next.map((p) => p.title)).toEqual(["a", "B!", "c"]);
    expect(next[0]).toBe(list[0]);
    expect(list[1].title).toBe("b");
  });

  it("drops deleted pages and appends created ones only when asked to insert", () => {
    const events = [event("page.deleted", page("a")), event("page.created", page("d"))];
    expect(applyPageEvents(list, events, true).map((p) => p.id)).toEqual(["b", "c", "d"]);
    expect(applyPageEvents(list, events, false).map((p) => p.id)).toEqual(["b", "c"]);
  });

  it("lets the last event for a page win and returns the same array when nothing changes", () => {
    const events = [event("page.updated", page("a", { title: "x" })), event("page.deleted", page("a"))];
    expect(applyPageEvents(list, events, true).map((p) => p.id)).toEqual(["b", "c"]);
    expect(applyPageEvents(list, [event("page.deleted", page("zzz"))], true)).toBe(list);
    expect(applyPageEvents(list, [event("page.updated", list[0])], true)).toBe(list);
    expect(applyPageEvents(list, [event("page.updated", page("zzz"))], false)).toBe(list);
  });
});

describe("derived aggregates", () => {
  it("counts tags in the backend's order", () => {
    const pages = [page("a", { tags: ["web", "rust"] }), page("b", { tags: ["rust"] })];
    expect(tagCounts(pages)).toEqual([{ tag: "rust", count: 2 }, { tag: "web", count: 1 }]);
  });

  it("flags only changes that can move the review queue", () => {
    const before = page("a", { tags: ["x"], nextReview: 5 });
    expect(affectsReviewQueue(before, event("page.updated", page("a", { tags: ["x"], nextReview: 5, title: "t" })))).toBe(false);
    expect(affectsReviewQueue(before, event("page.updated", page("a", { tags: ["x"], nextReview: 9 })))).toBe(true);
    expect(affectsReviewQueue(before, event("page.updated", page("a", { tags: ["y"], nextReview: 5 })))).toBe(true);
    expect(affectsReviewQueue(before, event("page.deleted", before))).toBe(true);
    expect(affectsReviewQueue(undefined, event("page.updated", before))).toBe(true);
  });

  it("notices a page landing in an unknown folder", () => {
    const inNew = event("page.updated", page("a", { folder: "New" }));
    expect(introducesFolder(["Old"], inNew)).toBe(true);
    expect(introducesFolder(["New"], inNew)).toBe(false);
    expect(introducesFolder([], event("page.updated", page("a")))).toBe(false);
  });
});
