// Single source of truth for the keyboard reference shown by ShortcutsDialog
// (and reusable by Settings). Entries describe keys that exist in the app:
// App.svelte (window handler), HtmlEditor (CodeMirror keymaps), the sidebar
// tree, the page list, the command palette and the review session.
// Modifier chords go through `modKey` so macOS shows ⌘ and other platforms Ctrl.

import { modKey } from "./format";
import { t, type MessageKey } from "./i18n.svelte";

export type ShortcutSectionId = "application" | "search" | "navigation" | "review" | "editor";

/** Where a shortcut is live; shown as a caption and used to pick the section to open on. */
export type ShortcutContext = "global" | "sidebar" | "list" | "search" | "review" | "reader" | "editor";

/**
 * One physical key combination.
 * - `string`: a literal key cap ("F2", "↑", "Esc").
 * - `{ mod }`: the platform modifier plus a key — "⌘K" / "Ctrl K"; `mod` may carry
 *   extra modifiers ("Shift+Z").
 * - `{ click }`: a pointer chord — modifier + click.
 */
export type Chord = string | { mod: string } | { click: "mod" | "shift" };

export interface ShortcutDef {
  id: string;
  section: ShortcutSectionId;
  context: ShortcutContext;
  action: MessageKey;
  /** Alternatives; any one triggers the action. */
  keys: readonly Chord[];
}

export interface ResolvedShortcut {
  id: string;
  section: ShortcutSectionId;
  context: ShortcutContext;
  contextLabel: string;
  action: string;
  /** Rendered key caps, one entry per alternative. */
  keys: string[];
}

export interface ResolvedSection {
  id: ShortcutSectionId;
  title: string;
  items: ResolvedShortcut[];
}

export const SHORTCUT_SECTION_ORDER: readonly ShortcutSectionId[] = [
  "application",
  "search",
  "navigation",
  "review",
  "editor",
];

const SECTION_TITLE: Record<ShortcutSectionId, MessageKey> = {
  application: "shortcuts.section.application",
  search: "shortcuts.section.search",
  navigation: "shortcuts.section.navigation",
  review: "shortcuts.section.review",
  editor: "shortcuts.section.editor",
};

const CONTEXT_LABEL: Record<ShortcutContext, MessageKey> = {
  global: "shortcuts.context.global",
  sidebar: "shortcuts.context.sidebar",
  list: "shortcuts.context.list",
  search: "shortcuts.context.search",
  review: "shortcuts.context.review",
  reader: "shortcuts.context.reader",
  editor: "shortcuts.context.editor",
};

const CONTEXT_SECTION: Record<ShortcutContext, ShortcutSectionId> = {
  global: "application",
  sidebar: "navigation",
  list: "navigation",
  search: "search",
  review: "review",
  reader: "editor",
  editor: "editor",
};

