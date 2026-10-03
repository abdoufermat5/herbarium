import { invoke } from "@tauri-apps/api/core";
import type {
  Config,
  ImportFile,
  ImportResult,
  IndexReport,
  MetaPatch,
  Page,
  PageMeta,
  ReviewSettings,
  TagCount,
} from "./types";

/** Run an operation registered by a backend extension (`area.verb`). */
export function op<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
  return invoke<T>("invoke_op", { name, args });
}

export const api = {
  getConfig(): Promise<Config> {
    return invoke("get_config");
  },

  setVault(path: string): Promise<Config> {
    return invoke("set_vault", { path });
  },

  createVault(parentDir: string, name: string): Promise<Config> {
    return invoke("create_vault", { parentDir, name });
  },

  /** `folder` null imports to the vault root. */
  importFiles(files: ImportFile[], folder: string | null = null): Promise<ImportResult> {
    return op("pages.import", { files, folder });
  },

  listPages(): Promise<PageMeta[]> {
    return op("pages.list");
  },

  searchPages(query: string): Promise<PageMeta[]> {
    return op("pages.search", { query });
  },

  getPage(id: string): Promise<Page> {
    return op("pages.get", { id, format: "html" });
  },

  updatePageMeta(id: string, patch: MetaPatch): Promise<PageMeta> {
    return op("pages.update", { id, ...patch });
  },

  scheduleReview(id: string, intervalMinutes: number): Promise<PageMeta> {
    return op("review.schedule", { id, intervalMinutes });
  },

  clearReview(id: string): Promise<PageMeta> {
    return op("review.clear", { id });
  },

  /** Record a review and schedule the next one with the vault's strategy. */
  completeReview(id: string): Promise<PageMeta> {
    return op("review.complete", { id });
  },

  reviewSettings(): Promise<ReviewSettings> {
    return op("review.settings");
  },

  /** Replaces all review settings; rejects out-of-range values. */
  configureReview(settings: ReviewSettings): Promise<ReviewSettings> {
    return op("review.configure", { ...settings });
  },

  setNetwork(id: string, allowCdn: boolean): Promise<PageMeta> {
    return op("network.set", { id, allowCdn });
  },

  deletePage(id: string): Promise<void> {
    return op("pages.delete", { id });
  },

  reviewToday(): Promise<PageMeta[]> {
    return op("review.due");
  },

  tags(): Promise<TagCount[]> {
    return op("tags.list");
  },

  folders(): Promise<string[]> {
    return op("folders.list");
  },

  setPageHtml(id: string, html: string): Promise<PageMeta> {
    return op("pages.set_html", { id, html });
  },

  createFolder(path: string): Promise<string> {
    return op("folders.create", { path });
  },

  /** A new page from a blank HTML skeleton titled `title`. */
  createPage(title: string, folder: string | null): Promise<PageMeta> {
    const esc = title.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    const html = `<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<title>${esc}</title>\n</head>\n<body>\n<h1>${esc}</h1>\n</body>\n</html>\n`;
    return op("pages.create", { html, title, folder });
  },

  rescan(): Promise<IndexReport> {
    return op("vault.rescan");
  },
};
