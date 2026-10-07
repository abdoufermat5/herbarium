import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PageMeta, ProposalSummary, ReviewSettings, ReviewStats } from "./types";

const mocks = vi.hoisted(() => {
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  });
  vi.stubGlobal("window", { matchMedia: () => ({ matches: false, addEventListener: () => {} }) });
  return {
    tags: vi.fn(), folders: vi.fn(), reviewToday: vi.fn(), listPages: vi.fn(),
    reviewSettings: vi.fn(), reviewStats: vi.fn(), searchPages: vi.fn(), listProposals: vi.fn(), savedSearches: vi.fn(), listPaths: vi.fn(), updatePath: vi.fn(), appearance: vi.fn(),
  };
});
vi.mock("./api", () => ({ api: mocks }));
vi.mock("./notifier", () => ({ notifyDue: vi.fn().mockResolvedValue(undefined) }));
import { app, inFolder, movePageInPath, refreshAll, reloadPages, resetWorkspace, visiblePages } from "./state.svelte";

function page(id: string, patch: Partial<PageMeta> = {}): PageMeta {
  return { schemaVersion: 1, id, title: id, tags: [], folder: null, note: "", createdAt: 1,
    updatedAt: 1, intervalMinutes: null, nextReview: null, lastReview: null, allowCdn: false, ...patch };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}
const settings: ReviewSettings = { presets: [1440], strategy: "ladder", multiplier: 2,
  maxIntervalMinutes: 525600, desiredRetention: 0.9, importReviewMinutes: null, queueLimit: 1,
  excludeFolders: [], excludeTags: [] };
const stats: ReviewStats = { dueTotal: 250, overdue: 100, reviewedToday: 0, upcoming: [], totalReviews: 0,
  activity: [], streak: 0, longestStreak: 0 };

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(console, "error").mockImplementation(() => {});
  resetWorkspace();
  app.toasts = [];
  app.sort = "relevance";
  Object.values(mocks).forEach((mock) => mock.mockReset());
  mocks.tags.mockResolvedValue([]);
  mocks.folders.mockResolvedValue([]);
  mocks.reviewToday.mockResolvedValue([page("one-due", { nextReview: 1 })]);
  mocks.listPages.mockResolvedValue([]);
  mocks.reviewSettings.mockResolvedValue(settings);
  mocks.reviewStats.mockResolvedValue(stats);
  mocks.listProposals.mockResolvedValue([]);
  mocks.savedSearches.mockResolvedValue([]);
  mocks.listPaths.mockResolvedValue([]);
  mocks.appearance.mockResolvedValue({ folders: {}, tags: {} });
});
afterEach(() => { resetWorkspace(); vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); });
afterAll(() => vi.unstubAllGlobals());

