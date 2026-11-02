// The reading-path tree: a deterministic, additive model of a tree that grows
// as pages are read. Pure data, no DOM.
//
// One MATURE tree is generated per (seed, detail). Every element carries a growth
// threshold in [0, 1]; the tree at progress p is exactly the elements whose
// threshold <= p, so tree(p1) is a subset of tree(p2) for p1 < p2. Page i of
// `total` owns the elements whose threshold falls in (i/total, (i+1)/total].
//
// Geometry lives in a 90 x 100 viewBox (aspect 0.9), ground at y = 90.

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
  /** Growth order 0..1, used to stagger the grow-in animation. */
  seq: number;
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

// ---------------------------------------------------------------- PRNG

function xmur3(str: string): () => number {
  let h = 1779033703 ^ str.length;
  for (let i = 0; i < str.length; i++) {
    h = Math.imul(h ^ str.charCodeAt(i), 3432918353);
    h = (h << 13) | (h >>> 19);
  }
  return () => {
    h = Math.imul(h ^ (h >>> 16), 2246822507);
    h = Math.imul(h ^ (h >>> 13), 3266489909);
    return (h ^= h >>> 16) >>> 0;
  };
}

function mulberry32(a: number): () => number {
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// ---------------------------------------------------------------- geometry helpers

type P = [number, number];

const n = (x: number) => Math.round(x * 10) / 10;
const clamp = (x: number, a: number, b: number) => Math.min(b, Math.max(a, x));
const circ = (cx: number, cy: number, r: number) =>
  `M${n(cx - r)} ${n(cy)}a${n(r)} ${n(r)} 0 1 0 ${n(2 * r)} 0a${n(r)} ${n(r)} 0 1 0 ${n(-2 * r)} 0Z`;

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

/** One teardrop leaf from `o`, pointing at `ang`, length L, half-width w. */
function leaf(o: P, ang: number, L: number, w: number): string {
  const dx = Math.cos(ang);
  const dy = Math.sin(ang);
  const px = -dy;
  const py = dx;
  const x = o[0];
  const y = o[1];
  const lx = dx * L;
  const ly = dy * L;
  const wx = px * w;
  const wy = py * w;
  // control points at 25% (full width) and 80% (70% width) along the leaf, mirrored
  const ax = lx * 0.25;
  const ay = ly * 0.25;
  const bx = lx * 0.8;
  const by = ly * 0.8;
  const hx = wx * 0.7;
  const hy = wy * 0.7;
  return (
    `M${n(x)} ${n(y)}C${n(x + ax + wx)} ${n(y + ay + wy)} ${n(x + bx + hx)} ${n(y + by + hy)} ${n(x + lx)} ${n(y + ly)}` +
    `C${n(x + bx - hx)} ${n(y + by - hy)} ${n(x + ax - wx)} ${n(y + ay - wy)} ${n(x)} ${n(y)}Z`
  );
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
  forkX: number;
  trunkH: number;
  style: number;
  cx: number;
  cy: number;
  rx: number;
  ry: number;
  trunk: Curve;
  rnd: () => number;
}

function makeCrown(seed: string): Crown {
  const rnd = mulberry32(xmur3(`herbarium/${seed}`)());
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
  ry = Math.min(ry * 1.38,(GROUND - trunkH - 5) / 1.5);
  const forkX = baseX + lean;
  const cx = clamp(forkX + ox, rx + 4, TREE_W - rx - 4);
  const cy = GROUND - trunkH - ry * 0.5;
  const trunk = curve([baseX, GROUND], [forkX, GROUND - trunkH], R(-0.12, 0.12), 0);
  return { baseX, forkX, trunkH, style, cx, cy, rx, ry, trunk, rnd };
}

function finish(drafts: Draft[], baseX: number, limbs: { x: number; y: number }[]): Geometry {
  const base = drafts.filter((d) => d.base);
  const grow = drafts.filter((d) => !d.base).sort((a, b) => a.g - b.g || a.n - b.n);
  const K = grow.filter((d) => !d.mature).length;
  let r = 0;
  const out: Omit<TreeElement, "page">[] = [];
  const push = (d: Draft, threshold: number, seq: number) =>
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
      hideAt: d.hideAt,
      seq,
    });
  for (const d of base) push(d, 0, 0);
  const last = Math.max(1, grow.length - 1);
  grow.forEach((d, idx) => push(d, d.mature ? 1 : ++r / (K + 1), idx / last));
  out.sort((a, b) => a.layer - b.layer); // stable: growth order kept inside a layer
  out.forEach((e, i) => (e.i = i));
  return { baseX, ground: GROUND, limbs, elements: out };
}

