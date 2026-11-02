// The reading-path tree: a deterministic, additive model of a tree that grows
// as pages are read. Pure data, no DOM.
//
// One MATURE tree is generated per (seed, detail). Every element carries a growth
// threshold in [0, 1]; the tree at progress p is exactly the elements whose
// threshold <= p, so tree(p1) is a subset of tree(p2) for p1 < p2. Page i of
// `total` owns the elements whose threshold falls in (i/total, (i+1)/total].
//
// Geometry lives in a 90 x 100 viewBox (aspect 0.9), ground at y = 90.

import { growSkeleton } from "./treegen/skeleton";
import { projectSkeleton } from "./treegen/project";
import { circlePath, clusterPaths, leafPath, smoothPath } from "./treegen/draw";
import { makeRng } from "./treegen/rng";

export type TreeDetail = "full" | "mini";
export type TreeStage = "seed" | "sprout" | "sapling" | "young" | "mature";
export type TreeKind = "sun" | "ground" | "seed" | "branch" | "mound" | "leaf" | "petals" | "heart";

export interface TreeElement {
  /** Index in draw order; stable for a given model, usable as a keyed-each key. */
  i: number;
  kind: TreeKind;
  /** Paint layer, see LAYER. */
  layer: number;
  /** SVG path data. */
  d: string;
  /** Branch depth: 0 trunk, 1 limb, 2 branch, 3 twig. */
  depth: number;
  /** Stroke width in viewBox units (branches, ground). */
  w: number;
  /** Leaf tone 0..4 (0-1 back clusters, 2-4 front clusters). */
  tone: number;
  /** Grows in once progress >= threshold. 0 = always present. */
  threshold: number;
  /** Page index that owns the element, or -1 for the always-present base. */
  page: number;
  /** Limb the element belongs to (0-based), or -1 for base art and the trunk. */
  limb: number;
  /** Hidden again once progress reaches this value (the sprout parts). */
  hideAt?: number;
  /** Skeleton branch or leaf-anchor index for the 3D hero; absent on base art. */
  ref?: number;
  /** Branch segment or leaf-card index within that source. */
  part?: number;
}

export interface TreeModel {
  seed: string;
  total: number;
  detail: TreeDetail;
  /** x of the trunk base. */
  baseX: number;
  /** y of the ground line; plant layers scale from (baseX, ground). */
  ground: number;
  /** Trunk attachment point of each limb, in viewBox units; index = limb. */
  limbs: { x: number; y: number }[];
  elements: TreeElement[];
}

export const TREE_W = 90;
export const TREE_H = 100;
/** width / height of the viewBox. */
export const TREE_ASPECT = TREE_W / TREE_H;
export const LAYER = { sun: 0, ground: 1, back: 2, branch: 3, mound: 4, front: 5, bloom: 6 } as const;
export const LAYER_COUNT = 7;

const EPS = 1e-9;

export function treeStage(read: number, total: number): TreeStage {
  if (total <= 0 || read <= 0) return "seed";
  const p = Math.min(read, total) / total;
  if (p >= 1 - EPS) return "mature";
  if (p < 0.25) return "sprout";
  if (p < 0.6) return "sapling";
  return "young";
}

/** Progress in [0, 1] for `read` of `total` pages. */
export function treeProgress(read: number, total: number): number {
  return total > 0 ? Math.min(Math.max(read, 0), total) / total : 0;
}

/** Is the element part of the tree at `progress`? Sprout parts hide again. */
export const isGrown = (el: Pick<TreeElement, "threshold"> & Pick<Partial<TreeElement>, "hideAt">, progress: number) =>
  el.threshold <= progress + EPS && (el.hideAt === undefined || progress < el.hideAt);

// ---------------------------------------------------------------- geometry helpers

type P = [number, number];

const n = (x: number) => Math.round(x * 10) / 10;
/** Stroke width rounded to 2 decimals. */
const w2 = (x: number) => Math.round(x * 100) / 100;
const clamp = (x: number, a: number, b: number) => Math.min(b, Math.max(a, x));

/** A quadratic Bézier with cached length. */
interface Curve {
  a: P;
  c: P;
  b: P;
  len: number;
}

const qpt = (q: Curve, t: number): P => {
  const u = 1 - t;
  return [
    u * u * q.a[0] + 2 * u * t * q.c[0] + t * t * q.b[0],
    u * u * q.a[1] + 2 * u * t * q.c[1] + t * t * q.b[1],
  ];
};
const qtan = (q: Curve, t: number): P => [
  2 * ((1 - t) * (q.c[0] - q.a[0]) + t * (q.b[0] - q.c[0])),
  2 * ((1 - t) * (q.c[1] - q.a[1]) + t * (q.b[1] - q.c[1])),
];

