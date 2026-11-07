// Row arithmetic for a windowed list: rows may differ in height, so each
// row's top edge is a prefix sum of the measured heights (an estimate stands
// in for rows that were never on screen) plus the gap after every row.

/** `offsets[i]` is the top of row `i`; `offsets[rows]` the total including the last gap. */
export function buildOffsets(rows: number, heights: ArrayLike<number>, estimate: number, gap: number): Float64Array {
  const offsets = new Float64Array(rows + 1);
  for (let i = 0; i < rows; i++) offsets[i + 1] = offsets[i] + (heights[i] || estimate) + gap;
  return offsets;
}

/** The row whose band contains `y`, clamped to the existing rows. */
export function rowAt(offsets: Float64Array, y: number): number {
  const rows = offsets.length - 1;
  if (rows <= 0 || y <= 0) return 0;
  let lo = 0;
  let hi = rows - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (offsets[mid] <= y) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** Inclusive row range intersecting the band `[top, bottom]`. */
export function rowRange(offsets: Float64Array, top: number, bottom: number): { first: number; last: number } {
  return { first: rowAt(offsets, top), last: rowAt(offsets, bottom) };
}

/** The index to move keyboard focus to, or null when there is nowhere to go. */
export function stepIndex(
  from: number,
  dir: "up" | "down" | "left" | "right",
  cols: number,
  count: number,
): number | null {
  if (count === 0) return null;
  let next = from;
  if (dir === "left") next = from - 1;
  else if (dir === "right") next = from + 1;
  else if (dir === "up") next = from - cols;
  else {
    next = from + cols;
    // A short last row: step to its final card rather than nowhere.
    if (next >= count && Math.floor(from / cols) < Math.floor((count - 1) / cols)) next = count - 1;
  }
  return next < 0 || next >= count ? null : next;
}