describe("library state", () => {
  it("includes descendants without matching similarly named siblings and intersects tags", () => {
    const pages = [page("parent", { folder: "Notes", tags: ["botany"] }),
      page("child", { folder: "Notes/Deep", tags: ["botany"] }),
      page("other-tag", { folder: "Notes/Deep", tags: ["math"] }),
      page("sibling", { folder: "NotesExtra", tags: ["botany"] }), page("root")];
    app.folderFilter = "Notes";
    app.tagFilter = "botany";
    expect(visiblePages(pages).map((p) => p.id)).toEqual(["parent", "child"]);
    expect(inFolder("NotesExtra", "Notes")).toBe(false);
    expect(inFolder(null, "Notes")).toBe(false);
  });

  it("preserves backend search relevance unless an explicit sort is chosen", () => {
    const pages = [page("z", { updatedAt: 1 }), page("a", { updatedAt: 100 })];
    app.search = "orchid";
    expect(visiblePages(pages).map((p) => p.id)).toEqual(["z", "a"]);
    app.sort = "recent";
    expect(visiblePages(pages).map((p) => p.id)).toEqual(["a", "z"]);
    expect(pages.map((p) => p.id)).toEqual(["z", "a"]);
  });

  it("keeps the library available when review settings fail and uses uncapped due totals", async () => {
    mocks.listPages.mockResolvedValue([page("healthy")]);
    mocks.reviewSettings.mockRejectedValue("review.json is malformed");
    await reloadPages(true);
    expect(app.pages.map((p) => p.id)).toEqual(["healthy"]);
    expect(app.loadError).toBeNull();
    expect(app.reviewError).toBe("review.json is malformed");
    expect(app.dueCount).toBe(250);
  });

  it("retains a descriptive load failure until a successful retry", async () => {
    mocks.listPages.mockRejectedValueOnce("permission denied: /vault");
    await reloadPages(true);
    expect(app.loadError).toBe("permission denied: /vault");
    mocks.listPages.mockResolvedValueOnce([page("recovered")]);
    await reloadPages(true);
    expect(app.loadError).toBeNull();
    expect(app.pages.map((p) => p.id)).toEqual(["recovered"]);
  });

  it("prevents a slower previous search from replacing the current results", async () => {
    const old = deferred<PageMeta[]>();
    mocks.searchPages.mockReturnValueOnce(old.promise).mockResolvedValueOnce([page("current")]);
    app.search = "old";
    const previous = reloadPages(true);
    app.search = "current";
    await reloadPages(true);
    old.resolve([page("stale")]);
    await previous;
    expect(app.search).toBe("current");
    expect(app.pages.map((p) => p.id)).toEqual(["current"]);
  });

  it("invalidates an in-flight vault refresh when the workspace is reset", async () => {
    const old = deferred<PageMeta[]>();
    mocks.listPages.mockReturnValueOnce(old.promise);
    const pending = refreshAll();
    resetWorkspace();
    old.resolve([page("previous-vault")]);
    await pending;
    expect(app.library).toEqual([]);
    expect(app.dueCount).toBe(0);
    expect(app.reviewStats).toBeNull();
  });
});

describe("agent proposals", () => {
  const proposal = (id: string, at: number): ProposalSummary => ({
    id, at, baseUpdatedAt: 1, title: `${id} v2`, pageTitle: id, bytes: 10, stale: false,
  });

  it("announces what is waiting once, then only proposals that arrive later", async () => {
    mocks.listProposals.mockResolvedValue([proposal("a", 2), proposal("b", 1)]);
    await refreshAll();
    expect(app.proposals.map((p) => p.id)).toEqual(["a", "b"]);
    expect(app.toasts).toHaveLength(1);
    expect(app.toasts[0].message).toContain("2");

    await refreshAll();
    expect(app.toasts).toHaveLength(1);

    mocks.listProposals.mockResolvedValue([proposal("a", 3), proposal("b", 1)]);
    await refreshAll();
    expect(app.toasts).toHaveLength(2);
    expect(app.toasts[1].action).toBeDefined();
  });

  it("keeps the library when proposals cannot be read, and forgets them on reset", async () => {
    mocks.listPages.mockResolvedValue([page("p")]);
    mocks.listProposals.mockRejectedValue("proposals unreadable");
    expect(await refreshAll()).toHaveLength(1);
    expect(app.proposals).toEqual([]);

    mocks.listProposals.mockResolvedValue([proposal("p", 1)]);
    await refreshAll();
    expect(app.proposals).toHaveLength(1);
    resetWorkspace();
    expect(app.proposals).toEqual([]);
    expect(app.proposalOpen).toBe(false);
  });
});

describe("reading paths", () => {
  it("moves a page one step and adopts the saved order", async () => {
    mocks.listPaths.mockResolvedValue([{ id: "p", name: "P", pages: ["a", "b", "c"] }]);
    await refreshAll();
    mocks.updatePath.mockImplementation((id: string, patch: { pages: string[] }) =>
      Promise.resolve({ id, name: "P", pages: patch.pages }),
    );
    await movePageInPath("p", "c", -1);
    expect(mocks.updatePath).toHaveBeenCalledWith("p", { pages: ["a", "c", "b"] });
    expect(app.paths[0].pages).toEqual(["a", "c", "b"]);

    // At an end it is a no-op, with no backend call.
    mocks.updatePath.mockClear();
    await movePageInPath("p", "a", -1);
    expect(mocks.updatePath).not.toHaveBeenCalled();
  });
});
