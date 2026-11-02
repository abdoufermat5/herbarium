// Orthographic flattening of the 3D skeleton into the 90 x 100 viewBox.

import type { Skeleton, V3 } from "./types";

export interface P3 {
  x: number;
  y: number;
  z: number;
}

export interface ProjectedBranch {
  points: P3[];
  radii: number[];
}

export interface ProjectedLeaf {
  x: number;
  y: number;
  z: number;
  angle: number;
  size: number;
  level: number;
  tip: boolean;
  parent: number;
  limb: number;
}

export interface Projection {
  baseX: number;
  ground: number;
  scale: number;
  branches: ProjectedBranch[];
  leaves: ProjectedLeaf[];
  limbs: { x: number; y: number }[];
  bounds: { left: number; top: number; right: number; bottom: number };
}

const YAW = -0.32;
const PITCH = 0.12;
const COS_YAW = Math.cos(YAW);
const SIN_YAW = Math.sin(YAW);
const COS_PITCH = Math.cos(PITCH);
const SIN_PITCH = Math.sin(PITCH);

const VIEW_W = 90;
const GROUND_Y = 90;
const CENTRE_X = 45;
const FIT_W = 84;
const FIT_H = 86;
const MIN_SCALE = 0.6;
const MAX_SCALE = 1.8;
const BASE_MIN = 36;
const BASE_MAX = 54;
const LEAF_PROBE = 4;
const LEAF_MARGIN = 0.5;
const MIN_DIR = 0.5;
const EPS = 1e-6;

interface Rotated {
  x: number;
  y: number;
  z: number;
}

/** Rotate world space (y up) by yaw around Y, then pitch around X. */
function rotate([x, y, z]: V3): Rotated {
  const rx = x * COS_YAW + z * SIN_YAW;
  const rz = z * COS_YAW - x * SIN_YAW;
  return {
    x: rx,
    y: y * COS_PITCH + rz * SIN_PITCH,
    z: rz * COS_PITCH - y * SIN_PITCH,
  };
}

/**
 * Flatten a 3D skeleton into the 90 x 100 viewBox with a fixed orthographic
 * camera. Branch points are fitted as centres while each leaf anchor reserves
 * its cluster length, so the fitted crown keeps leaf clearance; the crown is
 * centred and the trunk foot kept in the middle band of the ground.
 */
export function projectSkeleton(skel: Skeleton): Projection {
  const rotatedBranches = skel.branches.map((b) => b.points.map(rotate));
  const rotatedLeaves = skel.leaves.map((a) => rotate(a.at));
  const base = rotate(skel.base);
  const crown = rotate([skel.crown.cx, skel.crown.cy, skel.crown.cz]);

  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  const track = (p: Rotated) => {
    if (p.x < minX) minX = p.x;
    if (p.x > maxX) maxX = p.x;
    if (p.y < minY) minY = p.y;
    if (p.y > maxY) maxY = p.y;
  };
  for (const points of rotatedBranches) for (const p of points) track(p);
  // A leaf anchor is its cluster centre, so reserve the cluster length on both
  // axes; fitting centres alone lets a mature crown overflow the viewBox.
  rotatedLeaves.forEach((p, i) => {
    const s = skel.leaves[i].size + LEAF_MARGIN;
    track({ x: p.x - s, y: p.y - s, z: p.z });
    track({ x: p.x + s, y: p.y + s, z: p.z });
  });

  const empty = minX === Infinity;
  if (empty) {
    minX = 0;
    maxX = 0;
    minY = 0;
    maxY = 0;
  }

  const width = maxX - minX;
  const height = maxY - minY;
  const fit = Math.min(width > EPS ? FIT_W / width : Infinity, height > EPS ? FIT_H / height : Infinity);
  const scale = empty ? 1 : Math.min(MAX_SCALE, Math.max(MIN_SCALE, Number.isFinite(fit) ? fit : 1));

  const placeX = (x: number) => CENTRE_X + (x - crown.x) * scale;
  const placeY = (y: number) => GROUND_Y - (y - base.y) * scale;

  const baseX0 = placeX(base.x);
  let dx = baseX0 < BASE_MIN ? BASE_MIN - baseX0 : baseX0 > BASE_MAX ? BASE_MAX - baseX0 : 0;
  const lo = -placeX(minX);
  const hi = VIEW_W - placeX(maxX);
  dx = lo <= hi ? Math.min(hi, Math.max(lo, dx)) : (lo + hi) / 2;

  const branches: ProjectedBranch[] = skel.branches.map((b, i) => ({
    points: rotatedBranches[i].map((p) => ({ x: placeX(p.x) + dx, y: placeY(p.y), z: p.z })),
    radii: b.radii.map((r) => r * scale),
  }));

  const limbs: { x: number; y: number }[] = [];
  skel.branches.forEach((b, i) => {
    if (b.level !== 1 || b.limb < 0) return;
    const foot = branches[i].points[0];
    limbs[b.limb] = { x: foot.x, y: foot.y };
  });

  const crownX = CENTRE_X + dx;
  const crownY = placeY(crown.y);

  const leaves: ProjectedLeaf[] = skel.leaves.map((a, i) => {
    const at = rotatedLeaves[i];
    const tip = rotate([
      a.at[0] + a.dir[0] * LEAF_PROBE,
      a.at[1] + a.dir[1] * LEAF_PROBE,
      a.at[2] + a.dir[2] * LEAF_PROBE,
    ]);
    const x = placeX(at.x) + dx;
    const y = placeY(at.y);
    const vx = (tip.x - at.x) * scale;
    const vy = -(tip.y - at.y) * scale;
    const angle = Math.hypot(vx, vy) < MIN_DIR ? Math.atan2(y - crownY, x - crownX) : Math.atan2(vy, vx);
    return {
      x,
      y,
      z: at.z,
      angle,
      size: a.size * scale,
      level: a.level,
      tip: a.tip,
      parent: a.parent,
      limb: a.limb,
    };
  });

  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;
  const grow = (x: number, y: number) => {
    if (x < left) left = x;
    if (x > right) right = x;
    if (y < top) top = y;
    if (y > bottom) bottom = y;
  };
  for (const b of branches) for (const p of b.points) grow(p.x, p.y);
  for (const l of leaves) grow(l.x, l.y);
  if (left === Infinity) left = top = right = bottom = 0;

  return {
    baseX: baseX0 + dx,
    ground: GROUND_Y,
    scale,
    branches,
    leaves,
    limbs,
    bounds: { left, top, right, bottom },
  };
}
