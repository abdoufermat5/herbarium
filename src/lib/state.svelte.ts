import { api } from "./api";
import { confirmAction } from "./confirm.svelte";
import { pickFolder } from "./folder-picker.svelte";
import { i18n, t, LOCALES } from "./i18n.svelte";
import { navigate } from "./navigation.svelte";
import { notifyDue } from "./notifier";
import { prefs } from "./prefs.svelte";
import type {
  BulkError,
  BulkUpdateResult,
  Config,
  PageMeta,
  ReviewSettings,
  ReviewStats,
  TagCount,
} from "./types";

export type View = "list" | "review" | "settings" | "trash";
export type Layout = "grid" | "list";
/** `relevance` keeps the backend's full-text ranking while searching and falls back to recent otherwise. */
export type SortKey = "relevance" | "recent" | "title" | "review";
/** The theme actually shown. */
export type Theme = "light" | "dark";
/** What the user picked; `system` follows the OS. */
export type ThemeChoice = "system" | "light" | "dark";
export type ToastKind = "info" | "success" | "error";

interface Toast {
  id: number;
  message: string;
  kind: ToastKind;
  action?: ToastAction;
}

export interface ToastAction {
  label: string;
  run: () => void;
}

/** Files dropped anywhere on the window (or staged by a folder drop), waiting to be imported. */
export const pendingFiles = $state<{ files: File[]; folder?: string | null; allowCdn?: boolean }>({
  files: [],
});

/** A sidebar creation requested from elsewhere (palette, menus); the Sidebar consumes and clears it. */
export interface CreateRequest {
  kind: "page" | "folder";
  /** Parent folder path, "" for the vault root. */
  parent: string;
}

interface AppState {
  initialized: boolean;
  initError: string | null;
  /** Why the library could not be loaded, shown distinctly from an empty library. */
  loadError: string | null;
  /** Why the vault's review settings could not be read (e.g. a corrupt review.json). */
  reviewError: string | null;
  config: Config | null;
  view: View;
  pages: PageMeta[];
  folders: string[];
  tags: TagCount[];
  /** Pages due now, uncapped (not limited by the queue limit). */
  dueCount: number;
  reviewStats: ReviewStats | null;
  /** Every page regardless of search, for the sidebar folder tree. */
  library: PageMeta[];
  /** Bumped after each rescan so open documents can reload external changes. */
  vaultRevision: number;
  sidebarOpen: boolean;
  search: string;
  folderFilter: string | null;
  tagFilter: string | null;
  importOpen: boolean;
  readId: string | null;
  /** The reader was opened by a review session (auto-advance on completion). */
  reviewSession: boolean;
  createRequest: CreateRequest | null;
  busy: boolean;
  paletteOpen: boolean;
  inspectorOpen: boolean;
  /** The reader's version-history panel is open. */
  historyOpen: boolean;
  layout: Layout;
  sort: SortKey;
  /** The theme in effect. */
  theme: Theme;
  /** The user's choice, possibly `system`. */
  themeChoice: ThemeChoice;
  toasts: Toast[];
  /** The vault's review settings (preset intervals, strategy, queue). */
  review: ReviewSettings;
}

const DEFAULT_REVIEW: ReviewSettings = {
  presets: [1440, 4320, 10080, 43200],
  strategy: "fsrs",
  multiplier: 2,
  maxIntervalMinutes: 525600,
  desiredRetention: 0.9,
  importReviewMinutes: null,
  queueLimit: null,
};

function storedLayout(): Layout {
  return localStorage.getItem("herbarium.layout") === "list" ? "list" : "grid";
}

function systemTheme(): Theme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** Earlier versions stored only light/dark (and nothing for "follow the OS"); both stay valid. */
function storedThemeChoice(): ThemeChoice {
  const v = localStorage.getItem("herbarium.theme");
  return v === "light" || v === "dark" ? v : "system";
}

function resolveTheme(choice: ThemeChoice): Theme {
  return choice === "system" ? systemTheme() : choice;
}

function storedSort(): SortKey {
  const v = localStorage.getItem("herbarium.sort");
  return v === "title" || v === "review" || v === "recent" ? v : "relevance";
}

function storedSidebar(): boolean {
  return localStorage.getItem("herbarium.sidebar") !== "closed";
}

const initialThemeChoice = storedThemeChoice();

