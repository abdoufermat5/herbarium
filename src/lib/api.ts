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
  SavedSearch,
  PageLinks,
  AiExportListing,
  TodaySummary,
  Appearance,
  PageGraph,
  PreviewDigest,
  Highlight,
  HighlightColor,
  HealthIssue,
  LabelColor,
  ReadingPath,
  ProposalSummary,
  AiSettings,
  GithubSettings,
  PublishRecord,
  PublishTarget,
  AiProvider,
  RemixPreset,
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

  /** Change the capture shortcut and watchers; rejects a shortcut the system refuses. */
  setCapture(settings: { captureShortcut: string | null; watchDownloads: boolean; watchClipboard: boolean }): Promise<Config> {
    return invoke("set_capture", { settings });
  },

  /** Save the clipboard's HTML as a page in the Inbox; resolves to its title. */
  saveClipboardPage(): Promise<string> {
    return invoke("save_clipboard_page");
  },

  /** Save a downloaded HTML file the watcher offered; resolves to its title. */
  saveDownload(path: string): Promise<string> {
    return invoke("save_download", { path });
  },

  /** Add the sample pages and their reading path (skipping any already added). */
  addSamples(): Promise<{ added: number; pages: string[]; path: string }> {
    return op("vault.add_samples");
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
  searchPages(query: string, limit?: number): Promise<SearchHit[]> {
    return op("pages.search", limit ? { query, limit } : { query });
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

  /** Pages a page links to, broken links, and pages linking to it. */
  pageLinks(id: string): Promise<PageLinks> {
    return op("pages.links", { id });
  },

  listPaths(): Promise<ReadingPath[]> {
    return op("paths.list");
  },

  createPath(name: string, pages: string[] = []): Promise<ReadingPath> {
    return op("paths.create", { name, pages });
  },

  /** Rename and/or reorder: `pages` is the full list in its new order. */
  updatePath(id: string, patch: { name?: string; pages?: string[] }): Promise<ReadingPath> {
    return op("paths.update", { id, ...patch });
  },

  addToPath(id: string, page: string): Promise<ReadingPath> {
    return op("paths.add_page", { id, page });
  },

  removeFromPath(id: string, page: string): Promise<ReadingPath> {
    return op("paths.remove_page", { id, page });
  },

  deletePath(id: string): Promise<{ deleted: string }> {
    return op("paths.delete", { id });
  },

  /** Linked pages and their links; `all` includes pages without links. */
  pageGraph(all = false): Promise<PageGraph> {
    return op("pages.graph", { all });
  },

  addHighlight(
    page: string,
    h: { quote: string; prefix: string; suffix: string; color: HighlightColor; note?: string },
  ): Promise<{ page: PageMeta; highlights: Highlight[]; highlight: Highlight }> {
    return op("highlights.add", { page, ...h });
  },

  updateHighlight(
    page: string,
    id: string,
    patch: { color?: HighlightColor; note?: string },
  ): Promise<{ page: PageMeta; highlights: Highlight[] }> {
    return op("highlights.update", { page, id, ...patch });
  },

  removeHighlight(page: string, id: string): Promise<{ page: PageMeta; highlights: Highlight[] }> {
    return op("highlights.remove", { page, id });
  },

  pageHealth(id: string): Promise<{ id: string; issues: HealthIssue[] }> {
    return op("health.check", { id });
  },

  vaultHealth(): Promise<{ pages: Array<{ id: string; title: string; folder: string | null; issues: HealthIssue[] }> }> {
    return op("health.check", {});
  },

  /** Inline a page's local files so it stands on its own. */
  inlineAssets(id: string): Promise<PageMeta> {
    return op("health.fix", { id, fix: "inline-assets" });
  },

  /** Current previews by page id, and the pages that need one. */
  listPreviews(): Promise<{ previews: Record<string, PreviewDigest>; missing: string[] }> {
    return op("previews.list");
  },

  savePreview(id: string, digest: PreviewDigest): Promise<unknown> {
    return op("previews.save", { id, digest });
  },

  appearance(): Promise<Appearance> {
    return op("appearance.get");
  },

  setFolderLook(path: string, icon: string | null, color: LabelColor | null): Promise<Appearance> {
    return op("appearance.set_folder", { path, icon, color });
  },

  setTagColor(tag: string, color: LabelColor | null): Promise<Appearance> {
    return op("appearance.set_tag", { tag, color });
  },

  /** Set a page's icon (an emoji); null removes it. */
  setPageIcon(id: string, icon: string | null): Promise<PageMeta> {
    return op("pages.update", { id, icon });
  },

  today(): Promise<TodaySummary> {
    return op("today.summary");
  },

  /** Record that the user opened a page (for Today's suggestions). */
  markRead(id: string): Promise<unknown> {
    return op("reads.mark", { id });
  },

  savedSearches(): Promise<SavedSearch[]> {
    return op("searches.list");
  },

  /** Save (or replace, by name) a search; resolves to every saved search. */
  saveSearch(name: string, query: string): Promise<SavedSearch[]> {
    return op("searches.save", { name, query });
  },

  deleteSearch(name: string): Promise<SavedSearch[]> {
    return op("searches.delete", { name });
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

  /** Write one page as a self-contained HTML file (local assets inlined). */
  exportPageHtml(pageId: string, dest: string): Promise<void> {
    return invoke("export_page_html", { pageId, dest });
  },

  /** Publish the vault (or `folder`) as a static site in a new or empty folder. */
  exportSite(folder: string | null, title: string, destDir: string): Promise<{ pages: number; index: string }> {
    return invoke("export_site", { folder, title, destDir });
  },

  /** List the HTML artifacts in a Claude or ChatGPT export (.zip or conversations.json). */
  scanAiExport(path: string): Promise<AiExportListing> {
    return invoke("scan_ai_export", { path });
  },

  /** Import the chosen artifacts (by key) of the last scanned export. */
  importAiExport(
    keys: string[],
    folder: string | null,
  ): Promise<{ imported: number; skipped: number; errors: string[] }> {
    return invoke("import_ai_export", { keys, folder });
  },

  /** Browsers found and whether the extension's native host is registered with each. */
  browserStatus(): Promise<Array<{ browser: string; connected: boolean }>> {
    return invoke("browser_status");
  },

  /** Register the native messaging host with every browser found; lines describe what was done. */
  connectBrowsers(): Promise<string[]> {
    return invoke("connect_browsers");
  },

  /** Copy the bundled extension to a stable folder, reveal it and return its path. */
  revealExtension(): Promise<string> {
    return invoke("reveal_extension");
  },

  /** Who remixes pages and whether an API key is stored (the key itself stays in the backend). */
  aiSettings(): Promise<AiSettings> {
    return invoke("ai_settings");
  },

  /** `key`: a new Anthropic API key, "" to remove it, or undefined to keep it. */
  setAiSettings(provider: AiProvider, model: string, key?: string): Promise<AiSettings> {
    return invoke("set_ai_settings", { provider, model, key });
  },

  /** Remix a page with the configured model; the result waits as a proposal. */
  remixPage(id: string, preset: RemixPreset, instructions: string): Promise<ProposalSummary> {
    return invoke("remix_page", { id, preset, instructions });
  },

  cancelRemix(): Promise<void> {
    return invoke("cancel_remix");
  },

  /** The GitHub account used to publish and the site's repository. */
  githubSettings(): Promise<GithubSettings> {
    return invoke("github_settings");
  },

  /** `token`: a new token (checked with GitHub), "" to disconnect, or undefined to keep it. */
  setGithub(repo: string, token?: string): Promise<GithubSettings> {
    return invoke("set_github", { token, repo });
  },

  /** Publish a page as a secret gist or on the GitHub Pages site, or take it down. */
  publishPage(id: string, target: PublishTarget, unpublish = false): Promise<PublishRecord | { unpublished: string }> {
    return invoke("publish_page", { id, target, unpublish });
  },

  /** Where pages were published, by page id. */
  publishedList(): Promise<Record<string, Partial<Record<PublishTarget, PublishRecord>>>> {
    return op("published.list");
  },

  /** Put HTML (and its plain-text version) on the clipboard. */
  copyRich(html: string, text: string): Promise<void> {
    return invoke("copy_rich", { html, text });
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
