// Device-local preferences beyond theme/layout/sort: editor, startup, import
// and reader behaviour. Stored as one JSON object in localStorage; every
// field is validated on load so a stale or hand-edited value falls back to its
// default instead of breaking the UI.

const KEY = "herbarium.prefs";
const LAST_IMPORT_KEY = "herbarium.lastImportFolder";

export type StartView = "list" | "review";
export type ImportTarget = "browsing" | "root" | "last";
export type PreviewLayout = "right" | "below" | "off";
export type EditorFont = "app" | "system";

export interface Prefs {
  /** Screen shown when the app launches. */
  startView: StartView;
  /** Which folder the import dialog starts on. */
  importTarget: ImportTarget;
  /** Open the page details panel whenever the app starts. */
  detailsOpen: boolean;
  /** Re-read the vault from disk when the window regains focus. */
  refreshOnFocus: boolean;
  /** Desktop notification when a page becomes due for review. */
  reviewNotify: boolean;
  editorFont: EditorFont;
  /** px */
  editorFontSize: number;
  editorLineHeight: number;
  editorWrap: boolean;
  /** Spaces per indent level. */
  editorTabSize: number;
  /** Tab inserts spaces instead of moving focus. */
  editorTabIndents: boolean;
  editorSpellcheck: boolean;
  editorPreview: PreviewLayout;
  /** Detected editor used by "open in editor"; empty picks the first detected. */
  externalEditor: string;
  /** Command template (`code --goto {file}`); overrides `externalEditor` when set. */
  externalEditorCommand: string;
  /** Zoom applied to pages in the reader. */
  readerZoom: number;
}

export const DEFAULT_PREFS: Prefs = {
  startView: "list",
  importTarget: "browsing",
  detailsOpen: false,
  refreshOnFocus: true,
  reviewNotify: true,
  editorFont: "app",
  editorFontSize: 12.5,
  editorLineHeight: 1.6,
  editorWrap: false,
  editorTabSize: 2,
  editorTabIndents: true,
  editorSpellcheck: false,
  editorPreview: "right",
  externalEditor: "",
  externalEditorCommand: "",
  readerZoom: 1,
};

/** Values offered in the settings UI. */
export const EDITOR_FONT_SIZES = [11, 12, 12.5, 13, 14, 15, 16, 18, 20];
export const EDITOR_LINE_HEIGHTS = [1.4, 1.6, 1.85];
export const EDITOR_TAB_SIZES = [2, 4, 8];
export const READER_ZOOMS = [0.5, 0.67, 0.75, 0.85, 1, 1.1, 1.25, 1.5, 1.75, 2];

function oneOf<T>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function text(value: unknown, fallback: string): string {
  return typeof value === "string" && value.length <= 500 ? value : fallback;
}

function sanitize(raw: unknown): Prefs {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_PREFS;
  return {
    startView: oneOf(r.startView, ["list", "review"], d.startView),
    importTarget: oneOf(r.importTarget, ["browsing", "root", "last"], d.importTarget),
    detailsOpen: bool(r.detailsOpen, d.detailsOpen),
    refreshOnFocus: bool(r.refreshOnFocus, d.refreshOnFocus),
    reviewNotify: bool(r.reviewNotify, d.reviewNotify),
    editorFont: oneOf(r.editorFont, ["app", "system"], d.editorFont),
    editorFontSize: oneOf(r.editorFontSize, EDITOR_FONT_SIZES, d.editorFontSize),
    editorLineHeight: oneOf(r.editorLineHeight, EDITOR_LINE_HEIGHTS, d.editorLineHeight),
    editorWrap: bool(r.editorWrap, d.editorWrap),
    editorTabSize: oneOf(r.editorTabSize, EDITOR_TAB_SIZES, d.editorTabSize),
    editorTabIndents: bool(r.editorTabIndents, d.editorTabIndents),
    editorSpellcheck: bool(r.editorSpellcheck, d.editorSpellcheck),
    editorPreview: oneOf(r.editorPreview, ["right", "below", "off"], d.editorPreview),
    externalEditor: text(r.externalEditor, d.externalEditor),
    externalEditorCommand: text(r.externalEditorCommand, d.externalEditorCommand),
    readerZoom: oneOf(r.readerZoom, READER_ZOOMS, d.readerZoom),
  };
}

function load(): Prefs {
  try {
    return sanitize(JSON.parse(localStorage.getItem(KEY) ?? "null"));
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

export const prefs: Prefs = $state(load());

export function setPref<K extends keyof Prefs>(key: K, value: Prefs[K]) {
  prefs[key] = value;
  try {
    localStorage.setItem(KEY, JSON.stringify($state.snapshot(prefs)));
  } catch {
    /* the preference just won't persist */
  }
}

/** The folder the last import went to ("" is the vault root). */
export function lastImportFolder(): string {
  try {
    return localStorage.getItem(LAST_IMPORT_KEY) ?? "";
  } catch {
    return "";
  }
}

export function rememberImportFolder(folder: string) {
  try {
    localStorage.setItem(LAST_IMPORT_KEY, folder);
  } catch {
    /* ignore */
  }
}