/** The exact sub-curve [s, e] of a quadratic, as path data. */
function qsub(q: Curve, s: number, e: number): string {
  const S = qpt(q, s);
  const E = qpt(q, e);
  const T = qtan(q, s);
  const k = (e - s) / 2;
  return `M${n(S[0])} ${n(S[1])}Q${n(S[0] + T[0] * k)} ${n(S[1] + T[1] * k)} ${n(E[0])} ${n(E[1])}`;
}

function curve(a: P, b: P, bow: number, lift: number): Curve {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len = Math.hypot(dx, dy) || 1;
  const c: P = [
    (a[0] + b[0]) / 2 + (-dy / len) * bow * len,
    (a[1] + b[1]) / 2 + (dx / len) * bow * len - lift * len,
  ];
  return { a, c, b, len };
}

// ---------------------------------------------------------------- drafts

interface Draft {
  kind: TreeKind;
  layer: number;
  d: string;
  depth: number;
  w: number;
  tone: number;
  /** Growth time; only the order matters. */
  g: number;
  /** Always present (threshold 0). */
  base?: boolean;
  /** Appears only when the tree is fully grown (threshold 1). */
  mature?: boolean;
  /** Hidden again once progress reaches this value. */
  hideAt?: number;
  /** Limb this element grows with, -1 outside a limb. */
  limb?: number;
  /** Skeleton branch or leaf-anchor index for the 3D hero. */
  ref?: number;
  /** Branch segment or leaf-card index within that source. */
  part?: number;
  n: number;
}

interface Geometry {
  baseX: number;
  ground: number;
  limbs: { x: number; y: number }[];
  /** Always-present elements first, then the growth sequence, each already thresholded. */
  elements: Omit<TreeElement, "page">[];
}

export const GROUND = 90;

interface Crown {
  baseX: number;
  cx: number;
  cy: number;
  rx: number;
  ry: number;
  trunk: Curve;
  rnd: () => number;
}

function makeCrown(seed: string): Crown {
  const rnd = makeRng(`herbarium/${seed}`);
  const R = (a: number, b: number) => a + (b - a) * rnd();
  const baseX = 45 + R(-5, 5);
  const trunkH = R(22, 31);
  const style = Math.floor(rnd() * 5);
  const lean = R(-9, 9);
  let rx: number;
  let ry: number;
  let ox = 0;
  if (style === 0) {
    rx = R(27, 31);
    ry = rx * R(0.9, 1);
  } else if (style === 1) {
    rx = R(33, 37);
    ry = R(17, 21);
  } else if (style === 2) {
    rx = R(20, 24);
    ry = R(29, 33);
  } else if (style === 3) {
    rx = R(31, 35);
    ry = R(21, 25);
  } else {
    rx = R(28, 32);
    ry = R(19, 24);
    ox = (lean >= 0 ? 1 : -1) * R(5, 9);
  }
  rx = Math.min(rx * 1.08, 39);
  ry = Math.min(ry * 1.38, (GROUND - trunkH - 5) / 1.5);
  const forkX = baseX + lean;
  const cx = clamp(forkX + ox, rx + 4, TREE_W - rx - 4);
  const cy = GROUND - trunkH - ry * 0.5;
  const trunk = curve([baseX, GROUND], [forkX, GROUND - trunkH], R(-0.12, 0.12), 0);
  return { baseX, cx, cy, rx, ry, trunk, rnd };
}

function finish(drafts: Draft[], baseX: number, limbs: { x: number; y: number }[]): Geometry {
  const base = drafts.filter((d) => d.base);
  const grow = drafts.filter((d) => !d.base).sort((a, b) => a.g - b.g || a.n - b.n);
  const K = grow.filter((d) => !d.mature).length;
  // The sprout parts hide at half the first growth threshold, so they are gone
  // as soon as the tree starts to grow instead of lingering at EPS.
  const sproutHide = K > 0 ? 0.5 / (K + 1) : EPS;
  let r = 0;
  const out: Omit<TreeElement, "page">[] = [];
  const push = (d: Draft, threshold: number) =>
    out.push({
      i: 0,
      kind: d.kind,
      layer: d.layer,
      d: d.d,
      depth: d.depth,
      w: d.w,
      tone: d.tone,
      threshold,
      limb: d.limb ?? -1,
      hideAt: d.hideAt === EPS ? sproutHide : d.hideAt,
    });
  for (const d of base) push(d, 0);
  grow.forEach((d) => push(d, d.mature ? 1 : ++r / (K + 1)));
  out.sort((a, b) => a.layer - b.layer); // stable: growth order kept inside a layer
  out.forEach((e, i) => (e.i = i));
  return { baseX, ground: GROUND, limbs, elements: out };
}

