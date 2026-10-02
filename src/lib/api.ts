import { invoke } from "@tauri-apps/api/core";
import type {
  Config,
  ImportFile,
  ImportResult,
  MetaPatch,
  Page,
  PageMeta,
  TagCount,
} from "./types";

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

  importFiles(files: ImportFile[]): Promise<ImportResult> {
    return invoke("import_files", { files });
  },

  listPages(): Promise<PageMeta[]> {
    return invoke("list_pages");
  },

  searchPages(query: string): Promise<PageMeta[]> {
    return invoke("search_pages", { query });
  },

  getPage(id: string): Promise<Page> {
    return invoke("get_page", { id });
  },

  updatePageMeta(id: string, patch: MetaPatch): Promise<PageMeta> {
    return invoke("update_page_meta", { id, patch });
  },

  scheduleReview(id: string, intervalDays: number): Promise<PageMeta> {
    return invoke("schedule_review", { id, intervalDays });
  },

  clearReview(id: string): Promise<PageMeta> {
    return invoke("clear_review", { id });
  },

  setNetwork(id: string, allowCdn: boolean): Promise<PageMeta> {
    return invoke("set_network", { id, allowCdn });
  },

  deletePage(id: string): Promise<void> {
    return invoke("delete_page", { id });
  },

  reviewToday(): Promise<PageMeta[]> {
    return invoke("review_today");
  },

  tags(): Promise<TagCount[]> {
    return invoke("tags");
  },

  folders(): Promise<string[]> {
    return invoke("folders");
  },
};