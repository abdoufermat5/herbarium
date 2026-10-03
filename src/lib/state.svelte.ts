import { api } from "./api";
import { i18n, t, LOCALES } from "./i18n.svelte";
import { notifyDue } from "./notifier";
import { prefs } from "./prefs.svelte";
import type { Config, PageMeta, ReviewSettings, TagCount } from "./types";

type View = "list" | "review" | "settings";
export type Layout = "grid" | "list";
export type SortKey = "recent" | "title" | "review";
export type Theme = "light" | "dark";
export type ToastKind = "info" | "success" | "error";

interface Toast {
  id: number;
  message: string;
  kind: ToastKind;
}

/** Files dropped anywhere on the window, waiting to be imported. */
export const pendingFiles = $state<{ files: File[] }>({ files: [] });

interface AppState {
  initialized: boolean;
  initError: string | null;
  config: Config | null;
  view: View;
  pages: PageMeta[];
  folders: string[];
  tags: TagCount[];
  dueCount: number;
  /** Every page regardless of search, for the sidebar folder tree. */
  library: PageMeta[];
  sidebarOpen: boolean;
  search: string;
  folderFilter: string | null;
  tagFilter: string | null;
  importOpen: boolean;
  readId: string | null;
  busy: boolean;
  paletteOpen: boolean;
  inspectorOpen: boolean;
  layout: Layout;
  sort: SortKey;
  theme: Theme;
  toasts: Toast[];
  /** The vault's review settings (preset intervals, strategy, queue). */
  review: ReviewSettings;
}

const DEFAULT_REVIEW: ReviewSettings = {
  presets: [1440, 4320, 10080, 43200],
  strategy: "ladder",
  multiplier: 2,
  maxIntervalMinutes: 525600,
  importReviewMinutes: null,
  queueLimit: null,
};

function storedLayout(): Layout {
  return localStorage.getItem("herbarium.layout") === "list" ? "list" : "grid";
}

function systemTheme(): Theme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function storedTheme(): Theme {
  const v = localStorage.getItem("herbarium.theme");
  return v === "light" || v === "dark" ? v : systemTheme();
}

function storedSort(): SortKey {
  const v = localStorage.getItem("herbarium.sort");
  return v === "title" || v === "review" ? v : "recent";
}

function storedSidebar(): boolean {
  return localStorage.getItem("herbarium.sidebar") !== "closed";
}

export const app: AppState = $state({
  initialized: false,
  initError: null,
  config: null,
  view: "list",
  pages: [],
  folders: [],
  tags: [],
  library: [],
  sidebarOpen: storedSidebar(),
  dueCount: 0,
  search: "",
  folderFilter: null,
  tagFilter: null,
  importOpen: false,
  readId: null,
  busy: false,
  paletteOpen: false,
  inspectorOpen: prefs.detailsOpen,
  layout: storedLayout(),
  sort: storedSort(),
  theme: storedTheme(),
  toasts: [],
  review: { ...DEFAULT_REVIEW },
});

/* ------------------------------------------------------------------ toasts */

let toastSeq = 0;

export function toast(message: string, kind: ToastKind = "info", ttl?: number) {
  const id = ++toastSeq;
  app.toasts = [...app.toasts, { id, message, kind }].slice(-4);
  const life = ttl ?? (kind === "error" ? 6000 : 4200);
  setTimeout(() => dismissToast(id), life);
}

export function dismissToast(id: number) {
  app.toasts = app.toasts.filter((t) => t.id !== id);
}

/* ------------------------------------------------------------------ prefs */

export function setLayout(layout: Layout) {
  app.layout = layout;
  localStorage.setItem("herbarium.layout", layout);
}

export function setTheme(theme: Theme) {
  app.theme = theme;
  document.documentElement.dataset.theme = theme;
  localStorage.setItem("herbarium.theme", theme);
}

export function toggleTheme() {
  setTheme(app.theme === "dark" ? "light" : "dark");
}

/** Apply the theme on boot and follow the OS until the user picks one. */
export function initTheme() {
  document.documentElement.dataset.theme = app.theme;
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => {
    if (localStorage.getItem("herbarium.theme")) return;
    app.theme = e.matches ? "dark" : "light";
    document.documentElement.dataset.theme = app.theme;
  });
}

export function setSort(sort: SortKey) {
  app.sort = sort;
  localStorage.setItem("herbarium.sort", sort);
}

/* ------------------------------------------------------------------- data */

export function toggleSidebar() {
  app.sidebarOpen = !app.sidebarOpen;
  localStorage.setItem("herbarium.sidebar", app.sidebarOpen ? "open" : "closed");
}

