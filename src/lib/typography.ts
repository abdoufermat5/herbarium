// The ten type styles offered in Settings → Appearance, and the one function
// that puts a style on the document. The font stacks and tuning live in
// src/app.css under each `[data-type="…"]` block; this file only names the
// styles and carries their translated labels.

import type { MessageKey } from "./locales/en";

export type TypeStyle =
  | "herbarium"
  | "storybook"
  | "fieldnotes"
  | "casual"
  | "poster"
  | "library"
  | "typewriter"
  | "arcade"
  | "playground"
  | "legible";

export interface TypeStyleChoice {
  id: TypeStyle;
  /** Translation key for the style's name. */
  nameKey: MessageKey;
  /** Translation key for its one-line description. */
  blurbKey: MessageKey;
}

export const TYPE_STYLES = [
  { id: "herbarium", nameKey: "type.herbarium", blurbKey: "type.herbariumBlurb" },
  { id: "storybook", nameKey: "type.storybook", blurbKey: "type.storybookBlurb" },
  { id: "fieldnotes", nameKey: "type.fieldnotes", blurbKey: "type.fieldnotesBlurb" },
  { id: "casual", nameKey: "type.casual", blurbKey: "type.casualBlurb" },
  { id: "poster", nameKey: "type.poster", blurbKey: "type.posterBlurb" },
  { id: "library", nameKey: "type.library", blurbKey: "type.libraryBlurb" },
  { id: "typewriter", nameKey: "type.typewriter", blurbKey: "type.typewriterBlurb" },
  { id: "arcade", nameKey: "type.arcade", blurbKey: "type.arcadeBlurb" },
  { id: "playground", nameKey: "type.playground", blurbKey: "type.playgroundBlurb" },
  { id: "legible", nameKey: "type.legible", blurbKey: "type.legibleBlurb" },
] as const satisfies readonly TypeStyleChoice[];

/** The ids, in display order, for validating stored values and keyboard steps. */
export const TYPE_STYLE_IDS: readonly TypeStyle[] = TYPE_STYLES.map((s) => s.id);

/** Text sizes offered in Settings, as a multiplier on every `--fs-*` token. */
export const TEXT_SCALES = [0.92, 1, 1.08, 1.16] as const;
export type TextScale = (typeof TEXT_SCALES)[number];

function isTextScale(scale: number): scale is TextScale {
  return (TEXT_SCALES as readonly number[]).includes(scale);
}

/**
 * Put a type style and text scale on <html>. `data-type` selects the font
 * stack block in app.css; `--fs-scale` multiplies every size token there.
 * Called once at boot and again on every preference change.
 */
export function applyTypography(style: TypeStyle, scale: number) {
  const root = document.documentElement;
  root.dataset.type = style;
  root.style.setProperty("--fs-scale", String(isTextScale(scale) ? scale : 1));
}