// ---------------------------------------------------------------- full detail

function buildFull(seed: string): Geometry {
  const skel = growSkeleton(seed);
  const proj = projectSkeleton(skel);
  const baseX = proj.baseX;
  const drafts: Draft[] = [];
  let count = 0;
  const add = (d: Omit<Draft, "n" | "depth" | "w" | "tone"> & Partial<Pick<Draft, "depth" | "w" | "tone">>) => {
    const draft: Draft = {
      kind: d.kind,
      layer: d.layer,
      d: d.d,
      depth: d.depth ?? 0,
      w: d.w ?? 0,
      tone: d.tone ?? 0,
      g: d.g,
      base: d.base,
      mature: d.mature,
      hideAt: d.hideAt,
      limb: d.limb,
      ref: d.ref,
      part: d.part,
      n: count++,
    };
    drafts.push(draft);
    return draft;
  };

  // Ground: an ink line, a soil mound, and the seed beneath.
  const gx = clamp(baseX - 30, 3, 27);
  add({ kind: "ground", layer: LAYER.ground, d: `M${n(gx)} ${GROUND}q10 -1.4 20 0t20 0t20 0`, w: 0.9, g: -3, base: true });
  add({
    kind: "mound",
    layer: LAYER.mound,
    d: `M${n(baseX - 13)} ${GROUND}Q${n(baseX - 7)} ${GROUND - 5.6} ${n(baseX)} ${GROUND - 5.3}Q${n(baseX + 8)} ${GROUND - 5.1} ${n(baseX + 14)} ${GROUND}Z`,
    w: 1,
    g: -3,
    base: true,
  });
  add({ kind: "seed", layer: LAYER.ground, d: `M${n(baseX - 1.4)} ${GROUND + 4}a2 1.3 0 1 0 4 0a2 1.3 0 1 0 -4 0Z`, w: 1, g: -3, base: true, hideAt: EPS });

  // Sprout: the foot of the 3D trunk and a pair of seed leaves, there from the start.
  const trunkPoints = proj.branches[0].points;
  const cut = Math.min(trunkPoints.length, Math.max(2, Math.ceil(trunkPoints.length * 0.3)));
  const sprout = trunkPoints.slice(0, cut);
  add({ kind: "branch", layer: LAYER.branch, d: smoothPath(sprout), depth: 0, w: 1.5, g: -2, base: true, hideAt: EPS });
  const sp = sprout[sprout.length - 1];
  const sa = Math.atan2(sp.y - sprout[sprout.length - 2].y, sp.x - sprout[sprout.length - 2].x);
  add({ kind: "leaf", layer: LAYER.front, d: leafPath(sp.x, sp.y, sa - 0.95, 4.6, 1.7), tone: 3, g: -2, base: true, hideAt: EPS });
  add({ kind: "leaf", layer: LAYER.front, d: leafPath(sp.x, sp.y, sa + 0.8, 4.2, 1.6), tone: 2, g: -2, base: true, hideAt: EPS });

  // Trunk: three contiguous segments of the projected trunk, sharing endpoints.
  const trunkRadii = proj.branches[0].radii;
  const splitA = Math.max(1, Math.round(trunkPoints.length / 3));
  const splitB = Math.max(splitA + 1, Math.round((2 * trunkPoints.length) / 3));
  [
    [0, splitA],
    [splitA, splitB],
    [splitB, trunkPoints.length - 1],
  ].forEach(([s, e], k) => {
    const mid = Math.min((s + e) >> 1, trunkRadii.length - 1);
    add({
      kind: "branch",
      layer: LAYER.branch,
      d: smoothPath(trunkPoints.slice(s, e + 1)),
      depth: 0,
      w: w2(clamp(trunkRadii[mid] * 0.85, 0.35, 3.0)),
      g: 0.5 + k * 0.15,
      ref: 0,
      part: k,
    });
  });

  // Leaf clusters: tone and layer from depth (z) and the upper-left light.
  let zMin = Infinity;
  let zMax = -Infinity;
  for (const b of proj.branches) {
    for (const p of b.points) {
      if (p.z < zMin) zMin = p.z;
      if (p.z > zMax) zMax = p.z;
    }
  }
  for (const l of proj.leaves) {
    if (l.z < zMin) zMin = l.z;
    if (l.z > zMax) zMax = l.z;
  }
  const zSpan = zMax - zMin || 1;
  const leafCluster = (l: (typeof proj.leaves)[number], g: number, ref: number) => {
    const zN = clamp((l.z - zMin) / zSpan, 0, 1);
    const nx = clamp((l.x - 45) / 45, -1, 1);
    const ny = clamp((l.y - 50) / 45, -1, 1);
    const light = clamp(0.55 - 0.28 * nx - 0.22 * ny, 0, 1);
    const layer = zN < 0.2 ? LAYER.back : LAYER.front;
    let tone = 3;
    if (zN < 0.2) tone = zN < 0.08 ? 0 : 1;
    else if (light < 0.3) tone = 2;
    else if (light > 0.8 && zN > 0.8) tone = 4;
    const depth = clamp(l.level, 1, 3);
    clusterPaths(l.x, l.y, l.angle, l.size).forEach((d, p) =>
      add({ kind: "leaf", layer, d, depth, tone, g: g + 0.00001 * p, limb: l.limb, ref, part: p }),
    );
  };

  // Growth windows alternate sides so partial trees stay balanced. Projected y
  // grows downward, so larger y (lower limbs) goes first; sides use the limb
  // tip, since every foot sits close to the trunk.
  const tipX: number[] = [];
  skel.branches.forEach((b, i) => {
    if (b.level !== 1 || b.limb < 0) return;
    const points = proj.branches[i].points;
    tipX[b.limb] = points[points.length - 1].x;
  });
  const byHeight = proj.limbs.map((_, j) => j).sort((a, b) => proj.limbs[b].y - proj.limbs[a].y || a - b);
  const leftSide = byHeight.filter((j) => tipX[j] < 45);
  const rightSide = byHeight.filter((j) => tipX[j] >= 45);
  const growth: number[] = [];
  let takeLeft = byHeight.length > 0 && tipX[byHeight[0]] < 45;
  while (leftSide.length > 0 || rightSide.length > 0) {
    const side = takeLeft ? leftSide : rightSide;
    const pick = side.length > 0 ? side.shift() : (takeLeft ? rightSide : leftSide).shift();
    if (pick === undefined) break;
    growth.push(pick);
    takeLeft = !takeLeft;
  }
  const slot: number[] = [];
  growth.forEach((j, at) => (slot[j] = at));

  // Add one branch of limb `j` plus the leaf clusters growing from it.
  const addBranch = (j: number, i: number, g: number) => {
    const pb = proj.branches[i];
    const b = skel.branches[i];
    const depth = clamp(b.level, 0, 3);
    const last = pb.points.length - 1;
    if (b.level <= 1 && pb.points.length >= 4) {
      const m = Math.max(1, Math.floor(last / 2));
      [[0, m], [m, last]].forEach(([s, e], part) => {
        add({
          kind: "branch",
          layer: LAYER.branch,
          d: smoothPath(pb.points.slice(s, e + 1)),
          depth,
          w: w2(clamp(pb.radii[Math.min((s + e) >> 1, pb.radii.length - 1)] * 0.85, 0.35, 3.0)),
          g,
          limb: j,
          ref: i,
          part,
        });
      });
    } else {
      add({
        kind: "branch",
        layer: LAYER.branch,
        d: smoothPath(pb.points),
        depth,
        w: w2(clamp(pb.radii[last >> 1] * 0.85, 0.35, 3.0)),
        g,
        limb: j,
        ref: i,
        part: 0,
      });
    }
    let li = 0;
    proj.leaves.forEach((l, ref) => {
      if (l.tip || l.parent !== i || l.limb !== j) return;
      leafCluster(l, g + 0.001 * (li + 1), ref);
      li++;
    });
  };

  // Each limb grows in its own [g0, g0 + 1) window. Its level-1 branch comes
  // first so the tip cluster sits on wood; the remaining branches follow well
  // after it, so a visible deep branch always has a leaf in the same limb.
  for (let j = 0; j < proj.limbs.length; j++) {
    const g0 = 1 + slot[j];
    const own: number[] = [];
    skel.branches.forEach((b, i) => {
      if (b.limb === j) own.push(i);
    });
    const first = own.shift();
    if (first !== undefined) addBranch(j, first, g0 + 0.001);
    const tip = proj.leaves.findIndex((l) => l.tip && l.limb === j);
    if (tip >= 0) leafCluster(proj.leaves[tip], g0 + 0.002, tip);
    own.forEach((i, k) => addBranch(j, i, g0 + 0.02 + k * 0.006));
  }

  // Maturity: one offset sun disc and a handful of quiet blossoms. A separate
  // stream keeps blossom choices out of the skeleton's determinism.
  const bloom = makeRng(`herbarium/bloom/${seed}`);
  const R = (a: number, b: number) => a + (b - a) * bloom();
  const { left, top, right, bottom } = proj.bounds;
  const cx = (left + right) / 2;
  const cy = (top + bottom) / 2;
  const rx = Math.max(1, (right - left) / 2);
  const ry = Math.max(1, (bottom - top) / 2);
  const sr = R(14, 18);
  const sideSun = bloom() < 0.5 ? -1 : 1;
  const sx = clamp(cx + sideSun * rx * 0.5, sr + 2, TREE_W - sr - 2);
  const sy = clamp(cy - ry * 0.3, sr + 2, 60);
  add({ kind: "sun", layer: LAYER.sun, d: circlePath(sx, sy, sr), g: 20, mature: true });
  // Few and never crowded: candidates closer than 6 units to a kept blossom
  // are dropped, so the crown keeps its calm instead of filling with pinwheels.
  const nB = 6 + Math.floor(bloom() * 3);
  const spots: P[] = [];
  for (let tries = 0; tries < 30 && spots.length < nB; tries++) {
    const a = proj.leaves[Math.floor(bloom() * proj.leaves.length)];
    if (!a) break;
    const bx = a.x + R(-3, 3);
    const by = a.y + R(-3, 3);
    if (spots.some(([px, py]) => Math.hypot(px - bx, py - by) < 6)) continue;
    spots.push([bx, by]);
  }
  spots.forEach(([bx, by], k) => {
    const pr = R(1.0, 1.3);
    const rot = R(0, Math.PI * 2);
    let d = "";
    for (let i = 0; i < 5; i++) {
      const t = rot + (i * Math.PI * 2) / 5;
      d += circlePath(bx + Math.cos(t) * pr * 1.05, by + Math.sin(t) * pr * 1.05, pr);
    }
    add({ kind: "petals", layer: LAYER.bloom, d, w: 0.35, g: 21 + k * 0.01, mature: true });
    add({ kind: "heart", layer: LAYER.bloom, d: circlePath(bx, by, pr * 0.45), g: 21.005 + k * 0.01, mature: true });
  });
  return finish(drafts, proj.baseX, proj.limbs);
}

