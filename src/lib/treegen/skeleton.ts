import type { LeafAnchor, Skeleton, SkeletonBranch, V3 } from "./types";
import { clamp, makeRng, range } from "./rng";

const GOLDEN = 2.39996;
const MAX_BRANCHES = 70;
const MAX_LEAVES = 76;
const MIN_LEAVES = 68;
const CACHE_MAX = 256;
const TILT_MIN = 0.6981317;
const TILT_MAX = 1.134464;
const TROPISM_GAIN = 0.022;
const TROPISM_MAX = 0.075;

interface Grown {
  points: V3[];
  radii: number[];
  length: number;
}

interface Spawn {
  at: V3;
  radius: number;
  dir: V3;
}

const unit = (v: V3): V3 => {
  const l = Math.hypot(v[0], v[1], v[2]);
  return l > 1e-9 ? [v[0] / l, v[1] / l, v[2] / l] : [0, 1, 0];
};

const cross = (a: V3, b: V3): V3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];

/** Unit vector around `d` at `angle`, using a basis that stays valid near ±Y. */
const radial = (d: V3, angle: number): V3 => {
  const ref: V3 = Math.abs(d[1]) > 0.95 ? [1, 0, 0] : [0, 1, 0];
  const u = unit(cross(d, ref));
  const v = unit(cross(d, u));
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  return unit([u[0] * cos + v[0] * sin, u[1] * cos + v[1] * sin, u[2] * cos + v[2] * sin]);
};

function grow(
  rnd: () => number,
  origin: V3,
  dir: V3,
  length: number,
  baseRadius: number,
  sections: number,
  gnarl: number,
  tipRatio: number,
): Grown {
  const points: V3[] = [origin];
  const radii: number[] = [baseRadius];
  const step = length / sections;
  let p = origin;
  let d = unit(dir);
  let travelled = 0;
  for (let i = 1; i <= sections; i++) {
    const t = i / sections;
    const r = baseRadius * (1 - (1 - tipRatio) * t);
    const raw: V3 = [rnd() * 2 - 1, rnd() * 2 - 1, rnd() * 2 - 1];
    const along = raw[0] * d[0] + raw[1] * d[1] + raw[2] * d[2];
    const perp = unit([raw[0] - d[0] * along, raw[1] - d[1] * along, raw[2] - d[2] * along]);
    const turn = gnarl * range(rnd, 0.35, 1);
    const bent = unit([d[0] + perp[0] * turn, d[1] + perp[1] * turn, d[2] + perp[2] * turn]);
    const bend = Math.min(TROPISM_MAX, TROPISM_GAIN / Math.max(r, 0.05));
    d = unit([bent[0], bent[1] + bend, bent[2]]);
    const next: V3 = [p[0] + d[0] * step, p[1] + d[1] * step, p[2] + d[2] * step];
    travelled += Math.hypot(next[0] - p[0], next[1] - p[1], next[2] - p[2]);
    points.push(next);
    radii.push(r);
    p = next;
  }
  return { points, radii, length: travelled };
}

/** Point, radius and tangent at fraction `f` of a branch's polyline length. */
function locate(b: SkeletonBranch, f: number): Spawn {
  const points = b.points;
  let remaining = clamp(f, 0, 1) * b.length;
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1];
    const c = points[i];
    const seg = Math.hypot(c[0] - a[0], c[1] - a[1], c[2] - a[2]);
    if (remaining <= seg || i === points.length - 1) {
      const t = seg > 1e-9 ? clamp(remaining / seg, 0, 1) : 0;
      return {
        at: [a[0] + (c[0] - a[0]) * t, a[1] + (c[1] - a[1]) * t, a[2] + (c[2] - a[2]) * t],
        radius: b.radii[i - 1] + (b.radii[i] - b.radii[i - 1]) * t,
        dir: seg > 1e-9 ? unit([(c[0] - a[0]) / seg, (c[1] - a[1]) / seg, (c[2] - a[2]) / seg]) : [0, 1, 0],
      };
    }
    remaining -= seg;
  }
  const last = points[points.length - 1];
  return { at: last, radius: b.radii[b.radii.length - 1], dir: [0, 1, 0] };
}

