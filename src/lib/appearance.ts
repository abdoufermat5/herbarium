// Looks of folders and tags, resolved against the vault's appearance settings.
import { app } from "./state.svelte";
import type { LabelColor } from "./types";

export const LABEL_COLORS: LabelColor[] = ["sage", "sky", "plum", "rose", "amber", "clay", "teal", "slate"];

/** A few icons to pick from; any emoji can be typed too. */
export const ICON_CHOICES = [
  "🌿", "🌱", "🌸", "🍄", "📘", "📗", "📕", "📒",
  "🧠", "💡", "🧪", "🔬", "🧮", "📐", "🗺️", "🎨",
  "🦀", "🐍", "☕", "⚙️", "🔧", "📊", "🎵", "⭐",
];

export function tagColor(tag: string): LabelColor | undefined {
  return app.appearance.tags[tag];
}

/** A folder's own look, else the nearest parent folder's colour. */
export function folderLook(path: string | null | undefined): { icon?: string; color?: LabelColor } {
  if (!path) return {};
  const own = app.appearance.folders[path];
  if (own?.color) return own;
  const parts = path.split("/");
  for (let i = parts.length - 1; i > 0; i--) {
    const parent = app.appearance.folders[parts.slice(0, i).join("/")];
    if (parent?.color) return { icon: own?.icon, color: parent.color };
  }
  return own ?? {};
}
