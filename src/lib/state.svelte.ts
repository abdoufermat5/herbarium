import { api } from "./api";
import type { Config, PageMeta, TagCount } from "./types";

export type View = "list" | "review";
export type Layout = "grid" | "list";
export type SortKey = "recent" | "title" | "review";
export type ToastKind = "info" | "success" | "error";

export interface Toast {
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
  toasts: Toast[];
}

function storedLayout(): Layout {
  return localStorage.getItem("herbarium.layout") === "list" ? "list" : "grid";
}

function storedSort(): SortKey {
  const v = localStorage.getItem("herbarium.sort");
  return v === "title" || v === "review" ? v : "recent";
}

export const app: AppState = $state({
  initialized: false,
  initError: null,
  config: null,
  view: "list",
  pages: [],
  folders: [],
  tags: [],
  dueCount: 0,
  search: "",
  folderFilter: null,
  tagFilter: null,
  importOpen: false,
  readId: null,
  busy: false,
  paletteOpen: false,
  inspectorOpen: false,
  layout: storedLayout(),
  sort: storedSort(),
  toasts: [],
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

export function setSort(sort: SortKey) {
  app.sort = sort;
  localStorage.setItem("herbarium.sort", sort);
}

/* ------------------------------------------------------------------- data */

export async function refreshAll() {
  try {
    const [tags, folders, due] = await Promise.all([
      api.tags(),
      api.folders(),
      api.reviewToday(),
    ]);
    app.tags = tags;
    app.folders = folders;
    app.dueCount = due.length;
  } catch (e) {
    console.error(e);
  }
}

export async function reloadPages() {
  app.busy = true;
  try {
    const q = app.search.trim();
    app.pages = q ? await api.searchPages(q) : await api.listPages();
    await refreshAll();
  } catch (e) {
    console.error(e);
    toast("Couldn't load your pages.", "error");
  } finally {
    app.busy = false;
  }
}

export async function initApp() {
  try {
    const cfg = await api.getConfig();
    app.config = cfg;
    app.initError = null;
    if (cfg.vaultPath) {
      await api.setVault(cfg.vaultPath);
      await reloadPages();
    }
  } catch (e) {
    app.initError = String(e);
  } finally {
    app.initialized = true;
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
      sorted.sort((a, b) => (a.title || "Untitled").localeCompare(b.title || "Untitled"));
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
  if (n === 0) return "Review today";
  return n === 1 ? "1 page to review" : `${n} pages to review`;
}

export function clearFilters() {
  app.folderFilter = null;
  app.tagFilter = null;
}
