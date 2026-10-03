// Text-editing helpers for the HTML source editor. Pure, so the behaviour of
// the Tab key can be checked without a DOM.

export interface Edit {
  value: string;
  /** New selection, as offsets into `value`. */
  start: number;
  end: number;
}

/**
 * Tab / Shift+Tab on the selection `[start, end]` of `value`, indenting by
 * `size` spaces. A collapsed or single-line selection inserts spaces up to the
 * next tab stop; a selection that spans lines indents (or outdents) every line
 * it touches. Outdenting removes up to `size` leading spaces per line.
 */
export function indent(value: string, start: number, end: number, size: number, outdent: boolean): Edit {
  const unit = " ".repeat(size);
  const lineStart = start === 0 ? 0 : value.lastIndexOf("\n", start - 1) + 1;
  const multiline = value.slice(start, end).includes("\n");

  if (!multiline && !outdent) {
    const col = start - lineStart;
    const pad = " ".repeat(size - (col % size));
    const next = value.slice(0, start) + pad + value.slice(end);
    const caret = start + pad.length;
    return { value: next, start: caret, end: caret };
  }

  // Work on whole lines from the start of the first to the end of the last.
  const blockEnd = end > start && value[end - 1] === "\n" ? end - 1 : end;
  const nl = value.indexOf("\n", blockEnd);
  const lineEnd = nl === -1 ? value.length : nl;
  const lines = value.slice(lineStart, lineEnd).split("\n");

  let firstDelta = 0;
  let totalDelta = 0;
  const changed = lines.map((line, i) => {
    let delta: number;
    let out: string;
    if (outdent) {
      const strip = Math.min(size, line.length - line.trimStart().length);
      out = line.slice(strip);
      delta = -strip;
    } else {
      out = line.length === 0 ? line : unit + line;
      delta = out.length - line.length;
    }
    if (i === 0) firstDelta = delta;
    totalDelta += delta;
    return out;
  });

  const next = value.slice(0, lineStart) + changed.join("\n") + value.slice(lineEnd);
  const newStart = Math.max(lineStart, start + firstDelta);
  return { value: next, start: newStart, end: Math.max(newStart, end + totalDelta) };
}

/**
 * Apply `edit` to a textarea as one undoable step. Only the span that actually
 * changed is replaced, so the scroll position stays put.
 */
export function applyToTextarea(ta: HTMLTextAreaElement, edit: Edit) {
  const old = ta.value;
  let from = 0;
  while (from < old.length && from < edit.value.length && old[from] === edit.value[from]) from++;
  let oldEnd = old.length;
  let newEnd = edit.value.length;
  while (oldEnd > from && newEnd > from && old[oldEnd - 1] === edit.value[newEnd - 1]) {
    oldEnd--;
    newEnd--;
  }
  const insert = edit.value.slice(from, newEnd);
  ta.setSelectionRange(from, oldEnd);
  // execCommand keeps the browser's undo stack; fall back when it is unavailable.
  if (!document.execCommand("insertText", false, insert)) {
    ta.setRangeText(insert, from, oldEnd, "end");
    ta.dispatchEvent(new Event("input", { bubbles: true }));
  }
  ta.setSelectionRange(edit.start, edit.end);
}
