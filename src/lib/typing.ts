/** True when keyboard input belongs to a text control (or the source editor), so
 *  single-key shortcuts must stand down. */
export function isTypingTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable) return true;
  return typeof el.closest === "function" && !!el.closest(".cm-editor");
}