export const app: AppState = $state({
  initialized: false,
  initError: null,
  loadError: null,
  reviewError: null,
  config: null,
  view: "list",
  pages: [],
  folders: [],
  tags: [],
  library: [],
  vaultRevision: 0,
  sidebarOpen: storedSidebar(),
  dueCount: 0,
  reviewStats: null,
  search: "",
  folderFilter: null,
  tagFilter: null,
  importOpen: false,
  readId: null,
  reviewSession: false,
  createRequest: null,
  busy: false,
  paletteOpen: false,
  inspectorOpen: prefs.detailsOpen,
  historyOpen: false,
  layout: storedLayout(),
  sort: storedSort(),
  theme: resolveTheme(initialThemeChoice),
  themeChoice: initialThemeChoice,
  toasts: [],
  review: { ...DEFAULT_REVIEW },
});

/** A readable message for anything thrown by a backend call (Tauri rejects with plain strings). */
export function errorMessage(error: unknown): string {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message) return error.message;
  if (error && typeof error === "object" && "message" in error) {
    const m = (error as { message: unknown }).message;
    if (typeof m === "string" && m) return m;
  }
  return t("error.unknown");
}

/* ------------------------------------------------------------------ toasts */

let toastSeq = 0;

/** Show a toast and return its id. `action` adds a button that runs it and dismisses the toast. */
export function toast(message: string, kind: ToastKind = "info", ttl?: number, action?: ToastAction): number {
  const id = ++toastSeq;
  app.toasts = [...app.toasts, { id, message, kind, action }].slice(-4);
  const life = ttl ?? (kind === "error" ? 6000 : 4200);
  setTimeout(() => dismissToast(id), life);
  return id;
}

export function dismissToast(id: number) {
  app.toasts = app.toasts.filter((t) => t.id !== id);
}

/* ------------------------------------------------------------------ prefs */

export function setLayout(layout: Layout) {
  app.layout = layout;
  localStorage.setItem("herbarium.layout", layout);
}

/** Pick a theme; `system` follows the OS and keeps following it. */
export function setTheme(choice: ThemeChoice) {
  app.themeChoice = choice;
  app.theme = resolveTheme(choice);
  document.documentElement.dataset.theme = app.theme;
  if (choice === "system") localStorage.removeItem("herbarium.theme");
  else localStorage.setItem("herbarium.theme", choice);
}

/** Flip the theme in effect to its opposite as an explicit choice. */
export function toggleTheme() {
  setTheme(app.theme === "dark" ? "light" : "dark");
}

