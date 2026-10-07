import { invoke } from "@tauri-apps/api/core";
import { i18n } from "./i18n.svelte";
import type {
  BulkDeleteResult,
  BulkPatch,
  BulkUpdateResult,
  Config,
  EditorInfo,
  HistoryEntry,
  AgentSettings,
  ProposalSummary,
  ImportFile,
  ImportResult,
  IndexReport,
  MetaPatch,
  NetworkSettings,
  Page,
  PageMeta,
  PageStorage,
  ReviewGrade,
  ReviewPreview,
  ReviewSettings,
  ReviewStats,
  SearchHit,
  StorageChange,
  TagCount,
  TrashEntry,
  UpdateChannel,
  UpdateInfo,
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

  /** Drop a vault from the recent list (the folder itself is untouched). */
  removeRecentVault(path: string): Promise<Config> {
    return invoke("remove_recent_vault", { path });
  },

  setCloseToTray(enabled: boolean): Promise<Config> {
    return invoke("set_close_to_tray", { enabled });
  },

  /** Invoke only after the shared unsaved-work navigation guards allow exit. */
  quitApp(): Promise<void> {
    return invoke("quit_app");
  },

  /** `folder` null imports to the vault root; `allowCdn` undefined follows the vault default. */
  importFiles(
    files: ImportFile[],
    folder: string | null = null,
    allowCdn?: boolean,
  ): Promise<ImportResult> {
    return op("pages.import", { files, folder, allowCdn });
  },

  listPages(): Promise<PageMeta[]> {
    return op("pages.list");
  },

  /** Ranked full-text hits (best first) with optional `[match]` snippets. */
  searchPages(query: string): Promise<SearchHit[]> {
    return op("pages.search", { query });
  },

  getPage(id: string): Promise<Page> {
    return op("pages.get", { id, format: "html" });
  },

  /** `baseUpdatedAt` makes a stale write fail with a `conflict:` error. */
  updatePageMeta(id: string, patch: MetaPatch, baseUpdatedAt?: number): Promise<PageMeta> {
    return op("pages.update", { id, ...patch, baseUpdatedAt });
  },

  scheduleReview(id: string, intervalMinutes: number): Promise<PageMeta> {
    return op("review.schedule", { id, intervalMinutes });
  },

  clearReview(id: string): Promise<PageMeta> {
    return op("review.clear", { id });
  },

  /** Record a review. Under FSRS `hard`/`good`/`easy` follow the memory model;
   *  `again` restarts at the first preset. `review.preview` shows the intervals. */
  completeReview(id: string, grade: ReviewGrade = "good"): Promise<PageMeta> {
    return op("review.complete", { id, grade });
  },

  /** Interval in minutes each grade would schedule right now; no writes. */
  previewReview(id: string): Promise<ReviewPreview> {
    return op("review.preview", { id });
  },

  reviewSettings(): Promise<ReviewSettings> {
    return op("review.settings");
  },

  /** Replaces all review settings; rejects out-of-range values. */
  configureReview(settings: ReviewSettings): Promise<ReviewSettings> {
    return op("review.configure", { ...settings });
  },

  reviewStats(): Promise<ReviewStats> {
    return op("review.stats");
  },

  setNetwork(id: string, allowCdn: boolean): Promise<PageMeta> {
    return op("network.set", { id, allowCdn });
  },

  networkSettings(): Promise<NetworkSettings> {
    return op("network.settings");
  },

  configureNetwork(defaultAllowCdn: boolean): Promise<NetworkSettings> {
    return op("network.configure", { defaultAllowCdn });
  },

  /** Move one page to the trash. */
  deletePage(id: string): Promise<{ deleted: string }> {
    return op("pages.delete", { id });
  },

  /** Move pages to the trash; failures are listed per page. */
  trashPages(ids: string[]): Promise<BulkDeleteResult> {
    return op("pages.bulk_delete", { ids });
  },

  listTrash(): Promise<TrashEntry[]> {
    return op("pages.trash");
  },

  restorePage(id: string): Promise<PageMeta> {
    return op("pages.restore", { id });
  },

  /** Permanently delete one trashed page, or empty the trash when `id` is omitted. */
  purgePages(id?: string): Promise<{ removed: number }> {
    return op("pages.purge", { id });
  },

  /** Saved HTML versions for a page, newest first. */
  listHistory(id: string): Promise<HistoryEntry[]> {
    return op("history.list", { id });
  },

  /** One saved version's HTML. */
  getHistory(id: string, at: number): Promise<{ html: string }> {
    return op("history.get", { id, at });
  },

  /** Restore a saved version; `expectedUpdatedAt` rejects a stale overwrite with `conflict:`. */
  restoreHistory(id: string, at: number, expectedUpdatedAt?: number): Promise<PageMeta> {
    return op("history.restore", { id, at, expectedUpdatedAt });
  },

  /** Agent edits waiting for approval, newest first. */
  listProposals(): Promise<ProposalSummary[]> {
    return op("proposals.list");
  },

  /** The HTML an agent proposed for a page. */
  getProposal(id: string): Promise<{ proposal: ProposalSummary; html: string }> {
    return op("proposals.get", { id });
  },

  /** Apply a proposal; without `force` a page changed since rejects with `conflict:`. */
  acceptProposal(id: string, force = false): Promise<PageMeta> {
    return op("proposals.accept", { id, force });
  },

  rejectProposal(id: string): Promise<{ rejected: string }> {
    return op("proposals.reject", { id });
  },

  agentSettings(): Promise<AgentSettings> {
    return op("agents.settings");
  },

  configureAgents(settings: AgentSettings): Promise<AgentSettings> {
    return op("agents.configure", { ...settings });
  },

  duplicatePage(id: string): Promise<PageMeta> {
    return op("pages.duplicate", { id });
  },

  /** Move and/or retag several pages; failures are listed per page. */
  bulkUpdate(ids: string[], patch: BulkPatch): Promise<BulkUpdateResult> {
    return op("pages.bulk_update", { ids, ...patch });
  },

  reviewToday(): Promise<PageMeta[]> {
    return op("review.due");
  },

  tags(): Promise<TagCount[]> {
    return op("tags.list");
  },

  renameTag(from: string, to: string): Promise<{ updated: number }> {
    return op("tags.rename", { from, to });
  },

  deleteTag(tag: string): Promise<{ updated: number }> {
    return op("tags.delete", { tag });
  },

  folders(): Promise<string[]> {
    return op("folders.list");
  },

  /** `baseUpdatedAt` makes a stale write fail with a `conflict:` error. */
  setPageHtml(id: string, html: string, baseUpdatedAt?: number): Promise<PageMeta> {
    return op("pages.set_html", { id, html, baseUpdatedAt });
  },

  /** The state an interactive page saved for itself (`localStorage`/`window.storage`). */
  getStorage(id: string): Promise<PageStorage> {
    return op("storage.get", { id });
  },

  /** Apply page-state changes atomically; resolves to the page's new state. */
  writeStorage(id: string, changes: StorageChange[]): Promise<PageStorage> {
    return op("storage.write", { id, changes });
  },

  /** Erase everything an interactive page saved for itself. */
  clearStorage(id: string): Promise<PageStorage> {
    return op("storage.clear", { id });
  },

  createFolder(path: string): Promise<string> {
    return op("folders.create", { path });
  },

  /** Rename or move a folder (with its pages and subfolders); resolves to the new path. */
  renameFolder(from: string, to: string): Promise<{ folder: string }> {
    return op("folders.rename", { from, to });
  },

  /** Without `withPages` a folder that still holds pages is refused. */
  deleteFolder(path: string, withPages = false): Promise<{ trashed: number }> {
    return op("folders.delete", { path, withPages });
  },

  /** A new page from a blank HTML skeleton titled `title`, in the current UI language. */
  createPage(title: string, folder: string | null): Promise<PageMeta> {
    const esc = title.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    const html = `<!doctype html>\n<html lang="${i18n.locale}">\n<head>\n<meta charset="utf-8">\n<title>${esc}</title>\n</head>\n<body>\n<h1>${esc}</h1>\n</body>\n</html>\n`;
    return op("pages.create", { html, title, folder });
  },

  /** Re-read the vault from disk; unreadable files are listed in `skipped`. */
  rescan(): Promise<IndexReport> {
    return op("vault.rescan");
  },

  /** Show the page's HTML file in the system file manager. */
  revealPage(pageId: string): Promise<void> {
    return invoke("reveal_page", { pageId });
  },

  revealVault(): Promise<void> {
    return invoke("reveal_vault");
  },

  /** Zip the vault (index and trash excluded) to `destZip`. */
  exportVault(destZip: string): Promise<void> {
    return invoke("export_vault", { destZip });
  },

  /** Zip one page with its sibling assets to `destZip`. */
  exportPage(pageId: string, destZip: string): Promise<void> {
    return invoke("export_page", { pageId, destZip });
  },

  /** A newer signed release, or null when this build is current. */
  checkUpdate(): Promise<UpdateInfo | null> {
    return invoke("check_update");
  },

  /** Download, verify and install exactly `version`, then restart the app. */
  installUpdate(version: string): Promise<void> {
    return invoke("install_update", { version });
  },

  /** `"snap"` when the Snap Store owns updates, otherwise null. */
  updateManagedBy(): Promise<UpdateChannel | null> {
    return invoke("update_managed_by");
  },

  /** Text editors installed on this system, best candidates first. */
  listEditors(): Promise<EditorInfo[]> {
    return invoke("list_editors");
  },

  /** Open the page's HTML file in an external editor; resolves to the editor's name. */
  openInEditor(pageId: string, editor: string | null, custom: string | null): Promise<string> {
    return invoke("open_in_editor", { pageId, editor, custom });
  },

  /** Open an http(s) link in the system browser. */
  openExternal(url: string): Promise<void> {
    return invoke("open_external", { url });
  },
};
