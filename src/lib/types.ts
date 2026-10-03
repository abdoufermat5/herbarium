// Shared types mirroring the Rust backend (serde `rename_all = "camelCase"`).

export interface Config {
  vaultPath: string | null;
  /** Most recent first, at most 8. */
  recentVaults: string[];
  /** Closing the window hides it to the tray instead of quitting. */
  closeToTray: boolean;
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
  ext?: Record<string, unknown> & { review?: ReviewHistory };
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

export interface Page {
  meta: PageMeta;
  html: string;
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

export type ReviewGrade = "again" | "good";

/** One completed review in `meta.ext.review.log`. */
export interface ReviewLogEntry {
  at: number; // unix ms
  grade: ReviewGrade;
  intervalMinutes: number;
}

/** `meta.ext.review`: the last 100 reviews and the lifetime count. */
export interface ReviewHistory {
  count: number;
  log: ReviewLogEntry[];
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
}

/** Output of `check_update`; null when this build is current. */
export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string | null;
}

export interface ImportResult {
  imported: number;
  errors: string[];
}

/** How completing a review picks the next interval. */
export type ReviewStrategy = "same" | "ladder" | "multiply";

/** Every duration is in minutes. */
export interface ReviewSettings {
  /** Preset intervals, ascending. */
  presets: number[];
  strategy: ReviewStrategy;
  multiplier: number;
  maxIntervalMinutes: number;
  /** Schedule imports for review this many minutes out; null = don't. */
  importReviewMinutes: number | null;
  /** Cap on the review queue; null = show everything due. */
  queueLimit: number | null;
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
