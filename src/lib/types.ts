// Shared types mirroring the Rust backend (serde `rename_all = "camelCase"`).

export interface Config {
  vaultPath: string | null;
  /** Most recent first, at most 8. */
  recentVaults: string[];
  /** Closing the window hides it to the tray instead of quitting. */
  closeToTray: boolean;
  /** Global shortcut that saves the clipboard's HTML; null turns it off. */
  captureShortcut: string | null;
  /** Offer to save HTML files that appear in the Downloads folder. */
  watchDownloads: boolean;
  /** Offer to save whole HTML pages when they are copied. */
  watchClipboard: boolean;
}

/** An HTML page offered by the Downloads or clipboard watcher. */
export interface CaptureOffer {
  title: string;
  bytes: number;
  /** Set for a downloaded file. */
  path?: string;
  name?: string;
}

export interface PageMeta {
  schemaVersion: number;
  id: string;
  title: string;
  tags: string[];
  folder: string | null;
  note: string;
  createdAt: number; // unix ms
  updatedAt: number; // unix ms
  /** Current review interval, in minutes. */
  intervalMinutes: number | null;
  nextReview: number | null; // unix ms
  lastReview: number | null; // unix ms
  allowCdn: boolean;
  /** The `<title>` extracted from the HTML at its last write or index. */
  sourceTitle?: string | null;
  /** Extension-owned data keyed by extension id; absent when empty. */
  ext?: Record<string, unknown> & {
    review?: ReviewHistory;
    source?: PageSource;
    look?: { icon?: string };
    highlights?: Highlight[];
  };
}

/** Where a page came from (`ext.source`); every field is optional. */
export interface PageSource {
  /** http(s) address the page was saved from. */
  url?: string;
  /** Tool or model that generated the page. */
  tool?: string;
  /** The request that produced the page. */
  prompt?: string;
}

/** A full-text hit: the page plus a `[match]`-marked excerpt of its text, when the match was there. */
export interface SearchHit extends PageMeta {
  snippet?: string;
}

/** A page in the trash. */
export interface TrashEntry extends PageMeta {
  /** unix ms */
  deletedAt: number;
}

/** One saved version of a page's HTML (`.herbarium/history/<id>/`). */
export interface HistoryEntry {
  /** When the snapshot was taken, unix ms (its `at` key for get/restore). */
  at: number;
  /** Who made the change the snapshot preserved. */
  caller: "ui" | "agent";
  bytes: number;
}

/** An agent's rewrite of a page waiting for approval (`proposals.list`). */
export interface ProposalSummary {
  /** The page id. */
  id: string;
  /** When the agent proposed it, unix ms. */
  at: number;
  baseUpdatedAt: number;
  /** The `<title>` of the proposed HTML. */
  title: string;
  /** The page's current title. */
  pageTitle: string;
  bytes: number;
  /** The page changed after the agent read it. */
  stale: boolean;
}

/** A page's links (`pages.links`). */
export interface PageLinks {
  /** Pages this page links to. */
  links: PageMeta[];
  /** Linked ids with no page in the vault. */
  broken: string[];
  /** Pages linking to this page. */
  backlinks: PageMeta[];
}

/** A reading path: pages read in order (`.herbarium/paths.json`). */
export interface ReadingPath {
  id: string;
  name: string;
  description?: string;
  /** Page ids in reading order; a trashed page keeps its place. */
  pages: string[];
}

/** An HTML artifact found in a Claude or ChatGPT data export. */
export interface AiExportCandidate {
  key: string;
  title: string;
  bytes: number;
  createdAt: number;
  tool: string;
  url: string;
  prompt: string;
  conversation: string;
  alreadyImported: boolean;
}

export interface AiExportListing {
  candidates: AiExportCandidate[];
  conversations: number;
  /** Artifacts that are not standalone pages (React components, code…). */
  unsupported: number;
}

/** `today.summary`: what to do with the library now. */
export interface TodaySummary {
  dueTotal: number;
  due: PageMeta[];
  continue: Array<{ pathId: string; pathName: string; position: number; total: number; page: PageMeta }>;
  recent: PageMeta[];
  rediscover: PageMeta | null;
  onThisDay: PageMeta[];
  streak: number;
  totalPages: number;
}

/** Label colours offered for folders and tags (see app.css `--c-*`). */
export type LabelColor = "sage" | "sky" | "plum" | "rose" | "amber" | "clay" | "teal" | "slate";

/** Icons and colours of folders and tags (`.herbarium/appearance.json`). */
export interface Appearance {
  folders: Record<string, { icon?: string; color?: LabelColor }>;
  tags: Record<string, LabelColor>;
}

/** `pages.graph`: linked pages and their links. */
export interface PageGraph {
  nodes: Array<{ id: string; title: string; folder: string | null; degree: number }>;
  /** `[from, to]` page ids. */
  edges: Array<[string, string]>;
}

/** A page's first-screen layout, drawn as its miniature (see previews.rs). */
export interface PreviewDigest {
  w: number;
  h: number;
  bg: string;
  blocks: Array<{ k: "box" | "text" | "img"; x: number; y: number; w: number; h: number; c?: string; r?: number; s?: number; lh?: number }>;
  image?: { x: number; y: number; w: number; h: number; src: string };
}