export async function refreshAll() {
  try {
    const [tags, folders, due, library, review] = await Promise.all([
      api.tags(),
      api.folders(),
      api.reviewToday(),
      api.listPages(),
      api.reviewSettings(),
    ]);
    app.tags = tags;
    app.folders = folders;
    app.dueCount = due.length;
    app.library = library;
    app.review = review;
    armDueTimer(library);
    void notifyDue(due);
  } catch (e) {
    console.error(e);
  }
}

/** Interval of the background refresh; a review due sooner gets its own timer. */
const POLL_MS = 60_000;
let dueTimer: ReturnType<typeof setTimeout> | undefined;

/** Refresh right when the next scheduled review comes due, so a 5-minute review
 *  is announced on time instead of up to a poll late. */
function armDueTimer(library: PageMeta[]) {
  clearTimeout(dueTimer);
  const now = Date.now();
  let soonest = Infinity;
  for (const p of library) {
    if (p.nextReview && p.nextReview > now && p.nextReview < soonest) soonest = p.nextReview;
  }
  const wait = soonest - now;
  if (wait <= POLL_MS) dueTimer = setTimeout(() => void refreshAll(), wait + 250);
}

/** Persist new review settings; on rejection the old ones stay and the error is thrown. */
export async function saveReviewSettings(next: ReviewSettings) {
  app.review = await api.configureReview(next);
  const due = await api.reviewToday();
  app.dueCount = due.length;
}

let reloadSeq = 0;

/** `quiet` skips the busy indicator, for background refreshes. */
export async function reloadPages(quiet = false) {
  const seq = ++reloadSeq;
  if (!quiet) app.busy = true;
  try {
    const q = app.search.trim();
    const pages = q ? await api.searchPages(q) : await api.listPages();
    // A newer reload (e.g. the search changed meanwhile) owns the list.
    if (seq !== reloadSeq) return;
    app.pages = pages;
    await refreshAll();
  } catch (e) {
    console.error(e);
    if (!quiet) toast(t("toast.loadFailed"), "error");
  } finally {
    if (!quiet) app.busy = false;
  }
}

/** Pick up changes made while the window was in the background (agents over
 *  MCP, edits or sync tools touching the vault folder). */
async function resync() {
  if (!prefs.refreshOnFocus || !app.config?.vaultPath || app.busy) return;
  try {
    await api.rescan();
  } catch (e) {
    console.error(e);
    return;
  }
  await reloadPages(true);
}

/** Re-read the vault from disk now and refresh every list. */
export async function rescanVault() {
  const report = await api.rescan();
  await reloadPages(true);
  return report;
}

let backgroundStarted = false;

export async function initApp() {
  try {
    const cfg = await api.getConfig();
    app.config = cfg;
    app.initError = null;
    if (cfg.vaultPath) {
      await api.setVault(cfg.vaultPath);
      await reloadPages();
      app.view = prefs.startView;
    }
  } catch (e) {
    app.initError = String(e);
  } finally {
    app.initialized = true;
    // initApp runs again on "Retry": register background work only once.
    if (!backgroundStarted) {
      backgroundStarted = true;
      window.addEventListener("focus", () => void resync());
      // Reviews can be minutes long: keep the "due" badge current while the window sits open.
      setInterval(() => {
        if (app.config?.vaultPath && !app.busy) void refreshAll();
      }, POLL_MS);
    }
  }
}

/** Apply the folder/tag filters and the current sort to a page list. */
export function visiblePages(pages: PageMeta[]): PageMeta[] {
  let list = pages;
  const folder = app.folderFilter;
  const tag = app.tagFilter;
  if (folder) list = list.filter((p) => p.folder === folder);
  if (tag) list = list.filter((p) => p.tags.includes(tag));
  const sorted = [...list];
  switch (app.sort) {
    case "title":
      sorted.sort((a, b) => (a.title || t("common.untitled")).localeCompare(
          b.title || t("common.untitled"),
          LOCALES[i18n.locale].bcp47,
        ));
      break;
    case "review":
      sorted.sort((a, b) => (a.nextReview ?? Infinity) - (b.nextReview ?? Infinity));
      break;
    default:
      sorted.sort((a, b) => (b.updatedAt || b.createdAt) - (a.updatedAt || a.createdAt));
  }
  return sorted;
}

export function dueLabel(): string {
  const n = app.dueCount;
  if (n === 0) return t("sidebar.review");
  return t("sidebar.reviewDue", { count: n });
}

export function clearFilters() {
  app.folderFilter = null;
  app.tagFilter = null;
}