// ---------------------------------------------------------------- full detail

function buildFull(seed: string): Geometry {
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

  // Sprout: a short stem and a pair of seed leaves, there from the start.
  add({ kind: "branch", layer: LAYER.branch, d: qsub(trunk, 0, 0.3), depth: 0, w: 1.5, g: -2, base: true, hideAt: EPS });
  const sp = qpt(trunk, 0.3);
  const stan = qtan(trunk, 0.3);
  const sa = Math.atan2(stan[1], stan[0]);
  add({ kind: "leaf", layer: LAYER.front, d: leaf(sp, sa - 0.95, 4.6, 1.7), tone: 3, g: -2, base: true, hideAt: EPS });
  add({ kind: "leaf", layer: LAYER.front, d: leaf(sp, sa + 0.8, 4.2, 1.6), tone: 2, g: -2, base: true, hideAt: EPS });

  // Limbs fan out across the crown; wide ones leave the trunk lower. They are
  // needed before the trunk: a segment rises only once the limbs below it start.
  const nL = 4 + Math.floor(rnd() * 3);
  interface Limb { th: number; t: number }
  const limbs: Limb[] = [];
  for (let i = 0; i < nL; i++) {
    const th = -1.15 + (2.3 * (i + 0.5)) / nL + R(-0.12, 0.12);
    limbs.push({ th, t: clamp(1 - 0.5 * Math.abs(Math.sin(th)) * R(0.75, 1), 0.5, 1) });
  }
  limbs.sort((a, b) => a.t - b.t);
  const below = (t: number) => limbs.reduce((k, L) => k + (L.t < t ? 1 : 0), 0);

  // Trunk: six tapering segments. Segments 0..2 come first; higher ones appear
  // just before the limbs that attach above them, so the trunk rises as it goes.
  const SEG = 6;
  for (let k = 0; k < SEG; k++) {
    add({
      kind: "branch",
      layer: LAYER.branch,
      d: qsub(trunk, k / SEG, (k + 1) / SEG),
      depth: 0,
      w: 2.9 - k * 0.18,
      g: 0.5 + below(k / SEG),
    });
  }

  const spawn = (parent: Curve, t: number, side: number, frac: number, depth: number, g: number, limb: number) => {
    const p = qpt(parent, t);
    const tn = qtan(parent, t);
    const ang = Math.atan2(tn[1], tn[0]) + side * R(0.5, 0.95);
    const len = parent.len * frac * R(0.85, 1.15);
    let tx = p[0] + Math.cos(ang) * len;
    let ty = p[1] + Math.sin(ang) * len;
    tx += (cx - tx) * 0.1;
    ty += (cy - ty) * 0.1 - len * 0.12;
    for (let k = 0; k < 3; k++) {
      const ex = (tx - cx) / (rx * 1.02);
      const ey = (ty - cy) / (ry * 1.02);
      if (ex * ex + ey * ey <= 1) break;
      tx = cx + (tx - cx) * 0.88;
      ty = cy + (ty - cy) * 0.88;
    }
    const q = curve(p, [tx, ty], R(-0.16, 0.16) * (side > 0 ? 1 : -1), 0.02);
    add({ kind: "branch", layer: LAYER.branch, d: qsub(q, 0, 1), depth, w: depth === 2 ? 0.85 : 0.6, g, limb });
    return { q, g };
  };

  // Foliage: layered clusters of three leaves. A cluster is drafted right after
  // the branch it sits on, so a limb can never be a bare twig.
  const scale = R(0.88, 1.18);
  const anchors: P[] = [];
  const cluster = (p: P, ang: number, g: number, scatter: number, limb: number) => {
    const x = p[0] + R(-scatter, scatter);
    const y = p[1] + R(-scatter, scatter);
    const out = Math.atan2(y - cy, x - cx);
    let a = Math.atan2(Math.sin(ang) + Math.sin(out), Math.cos(ang) + Math.cos(out)) + R(-0.5, 0.5);
    if (y > GROUND - 14) a = -Math.PI / 2 + R(-0.9, 0.9);
    const L = R(8, 11.5) * scale;
    const w = L * R(0.32, 0.42);
    const spread = R(0.6, 1.0);
    const o: P = [x, y];
    const d = leaf(o, a - spread, L * R(0.8, 1), w) + leaf(o, a, L * R(0.9, 1.08), w) + leaf(o, a + spread, L * R(0.8, 1), w);
    // Light from the upper left: lower and right clusters sit darker.
    const v = clamp(((y - (cy - ry)) / (2 * ry)) * 0.6 + ((x - cx) / rx * 0.5 + 0.5) * 0.4, 0, 1);
    const inner = 1 - Math.hypot((x - cx) / rx, (y - cy) / ry);
    const back = rnd() < 0.3 + clamp(inner, 0, 1) * 0.45;
    if (back) {
      add({ kind: "leaf", layer: LAYER.back, d, tone: rnd() < 0.1 ? 1 : 0, g, limb });
    } else {
      add({ kind: "leaf", layer: LAYER.front, d, tone: v < 0.34 ? 2 : v > 0.62 && rnd() < 0.3 ? 4 : 3, g, limb });
    }
    return [x, y] as P;
  };
  const dress = (q: Curve, g: number, limb: number) => {
    const tn = qtan(q, 1);
    const ang = Math.atan2(tn[1], tn[0]);
    let k = 1;
    anchors.push(cluster(qpt(q, 1), ang, g + 0.001 * k++, 1.8, limb));
    anchors.push(cluster(qpt(q, 0.7), ang, g + 0.001 * k++, 2.4, limb));
    if (rnd() < 0.6) cluster(qpt(q, 0.4), ang, g + 0.001 * k++, 3, limb);
    if (rnd() < 0.5) cluster(qpt(q, 0.95), ang, g + 0.001 * k++, 4.5, limb);
  };

  // Each limb is one self-contained growth group: its two segments, a tuft of
  // leaves at its tip, then each child branch dressed with leaves, then that
  // child's twigs dressed the same way, then the next child. Everything stays
  // inside [g0, g0 + 0.9), so a limb never shows a bare twig: its leaves share
  // its growth window.
  const limbBase: { x: number; y: number }[] = [];
  limbs.forEach((L, j) => {
    const g0 = 1 + j;
    const A = qpt(trunk, L.t);
    limbBase.push({ x: A[0], y: A[1] });
    const sθ = Math.sin(L.th);
    const cθ = Math.cos(L.th);
    const rr = R(0.62, 0.88);
    const tx = cx + rx * sθ * rr + R(-2, 2);
    const ty = cy - ry * cθ * rr * 0.92 + (cr.style === 3 ? ry * 0.5 * cθ ** 6 : 0) + R(-2, 2);
    const q = curve(A, [tx, ty], R(-0.17, 0.17), 0.1);
    add({ kind: "branch", layer: LAYER.branch, d: qsub(q, 0, 0.5), depth: 1, w: 1.5, g: g0, limb: j });
    add({ kind: "branch", layer: LAYER.branch, d: qsub(q, 0.5, 1), depth: 1, w: 1.2, g: g0 + 0.01, limb: j });
    // The limb is dressed at its tip as it appears, so no page boundary can ever
    // leave a freshly started limb standing as a bare twig.
    dress(q, g0 + 0.01, j);
    let side = rnd() < 0.5 ? 1 : -1;
    for (let c = 0; c < 3; c++) {
      side = -side;
      const gc = g0 + 0.02 + c * 0.28 + R(0, 0.004);
      const child = spawn(q, [0.4, 0.66, 0.92][c], side, 0.42, 2, gc, j);
      dress(child.q, gc, j);
      for (let w = 0; w < 2; w++) {
        side = -side;
        const gw = gc + 0.02 + w * 0.02;
        const twig = spawn(child.q, w === 0 ? 0.5 : 0.86, side, 0.45, 3, gw, j);
        dress(twig.q, gw, j);
      }
    }
  });

  // The last limb keeps its last six leaves for the final page.
  const lastLimb = limbs.length - 1;
  const leaves = drafts.filter((d) => d.kind === "leaf" && d.limb === lastLimb).sort((a, b) => a.g - b.g || a.n - b.n);
  for (const l of leaves.slice(-6)) l.mature = true;

  // Maturity: one offset sun disc and a handful of quiet blossoms.
  const sr = R(14, 18);
  const sideSun = rnd() < 0.5 ? -1 : 1;
  const sx = clamp(cx + sideSun * rx * 0.5, sr + 2, TREE_W - sr - 2);
  const sy = clamp(cy - ry * 0.3, sr + 2, 60);
  add({ kind: "sun", layer: LAYER.sun, d: circ(sx, sy, sr), g: 20, mature: true });
  // Few and never crowded: candidates closer than 6 units to a kept blossom
  // are dropped, so the crown keeps its calm instead of filling with pinwheels.
  const nB = 7 + Math.floor(rnd() * 5);
  const spots: P[] = [];
  for (let tries = 0; tries < 30 && spots.length < nB; tries++) {
    const a = anchors[Math.floor(rnd() * anchors.length)];
    const bx = a[0] + R(-3, 3);
    const by = a[1] + R(-3, 3);
    if (spots.some(([px, py]) => Math.hypot(px - bx, py - by) < 6)) continue;
    spots.push([bx, by]);
  }
  spots.forEach(([bx, by], k) => {
    const pr = R(1.0, 1.3);
    const rot = R(0, Math.PI * 2);
    let d = "";
    for (let i = 0; i < 5; i++) {
      const t = rot + (i * Math.PI * 2) / 5;
      d += circ(bx + Math.cos(t) * pr * 1.05, by + Math.sin(t) * pr * 1.05, pr);
    }
    add({ kind: "petals", layer: LAYER.bloom, d, w: 0.35, g: 21 + k * 0.01, mature: true });
    add({ kind: "heart", layer: LAYER.bloom, d: circ(bx, by, pr * 0.45), g: 21.005 + k * 0.01, mature: true });
  });
  return finish(drafts, baseX, limbBase);
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
  add({ kind: "leaf", layer: LAYER.front, d: circ(sp[0] + 3, sp[1] - 1, 4.2), tone: 3, g: -2, base: true, hideAt: EPS });

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
      d: circ(cx + Math.cos(a) * rx * 0.55, cy + 0.1 * ry + Math.sin(a) * ry * 0.5, m * R(0.58, 0.72)),
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
      d: circ(T[0] + R(-3, 3), T[1] + R(-2, 4), m * R(0.34, 0.46)),
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
    add({ kind: "petals", layer: LAYER.bloom, d: circ(bx, by, 3.2), w: 0.9, g: 20 + k, mature: true });
    add({ kind: "heart", layer: LAYER.bloom, d: circ(bx, by, 1.4), g: 20.5 + k, mature: true });
  }
  return finish(drafts, baseX, limbBase);
}

// ---------------------------------------------------------------- public API

const geometry = new Map<string, Geometry>();
const models = new Map<string, TreeModel>();
const CACHE_MAX = 256;

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
  const t = Math.max(0, Math.floor(total));
  const key = `${detail}|${t}|${seed}`;
  const hit = models.get(key);
  if (hit) return hit;
  const gkey = `${detail}|${seed}`;
  const geo = geometry.get(gkey) ?? remember(geometry, gkey, detail === "mini" ? buildMini(seed) : buildFull(seed));
  const pages = Math.max(t, 1);
  const elements: TreeElement[] = geo.elements.map((e) => ({
    ...e,
    page: e.threshold === 0 ? -1 : Math.min(pages - 1, Math.max(0, Math.ceil(e.threshold * pages - EPS) - 1)),
  }));
  return remember(models, key, { seed, total: t, detail, baseX: geo.baseX, ground: geo.ground, limbs: geo.limbs, elements });
}