export const SHORTCUTS: readonly ShortcutDef[] = [
  // Application — App.svelte window handler
  { id: "help", section: "application", context: "global", action: "shortcuts.action.help", keys: ["?"] },
  { id: "palette", section: "application", context: "global", action: "shortcuts.action.palette", keys: [{ mod: "K" }] },
  { id: "sidebar", section: "application", context: "global", action: "shortcuts.action.sidebar", keys: [{ mod: "B" }] },
  { id: "settings", section: "application", context: "global", action: "shortcuts.action.settings", keys: [{ mod: "," }] },
  { id: "import", section: "application", context: "list", action: "shortcuts.action.import", keys: ["I"] },
  { id: "escape", section: "application", context: "global", action: "shortcuts.action.escape", keys: ["Esc"] },

  // Search — page search field and command palette
  { id: "search", section: "search", context: "list", action: "shortcuts.action.search", keys: ["/"] },
  { id: "quickSwitch", section: "search", context: "search", action: "shortcuts.action.quickSwitch", keys: [{ mod: "K" }] },
  { id: "navigateResults", section: "search", context: "search", action: "shortcuts.action.navigateResults", keys: ["↑", "↓"] },
  { id: "openResult", section: "search", context: "search", action: "shortcuts.action.openResult", keys: ["Enter"] },
  { id: "clearSearch", section: "search", context: "search", action: "shortcuts.action.clearSearch", keys: ["Esc"] },

  // Navigation — sidebar tree and page list
  { id: "moveSelection", section: "navigation", context: "sidebar", action: "shortcuts.action.moveSelection", keys: ["↑", "↓"] },
  { id: "expandCollapse", section: "navigation", context: "sidebar", action: "shortcuts.action.expandCollapse", keys: ["→", "←"] },
  { id: "jumpEnds", section: "navigation", context: "sidebar", action: "shortcuts.action.jumpEnds", keys: ["Home", "End"] },
  { id: "openItem", section: "navigation", context: "sidebar", action: "shortcuts.action.openItem", keys: ["Enter"] },
  { id: "renameItem", section: "navigation", context: "sidebar", action: "shortcuts.action.renameItem", keys: ["F2"] },
  { id: "deleteItem", section: "navigation", context: "sidebar", action: "shortcuts.action.deleteItem", keys: ["Delete"] },
  { id: "rangeSelect", section: "navigation", context: "list", action: "shortcuts.action.rangeSelect", keys: [{ click: "shift" }] },
  { id: "multiSelect", section: "navigation", context: "list", action: "shortcuts.action.multiSelect", keys: [{ click: "mod" }] },

  // Review session
  { id: "reviewGood", section: "review", context: "review", action: "shortcuts.action.reviewGood", keys: ["2"] },
  { id: "reviewAgain", section: "review", context: "review", action: "shortcuts.action.reviewAgain", keys: ["1"] },
  { id: "reviewSkip", section: "review", context: "review", action: "shortcuts.action.reviewSkip", keys: ["S"] },
  { id: "exitReview", section: "review", context: "review", action: "shortcuts.action.exitReview", keys: ["Esc"] },

  // Reader and HTML editor
  { id: "inspector", section: "editor", context: "reader", action: "shortcuts.action.inspector", keys: ["I"] },
  { id: "exitReader", section: "editor", context: "reader", action: "shortcuts.action.exitReader", keys: ["Esc"] },
  { id: "save", section: "editor", context: "editor", action: "shortcuts.action.save", keys: [{ mod: "S" }] },
  { id: "undo", section: "editor", context: "editor", action: "shortcuts.action.undo", keys: [{ mod: "Z" }] },
  { id: "redo", section: "editor", context: "editor", action: "shortcuts.action.redo", keys: [{ mod: "Shift+Z" }, { mod: "Y" }] },
  { id: "find", section: "editor", context: "editor", action: "shortcuts.action.find", keys: [{ mod: "F" }] },
];

/** Render one chord for the current platform and locale. */
export function chordLabel(chord: Chord): string {
  if (typeof chord === "string") return chord;
  if ("mod" in chord) return modKey(chord.mod);
  const prefix = chord.click === "shift" ? "Shift" : modKey("").trim();
  return `${prefix}+${t("shortcuts.key.click")}`;
}

function resolve(def: ShortcutDef): ResolvedShortcut {
  return {
    id: def.id,
    section: def.section,
    context: def.context,
    contextLabel: t(CONTEXT_LABEL[def.context]),
    action: t(def.action),
    keys: def.keys.map(chordLabel),
  };
}

/** Section that documents `context` — what the dialog opens on from that screen. */
export function sectionForContext(context: ShortcutContext): ShortcutSectionId {
  return CONTEXT_SECTION[context];
}

/** All shortcuts grouped by section, in display order. Reads the locale, so call inside `$derived`/templates. */
export function shortcutSections(): ResolvedSection[] {
  return SHORTCUT_SECTION_ORDER.map((id) => ({
    id,
    title: t(SECTION_TITLE[id]),
    items: SHORTCUTS.filter((s) => s.section === id).map(resolve),
  }));
}

/** Rendered key caps for one shortcut id (e.g. a Settings row or tooltip). */
export function shortcutKeys(id: string): string[] {
  const def = SHORTCUTS.find((s) => s.id === id);
  return def ? def.keys.map(chordLabel) : [];
}

/** Single-line hint such as "Ctrl K" or "↑ / ↓". */
export function shortcutHint(id: string): string {
  return shortcutKeys(id).join(" / ");
}