function permutation(count: number, rnd: () => number): number[] {
  const out = new Array<number>(count);
  for (let i = 0; i < count; i++) out[i] = i;
  for (let i = count - 1; i > 0; i--) {
    const j = Math.floor(rnd() * (i + 1));
    const swap = out[i];
    out[i] = out[j];
    out[j] = swap;
  }
  return out;
}

function trimLeaves(leaves: LeafAnchor[]): LeafAnchor[] {
  const excess = leaves.length - MAX_LEAVES;
  if (excess <= 0) return leaves;
  let nonTip = 0;
  for (const leaf of leaves) if (!leaf.tip) nonTip++;
  const keep = nonTip - excess;
  const chosen = new Set<number>();
  for (let k = 0; k < keep; k++) chosen.add(Math.floor((k * nonTip) / keep));
  const out: LeafAnchor[] = [];
  let seen = 0;
  for (const leaf of leaves) {
    if (leaf.tip || chosen.has(seen)) out.push(leaf);
    if (!leaf.tip) seen++;
  }
  return out;
}

function build(seed: string): Skeleton {
  const rnd = makeRng(`herbarium/tree/${seed}`);
  const branches: SkeletonBranch[] = [];

  const trunk = grow(
    rnd,
    [0, 0, 0],
    unit([range(rnd, -0.16, 0.16), 1, range(rnd, -0.16, 0.16)]),
    range(rnd, 30, 38),
    range(rnd, 2.2, 2.8),
    9 + Math.floor(rnd() * 4),
    range(rnd, 0.03, 0.05),
    range(rnd, 0.45, 0.6),
  );
  branches.push({ level: 0, limb: -1, parent: -1, ...trunk });

  const limbCount = 4 + Math.floor(rnd() * 3);
  const limbKids: number[] = [];
  let l2Left = Math.floor((MAX_BRANCHES - 1 - limbCount) / 3);
  for (let i = 0; i < limbCount; i++) {
    const maxNow = Math.min(4, l2Left - 2 * (limbCount - i - 1));
    limbKids.push(Math.max(2, Math.min(2 + Math.floor(rnd() * 3), maxNow)));
    l2Left -= limbKids[i];
  }

  // Each limb yields three anchors; each level-2 branch yields six with its twigs.
  const l2Floor = Math.min(4 * limbCount, Math.max(2 * limbCount, Math.ceil((MIN_LEAVES - 3 * limbCount) / 6)));
  let l2Total = 0;
  for (const count of limbKids) l2Total += count;
  for (let i = 0; i < limbCount && l2Total < l2Floor; i++) {
    const add = Math.min(4 - limbKids[i], l2Floor - l2Total);
    limbKids[i] += add;
    l2Total += add;
  }

  const twigKids: number[] = [];
  let twigLeft = MAX_BRANCHES - 1 - limbCount - l2Total;
  for (let i = 0; i < l2Total; i++) {
    const want = 2 + Math.floor(rnd() * 2);
    const count = twigLeft - want >= 2 * (l2Total - i - 1) ? want : 2;
    twigKids.push(count);
    twigLeft -= count;
  }

  let l2Seen = 0;
  for (let bi = 0; bi < branches.length; bi++) {
    const parent = branches[bi];
    if (parent.level >= 3) continue;
    const count = parent.level === 0 ? limbCount : parent.level === 1 ? limbKids[parent.limb] : twigKids[l2Seen++];
    const start = range(rnd, 0.45, 0.6);
    const slots = permutation(count, rnd);
    for (let i = 0; i < count; i++) {
      const f = start + ((i + range(rnd, 0.15, 0.85)) / count) * (0.97 - start);
      const spawn = locate(parent, f);
      const side = radial(spawn.dir, GOLDEN * slots[i] + range(rnd, -0.2, 0.2));
      const tilt = range(rnd, TILT_MIN, TILT_MAX);
      const cos = Math.cos(tilt);
      const sin = Math.sin(tilt);
      const level = parent.level + 1;
      const dir = unit([
        spawn.dir[0] * cos + side[0] * sin,
        spawn.dir[1] * cos + side[1] * sin,
        spawn.dir[2] * cos + side[2] * sin,
      ]);
      const length = level === 1 ? range(rnd, 13, 19) : level === 2 ? range(rnd, 7, 11) : range(rnd, 3.5, 6);
      const sections = level === 1 ? 6 + Math.floor(rnd() * 4) : level === 2 ? 4 + Math.floor(rnd() * 3) : 3 + Math.floor(rnd() * 2);
      const gnarl = level === 1 ? range(rnd, 0.06, 0.1) : level === 2 ? range(rnd, 0.08, 0.12) : range(rnd, 0.1, 0.16);
      const baseRadius = spawn.radius * range(rnd, 0.55, 0.7);
      const grown = grow(rnd, spawn.at, dir, length, baseRadius, sections, gnarl, range(rnd, 0.45, 0.6));
      branches.push({
        level,
        limb: level === 1 ? i : parent.limb,
        parent: bi,
        ...grown,
      });
    }
  }

  const anchor = (b: SkeletonBranch, index: number, level: number, f: number, tip: boolean): LeafAnchor => {
    const spawn = locate(b, f);
    const h = Math.hypot(spawn.at[0], spawn.at[2]);
    const out: V3 = h > 1e-6 ? [spawn.at[0] / h, 0, spawn.at[2] / h] : [0, 0, 0];
    const dir = tip
      ? unit([spawn.dir[0], spawn.dir[1] + 0.3, spawn.dir[2]])
      : unit([spawn.dir[0] + out[0] * 0.25, spawn.dir[1] + 0.35, spawn.dir[2] + out[2] * 0.25]);
    return { at: spawn.at, dir, parent: index, limb: b.limb, level, size: range(rnd, 3.4, 5.2), tip };
  };

  let leaves: LeafAnchor[] = [];
  for (let i = 0; i < branches.length; i++) {
    const b = branches[i];
    if (b.level === 1) {
      leaves.push(anchor(b, i, 1, 1, true));
      leaves.push(anchor(b, i, 1, 0.55, false));
      leaves.push(anchor(b, i, 1, 0.85, false));
    } else if (b.level === 2) {
      leaves.push(anchor(b, i, 2, 1, false));
      leaves.push(anchor(b, i, 2, 0.55, false));
    } else if (b.level === 3) {
      leaves.push(anchor(b, i, 3, 0.6, false));
      leaves.push(anchor(b, i, 3, 1, false));
    }
  }
  leaves = trimLeaves(leaves);

  let minX = Infinity;
  let minY = Infinity;
  let minZ = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  let maxZ = -Infinity;
  for (const branch of branches) {
    for (const p of branch.points) {
      if (p[0] < minX) minX = p[0];
      if (p[1] < minY) minY = p[1];
      if (p[2] < minZ) minZ = p[2];
      if (p[0] > maxX) maxX = p[0];
      if (p[1] > maxY) maxY = p[1];
      if (p[2] > maxZ) maxZ = p[2];
    }
  }
  for (const leaf of leaves) {
    const p = leaf.at;
    if (p[0] < minX) minX = p[0];
    if (p[1] < minY) minY = p[1];
    if (p[2] < minZ) minZ = p[2];
    if (p[0] > maxX) maxX = p[0];
    if (p[1] > maxY) maxY = p[1];
    if (p[2] > maxZ) maxZ = p[2];
  }

  return {
    seed,
    branches,
    leaves,
    base: [0, 0, 0],
    height: maxY + 2,
    crown: {
      cx: (minX + maxX) / 2,
      cy: (minY + maxY) / 2,
      cz: (minZ + maxZ) / 2,
      rx: Math.max((maxX - minX) / 2, (maxZ - minZ) / 2),
      ry: (maxY - minY) / 2,
    },
  };
}

const cache = new Map<string, Skeleton>();

export function growSkeleton(seed: string): Skeleton {
  const cached = cache.get(seed);
  if (cached) return cached;
  const skeleton = build(seed);
  if (cache.size >= CACHE_MAX) {
    const oldest = cache.keys().next();
    if (!oldest.done) cache.delete(oldest.value);
  }
  cache.set(seed, skeleton);
  return skeleton;
}
