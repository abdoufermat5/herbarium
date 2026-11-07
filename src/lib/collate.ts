// One title ordering for the whole interface. The backend sorts with
// `title COLLATE NOCASE`; this is the closest Intl equivalent (case and
// accents ignored, digit runs compared as numbers). The collator is built
// once: constructing one per sort is far costlier than the sort itself.
const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

export function compareTitles(a: string, b: string): number {
  return collator.compare(a, b);
}

/**
 * The `n` items that rank first under `compare`, in order, without sorting
 * the whole input: a small insertion-sorted window is kept while scanning.
 */
export function topN<T>(items: readonly T[], n: number, compare: (a: T, b: T) => number): T[] {
  const best: T[] = [];
  if (n <= 0) return best;
  for (const item of items) {
    if (best.length === n && compare(item, best[n - 1]) >= 0) continue;
    let i = best.length === n ? n - 1 : best.length;
    while (i > 0 && compare(item, best[i - 1]) < 0) {
      best[i] = best[i - 1];
      i--;
    }
    best[i] = item;
  }
  return best;
}
