// Shared types mirroring the Rust backend (serde `rename_all = "camelCase"`).

export interface Config {
  vaultPath: string | null;
}

export interface PageMeta {
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
}

export interface Page {
  meta: PageMeta;
  html: string;
}

export interface TagCount {
  tag: string;
  count: number;
}

export interface MetaPatch {
  title: string;
  tags: string[];
  folder: string | null;
  note: string;
}

export interface ImportFile {
  name: string | null;
  content: string;
}

export interface ImportResult {
  imported: number;
  errors: string[];
}