// ---------------------------------------------------------------- mini detail

/** Sidebar and inline sizes (16-32px): chunky strokes, a handful of shapes. */
function buildMini(seed: string): Geometry {
  const cr = makeCrown(seed);
  const { rnd, baseX, cx, cy, rx, ry, trunk } = cr;
  const R = (a: number, b: number) => a + (b - a) * rnd();
  const drafts: Draft[] = [];
  let count = 0;
  const add = (d: Omit<Draft, "n" | "depth" | "w" | "tone"> & Partial<Pick<Draft, "depth" | "w" | "tone">>) =>
    drafts.push({
      kind: d.kind,
      layer: d.layer,
      d: d.d,
      depth: d.depth ?? 0,
      w: d.w ?? 0,
      tone: d.tone ?? 0,
      g: d.g,
      base: d.base,
      mature: d.mature,
      hideAt: d.hideAt,
      limb: d.limb,
      n: count++,
    });

  add({ kind: "ground", layer: LAYER.ground, d: `M${n(baseX - 26)} ${GROUND}H${n(baseX + 26)}`, w: 3.2, g: -3, base: true });
  add({
    kind: "mound",
    layer: LAYER.mound,
    d: `M${n(baseX - 14)} ${GROUND}Q${n(baseX - 6)} ${GROUND - 8} ${n(baseX)} ${GROUND - 7.5}Q${n(baseX + 8)} ${GROUND - 7} ${n(baseX + 14)} ${GROUND}Z`,
    w: 2,
    g: -3,
    base: true,
  });
  add({ kind: "branch", layer: LAYER.branch, d: qsub(trunk, 0, 0.4), w: 5, g: -2, base: true, hideAt: EPS });
  const sp = qpt(trunk, 0.4);
  add({ kind: "leaf", layer: LAYER.front, d: circlePath(sp[0] + 3, sp[1] - 1, 4.2), tone: 3, g: -2, base: true, hideAt: EPS });

  add({ kind: "branch", layer: LAYER.branch, d: qsub(trunk, 0, 1), w: 6.5, g: 0 });
  const nL = 2 + Math.floor(rnd() * 3);
  const tops: P[] = [];
  const limbBase: { x: number; y: number }[] = [];
  for (let i = 0; i < nL; i++) {
    const th = -1 + (2 * (i + 0.5)) / nL;
    const A = qpt(trunk, clamp(1 - 0.35 * Math.abs(th), 0.55, 1));
    limbBase.push({ x: A[0], y: A[1] });
    const T: P = [cx + rx * Math.sin(th) * 0.6, cy - ry * Math.cos(th) * 0.55];
    tops.push(T);
    add({ kind: "branch", layer: LAYER.branch, d: qsub(curve(A, T, R(-0.12, 0.12), 0.05), 0, 1), depth: 1, w: 4, g: 0.1 + i * 0.1, limb: i });
  }

  // Canopy blobs: big soft ones behind, smaller rich ones in front.
  const m = Math.min(rx, ry);
  const backs = 3 + Math.floor(rnd() * 2);
  for (let k = 0; k < backs; k++) {
    const a = Math.PI * (1.08 + (0.84 * k) / (backs - 1)) + R(-0.12, 0.12); // across the top of the crown
    add({
      kind: "leaf",
      layer: LAYER.back,
      d: circlePath(cx + Math.cos(a) * rx * 0.55, cy + 0.1 * ry + Math.sin(a) * ry * 0.5, m * R(0.58, 0.72)),
      tone: k % 3 === 1 ? 1 : 0,
      g: 0.5 + k * 0.05,
    });
  }
  const fronts = 2 + Math.floor(rnd() * 2);
  for (let k = 0; k < fronts; k++) {
    const T = tops[k % tops.length];
    add({
      kind: "leaf",
      layer: LAYER.front,
      d: circlePath(T[0] + R(-3, 3), T[1] + R(-2, 4), m * R(0.34, 0.46)),
      tone: k % 2 ? 3 : 2,
      g: 0.8 + k * 0.1,
    });
  }
  // One to three blossom dots at maturity, same quiet palette.
  const nB = 1 + Math.floor(rnd() * 3);
  for (let k = 0; k < nB; k++) {
    const T = tops[k % tops.length];
    const bx = T[0] + R(-3, 3) + (k ? 4 : -4);
    const by = T[1] + R(-4, 1);
    add({ kind: "petals", layer: LAYER.bloom, d: circlePath(bx, by, 3.2), w: 0.9, g: 20 + k, mature: true });
    add({ kind: "heart", layer: LAYER.bloom, d: circlePath(bx, by, 1.4), g: 20.5 + k, mature: true });
  }
  return finish(drafts, baseX, limbBase);
}

