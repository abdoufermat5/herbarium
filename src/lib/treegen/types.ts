// Shared contract for the reading-path tree generator.
//
// `skeleton.ts` grows a deterministic 3D skeleton from a seed; `project.ts`
// flattens it into the 90 x 100 viewBox; `tree.ts` turns that into the
// thresholded, limb-owned TreeModel the SVG renders. Keep this file free of
// imports so every layer can depend on it without cycles.

export type V3 = [number, number, number];

export interface SkeletonBranch {
  /** 0 trunk, 1 limb, 2 branch, 3 twig. */
  level: number;
  /** Top-level limb this branch belongs to; -1 for the trunk. */
  limb: number;
  /** Index in Skeleton.branches, or -1 for the trunk. */
  parent: number;
  /** Base followed by one point per section (length = sections + 1), y up. */
  points: V3[];
  /** Radius at each point; same length as `points`. */
  radii: number[];
  /** Polyline length. */
  length: number;
}

export interface LeafAnchor {
  /** Cluster centre in world space. */
  at: V3;
  /** Unit direction the cluster points, away from its branch. */
  dir: V3;
  /** Branch index the cluster grows from. */
  parent: number;
  limb: number;
  /** Parent branch level (1 = limb tip, 3 = twig). */
  level: number;
  /** Leaf cluster length in world units, roughly 6-10. */
  size: number;
  /** True for the one anchor per limb that must appear before its branches. */
  tip: boolean;
}

export interface Skeleton {
  seed: string;
  /** Growth order: trunk first, then breadth-first. */
  branches: SkeletonBranch[];
  /** Anchor points for leaf clusters, not individual leaves. */
  leaves: LeafAnchor[];
  /** Trunk foot, world space (y = 0). */
  base: V3;
  /** Crown top, world space. */
  height: number;
  /** Crown box used to frame the tree. */
  crown: { cx: number; cy: number; cz: number; rx: number; ry: number };
}
