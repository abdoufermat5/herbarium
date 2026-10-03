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
  intervalDays: number | null;
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

export interface IndexReport {
  indexed: number;
  removed: number;
  total: number;
}