// ---------------------------------------------------------------- public API

const geometry = new Map<string, Geometry>();
const models = new Map<string, TreeModel>();
const CACHE_MAX = 96;

function remember<V>(map: Map<string, V>, key: string, value: V): V {
  if (map.size >= CACHE_MAX) map.delete(map.keys().next().value as string);
  map.set(key, value);
  return value;
}

/**
 * The mature tree for `seed`, thresholded for a path of `total` pages. Memoized
 * per (seed, total, detail); the same arguments return the same object.
 */
export function growTree(seed: string, total: number, detail: TreeDetail = "full"): TreeModel {
  const t = Number.isFinite(total) ? Math.max(0, Math.floor(total)) : 0;
  const key = `${detail}|${t}|${seed}`;
  const hit = models.get(key);
  if (hit) {
    // Refresh recency so the map behaves as an LRU, not FIFO.
    models.delete(key);
    models.set(key, hit);
    return hit;
  }
  const gkey = `${detail}|${seed}`;
  const geo = geometry.get(gkey) ?? remember(geometry, gkey, detail === "mini" ? buildMini(seed) : buildFull(seed));
  const pages = Math.max(t, 1);
  const elements: TreeElement[] = geo.elements.map((e) => ({
    ...e,
    page: e.threshold === 0 ? -1 : Math.min(pages - 1, Math.max(0, Math.ceil(e.threshold * pages - EPS) - 1)),
  }));
  return remember(models, key, { seed, total: t, detail, baseX: geo.baseX, ground: geo.ground, limbs: geo.limbs, elements });
}