/** A highlighted passage of a page, anchored by its text and context. */
export interface Highlight {
  id: string;
  quote: string;
  prefix: string;
  suffix: string;
  color: HighlightColor;
  note?: string;
  at: number;
}

export type HighlightColor = "yellow" | "green" | "blue" | "pink";

/** Something that may stop a page from working (`health.check`). */
export interface HealthIssue {
  kind: "missing-asset" | "needs-network" | "blocked-host" | "broken-links" | "local-assets" | "large" | "untitled";
  level: "error" | "warn" | "info";
  items?: string[];
  fix?: "enable-network" | "inline-assets";
}

/** A named search (`.herbarium/searches.json`); `query` may include filters. */
export interface SavedSearch {
  name: string;
  query: string;
}

/** Vault-wide agent settings (`.herbarium/agents.json`). */
export interface AgentSettings {
  /** Agent rewrites of a page's HTML wait for approval. */
  reviewEdits: boolean;
}

export interface Page {
  meta: PageMeta;
  html: string;
}

/** Namespace of a page's saved state; `localStorage` uses `local`. */
export type StorageArea = "local" | "personal" | "shared";

/** One `storage.write` change; `value: null` deletes the key. */
export interface StorageChange {
  area: StorageArea;
  key: string;
  value: string | null;
}

/** The state an interactive page saved for itself, keyed by namespace. */
export interface PageStorage {
  local: Record<string, string>;
  personal: Record<string, string>;
  shared: Record<string, string>;
}

export interface TagCount {
  tag: string;
  count: number;
}

/** Fields left out stay unchanged; `folder: null` moves to the vault root. */
export interface MetaPatch {
  title?: string;
  tags?: string[];
  folder?: string | null;
  note?: string;
}

export interface ImportFile {
  name: string | null;
  content: string;
}

/** One page of a bulk operation that failed. */
export interface BulkError {
  id: string;
  error: string;
}

export interface BulkUpdateResult {
  updated: PageMeta[];
  errors: BulkError[];
}

export interface BulkDeleteResult {
  deleted: string[];
  errors: BulkError[];
}

/** Bulk edit: `folder` undefined leaves pages in place, `null` moves them to the root. */
export interface BulkPatch {
  folder?: string | null;
  addTags?: string[];
  removeTags?: string[];
}

export interface NetworkSettings {
  /** Whether newly created or imported pages may load allowlisted CDNs. */
  defaultAllowCdn: boolean;
}

export type ReviewGrade = "again" | "hard" | "good" | "easy";

/** Interval (minutes) each grade would schedule right now, from `review.preview`. */
export interface ReviewPreview {
  again: number;
  hard: number;
  good: number;
  easy: number;
}

/** One completed review in `meta.ext.review.log`. */
export interface ReviewLogEntry {
  at: number; // unix ms
  grade: ReviewGrade;
  intervalMinutes: number;
}

/** `meta.ext.review`: the last 100 reviews and the lifetime count, plus the
 *  FSRS memory state (`stability` in days, `difficulty` 1..10) when adaptive
 *  scheduling has run. */
export interface ReviewHistory {
  count: number;
  log: ReviewLogEntry[];
  stability?: number | null;
  difficulty?: number | null;
}

export interface ReviewDay {
  /** UTC date, YYYY-MM-DD. */
  day: string;
  count: number;
}

/** Output of `review.stats`. */
export interface ReviewStats {
  /** Every page due now, ignoring the queue limit. */
  dueTotal: number;
  /** Due before today (UTC). */
  overdue: number;
  /** Reviews completed since UTC midnight. */
  reviewedToday: number;
  /** Pages coming due on each of the next 14 UTC days. */
  upcoming: ReviewDay[];
  /** All completed reviews. */
  totalReviews: number;
  /** Reviews on each of the last 365 UTC days, oldest first (last = today). */
  activity: ReviewDay[];
  /** Consecutive days with a review up to today. */
  streak: number;
  longestStreak: number;
}

/** Output of `check_update`; null when this build is current. */
export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string | null;
}

/** Who owns updates for this build. `"snap"`: the Snap Store installs them. */
export type UpdateChannel = "snap";

export interface ImportResult {
  imported: number;
  errors: string[];
}

/** How completing a review picks the next interval. */
export type ReviewStrategy = "same" | "ladder" | "multiply" | "fsrs";

/** Every duration is in minutes. */
export interface ReviewSettings {
  /** Preset intervals, ascending. */
  presets: number[];
  strategy: ReviewStrategy;
  multiplier: number;
  maxIntervalMinutes: number;
  /** Target recall probability for the `fsrs` strategy (0.70..=0.97). */
  desiredRetention: number;
  /** Schedule imports for review this many minutes out; null = don't. */
  importReviewMinutes: number | null;
  /** Cap on the review queue; null = show everything due. */
  queueLimit: number | null;
  /** Folders (with subfolders) kept out of the queue, due counts and reminders. */
  excludeFolders: string[];
  /** Tags whose pages are kept out of the queue, due counts and reminders. */
  excludeTags: string[];
}

/** A file the rescan could not index. */
export interface SkippedFile {
  /** Relative to the vault. */
  path: string;
  reason: string;
}

export interface IndexReport {
  indexed: number;
  removed: number;
  total: number;
  skipped: SkippedFile[];
}

/** A text editor detected on the system. */
export interface EditorInfo {
  id: string;
  name: string;
}