/** Apply the theme on boot and follow OS changes while `system` is selected. */
export function initTheme() {
  document.documentElement.dataset.theme = app.theme;
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (app.themeChoice !== "system") return;
    app.theme = systemTheme();
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

function openSidebar() {
  if (app.sidebarOpen) return;
  app.sidebarOpen = true;
  localStorage.setItem("herbarium.sidebar", "open");
}

let refreshSeq = 0;

/**
 * Reload folders, tags, the whole library, the review queue/stats and the
 * review settings. Each source is independent: a corrupt review.json is
 * reported in `app.reviewError` and never prevents the library from loading;
 * a library failure sets `app.loadError`. Resolves to the library, or null
 * when it could not be loaded.
 */
export async function refreshAll(): Promise<PageMeta[] | null> {
  const seq = ++refreshSeq;
  const [tags, folders, due, library, settings, stats] = await Promise.allSettled([
    api.tags(),
    api.folders(),
    api.reviewToday(),
    api.listPages(),
    api.reviewSettings(),
    api.reviewStats(),
  ]);
  // A newer refresh owns the shared state; this one only hands back its library.
  if (seq !== refreshSeq) return library.status === "fulfilled" ? library.value : null;

  if (tags.status === "fulfilled") app.tags = tags.value;
  else console.error(tags.reason);
  if (folders.status === "fulfilled") app.folders = folders.value;
  else console.error(folders.reason);

  if (settings.status === "fulfilled") {
    app.review = settings.value;
    app.reviewError = null;
  } else {
    console.error(settings.reason);
    app.reviewError = errorMessage(settings.reason);
  }
  if (stats.status === "fulfilled") {
    app.reviewStats = stats.value;
    app.dueCount = stats.value.dueTotal;
  } else {
    console.error(stats.reason);
    if (due.status === "fulfilled") app.dueCount = due.value.length;
  }
  if (due.status === "fulfilled") void notifyDue(due.value);
  else console.error(due.reason);

  if (library.status === "rejected") {
    console.error(library.reason);
    app.loadError = errorMessage(library.reason);
    return null;
  }
  app.library = library.value;
  armDueTimer(library.value);
  return library.value;
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
  app.reviewError = null;
  try {
    app.reviewStats = await api.reviewStats();
    app.dueCount = app.reviewStats.dueTotal;
  } catch (e) {
    console.error(e);
  }
}

let reloadSeq = 0;

/**
 * Reload the page list (search results when a query is set) and every
 * sidebar/badge source. Failures land in `app.loadError`; `quiet` skips the
 * busy indicator and the error toast, for background refreshes.
 */
export async function reloadPages(quiet = false) {
  const seq = ++reloadSeq;
  if (!quiet) app.busy = true;
  try {
    const q = app.search.trim();
    const search = q
      ? api.searchPages(q).then(
          (hits) => ({ hits }),
          (error: unknown) => ({ error }),
        )
      : null;
    const library = await refreshAll();
    const outcome = search ? await search : null;
    // A newer reload (e.g. the search changed meanwhile) owns the list.
    if (seq !== reloadSeq) return;
    if (outcome && "error" in outcome) throw outcome.error;
    if (outcome) app.pages = outcome.hits;
    else if (library) app.pages = library;
    else throw new Error(app.loadError ?? t("toast.loadFailed"));
    if (library) app.loadError = null;
  } catch (e) {
    if (seq !== reloadSeq) return;
    console.error(e);
    app.loadError = errorMessage(e);
    if (!quiet) toast(`${t("toast.loadFailed")} ${app.loadError}`, "error");
  } finally {
    if (!quiet && seq === reloadSeq) app.busy = false;
  }
}

/** Pick up changes made while the window was in the background (agents over
 *  MCP, edits or sync tools touching the vault folder). */
async function resync() {
  if (!prefs.refreshOnFocus || !app.config?.vaultPath || app.busy) return;
  try {
    const report = await api.rescan();
    if (report.skipped.length > 0) console.warn("rescan skipped files", report.skipped);
  } catch (e) {
    console.error(e);
    return;
  }
  app.vaultRevision++;
  await reloadPages(true);
}

/** Re-read the vault from disk now and refresh every list. */
export async function rescanVault() {
  const report = await api.rescan();
  app.vaultRevision++;
  await reloadPages(true);
  return report;
}

/** Forget everything tied to the open vault (selection, filters, search, caches). */
export function resetWorkspace() {
  reloadSeq++;
  refreshSeq++;
  clearTimeout(dueTimer);
  app.pages = [];
  app.library = [];
  app.folders = [];
  app.tags = [];
  app.dueCount = 0;
  app.reviewStats = null;
  app.search = "";
  app.folderFilter = null;
  app.tagFilter = null;
  app.readId = null;
  app.reviewSession = false;
  app.historyOpen = false;
  app.createRequest = null;
  app.loadError = null;
  app.reviewError = null;
  app.view = "list";
  pendingFiles.files = [];
  pendingFiles.folder = undefined;
  pendingFiles.allowCdn = undefined;
  sessionSkipped.clear();
}

/**
 * Switch the app to a vault the backend has already opened (`api.setVault` /
 * `api.createVault` resolved to `config`). Call it only after success, so a
 * failed switch leaves the current workspace untouched. Callers that can leave
 * unsaved work wrap it in `navigate`.
 */
export async function adoptVault(config: Config) {
  resetWorkspace();
  app.config = config;
  app.busy = false;
  await reloadPages();
}

let backgroundStarted = false;

export async function initApp() {
  try {
    const cfg = await api.getConfig();
    app.config = cfg;
    app.initError = null;
    if (cfg.vaultPath) {
      app.config = await api.setVault(cfg.vaultPath);
      await reloadPages();
      app.view = prefs.startView;
    }
  } catch (e) {
    app.initError = errorMessage(e);
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

/** True when `folder` is `ancestor` or lies beneath it. */
export function inFolder(folder: string | null | undefined, ancestor: string): boolean {
  return !!folder && (folder === ancestor || folder.startsWith(`${ancestor}/`));
}

/** Apply the folder/tag filters and the current sort to a page list. A folder
 *  filter includes its subfolders. While searching, `relevance` keeps the
 *  backend's ranking; only an explicitly chosen other sort reorders hits. */
export function visiblePages(pages: PageMeta[]): PageMeta[] {
  let list = pages;
  const folder = app.folderFilter;
  const tag = app.tagFilter;
  if (folder) list = list.filter((p) => inFolder(p.folder, folder));
  if (tag) list = list.filter((p) => p.tags.includes(tag));
  if (app.sort === "relevance" && app.search.trim()) return [...list];
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

/** Clear the folder/tag filters in place (no navigation, nothing to guard). */
export function clearFilters() {
  app.folderFilter = null;
  app.tagFilter = null;
}

/* -------------------------------------------------------------- navigation
 * Every route change goes through `navigate` so views with unsaved work can
 * veto it. These helpers resolve true when the change happened, false when a
 * guard vetoed it or it failed (the failure is toasted); they never reject. */

async function go(action: () => void | Promise<void>): Promise<boolean> {
  try {
    return await navigate(action);
  } catch (e) {
    console.error(e);
    toast(errorMessage(e), "error");
    return false;
  }
}

/** Open a page in the reader. */
export function openPage(id: string): Promise<boolean> {
  return go(() => {
    app.reviewSession = false;
    app.readId = id;
  });
}

/** Show a top-level view (leaves the reader). */
export function goView(view: View): Promise<boolean> {
  return go(() => {
    app.reviewSession = false;
    app.readId = null;
    app.view = view;
  });
}

/** The whole library: no filters, no search. */
export function goAll(): Promise<boolean> {
  return go(async () => {
    const hadSearch = app.search !== "";
    app.reviewSession = false;
    app.readId = null;
    app.view = "list";
    clearFilters();
    app.search = "";
    if (hadSearch) await reloadPages(true);
  });
}

/** The list filtered to a folder and its subfolders. */
export function showFolder(path: string): Promise<boolean> {
  return go(() => {
    app.reviewSession = false;
    app.readId = null;
    app.view = "list";
    app.tagFilter = null;
    app.folderFilter = path;
  });
}

/** The list filtered to a tag. */
export function showTag(tag: string): Promise<boolean> {
  return go(() => {
    app.reviewSession = false;
    app.readId = null;
    app.view = "list";
    app.folderFilter = null;
    app.tagFilter = tag;
  });
}

/** Ask the sidebar to start creating a page or folder under `parent` ("" = root). */
export function requestCreate(kind: "page" | "folder", parent = "") {
  openSidebar();
  app.createRequest = { kind, parent };
}

/* ------------------------------------------------------------ review session */

/** Pages the user skipped in the running session; they stay due but are not offered again. */
const sessionSkipped = new Set<string>();

/** Open the first due page in the reader and keep advancing as pages are completed. */
export async function startReviewSession(): Promise<boolean> {
  const ok = await go(async () => {
    sessionSkipped.clear();
    const first = (await api.reviewToday())[0];
    if (!first) {
      toast(t("toast.reviewEmpty"));
      return;
    }
    app.reviewSession = true;
    app.readId = first.id;
  });
  return ok && app.reviewSession;
}

/**
 * Advance the session to the next due page. `skipId` is a page to leave due
 * but not offer again this session (skipping never touches its schedule). When
 * nothing is left the session ends on the review view. Resolves true when
 * another page was opened.
 */
export async function nextReviewPage(skipId?: string): Promise<boolean> {
  if (skipId) sessionSkipped.add(skipId);
  const ok = await go(async () => {
    const due = await api.reviewToday();
    const next = due.find((p) => !sessionSkipped.has(p.id) && p.id !== app.readId);
    if (next) {
      app.reviewSession = true;
      app.readId = next.id;
      return;
    }
    app.reviewSession = false;
    app.readId = null;
    app.view = "review";
    toast(t("toast.reviewSessionDone"), "success");
    void refreshAll();
  });
  return ok && app.reviewSession;
}

/* ------------------------------------------------------------- page actions */

function titleOf(id: string): string {
  const meta = app.library.find((p) => p.id === id) ?? app.pages.find((p) => p.id === id);
  return meta?.title || t("common.untitled");
}

/** First failure as "Title: reason", with a count of the rest. */
function failureDetail(errors: BulkError[]): string {
  const first = errors[0];
  if (!first) return "";
  const rest = errors.length > 1 ? ` (+${errors.length - 1})` : "";
  return `${titleOf(first.id)}: ${first.error}${rest}`;
}

/**
 * Choose a destination folder, then move `ids` there (null = vault root).
 * Resolves to the bulk result, or null when cancelled or the call failed
 * (toasted). Per-page failures are reported and listed in `errors`.
 */
export async function movePages(ids: string[]): Promise<BulkUpdateResult | null> {
  if (ids.length === 0) return null;
  const folders = new Set(ids.map((id) => app.library.find((p) => p.id === id)?.folder ?? null));
  const initial = folders.size === 1 ? [...folders][0] : undefined;
  const dest = await pickFolder({ title: t("folderPicker.move"), initial });
  if (dest === undefined) return null;
  try {
    const result = await api.bulkUpdate(ids, { folder: dest });
    const done = result.updated.length;
    if (result.errors.length > 0) {
      toast(
        t("toast.movePartial", { done, failed: result.errors.length, detail: failureDetail(result.errors) }),
        "error",
      );
    } else {
      toast(t("toast.moved", { count: done }), "success");
    }
    await reloadPages(true);
    return result;
  } catch (e) {
    console.error(e);
    toast(t("toast.moveFailed", { detail: errorMessage(e) }), "error");
    return null;
  }
}

async function restoreTrashed(ids: string[]) {
  const restored: string[] = [];
  const errors: BulkError[] = [];
  for (const id of ids) {
    try {
      await api.restorePage(id);
      restored.push(id);
    } catch (e) {
      errors.push({ id, error: errorMessage(e) });
    }
  }
  if (restored.length > 0) toast(t("toast.restored", { count: restored.length }), "success");
  if (errors.length > 0) toast(t("toast.restoreFailed", { detail: failureDetail(errors) }), "error");
  await reloadPages(true);
}

/**
 * Confirm, then move pages to the trash with an Undo toast. Resolves to the
 * ids actually trashed ([] when cancelled, vetoed or failed). Per-page
 * failures are reported. Closes the reader when its page was trashed.
 */
export async function deletePages(ids: string[]): Promise<string[]> {
  if (ids.length === 0) return [];
  const single = ids.length === 1 ? (app.library.find((p) => p.id === ids[0]) ?? app.pages.find((p) => p.id === ids[0])) : undefined;
  const confirmed = await confirmAction({
    title: t("confirm.trash.title", { count: ids.length }),
    message: t("confirm.trash.message"),
    confirmLabel: t("confirm.trash.confirm"),
    danger: true,
    subject: single
      ? { icon: "file-text", label: single.title || t("common.untitled"), meta: single.folder ?? t("confirm.vaultRoot") }
      : undefined,
  });
  if (!confirmed) return [];

  let trashed: string[] = [];
  const run = async () => {
    try {
      const result = await api.trashPages(ids);
      trashed = result.deleted;
      if (result.errors.length > 0) {
        toast(
          t("toast.trashPartial", { done: trashed.length, failed: result.errors.length, detail: failureDetail(result.errors) }),
          "error",
        );
      }
    } catch (e) {
      console.error(e);
      toast(t("toast.trashFailed", { detail: errorMessage(e) }), "error");
      return;
    }
    if (app.readId && trashed.includes(app.readId)) {
      app.readId = null;
      app.reviewSession = false;
    }
  };
  // Trashing the open page leaves the reader, so unsaved edits get their say first.
  if (app.readId && ids.includes(app.readId)) {
    if (!(await go(run))) return [];
  } else {
    await run();
  }

  if (trashed.length > 0) {
    const restorable = [...trashed];
    const message = t("toast.trashed", { count: trashed.length });
    toast(message, "success", 8000, {
      label: t("toast.undo"),
      run: () => void restoreTrashed(restorable),
    });
  }
  await reloadPages(true);
  return trashed;
}

/** Copy a page (new id, title "… (copy)"). Resolves to the copy, or null on failure (toasted). */
export async function duplicatePage(id: string): Promise<PageMeta | null> {
  try {
    const copy = await api.duplicatePage(id);
    toast(t("toast.duplicated", { title: copy.title || t("common.untitled") }), "success");
    await reloadPages(true);
    return copy;
  } catch (e) {
    console.error(e);
    toast(t("toast.duplicateFailed", { detail: errorMessage(e) }), "error");
    return null;
  }
}
