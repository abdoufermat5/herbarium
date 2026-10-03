// Shared types mirroring the Rust backend (serde `rename_all = "camelCase"`).

export interface Config {
  vaultPath: string | null;
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
  /** Extension-owned data keyed by extension id; absent when empty. */
  ext?: Record<string, unknown>;
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

export interface IndexReport {
  indexed: number;
  removed: number;
  total: number;
}
