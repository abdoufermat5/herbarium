// Optional 3D hero for the reading-path view. The SVG tree stays the source of
// truth and the fallback; this scene is built from the same deterministic
// skeleton and the same TreeModel thresholds, so limbs map to pages exactly as
// the 2D tree does. `three` lives only in this lazily imported module.

import {
  AmbientLight,
  BufferAttribute,
  BufferGeometry,
  CanvasTexture,
  Color,
  DirectionalLight,
  DoubleSide,
  DynamicDrawUsage,
  Group,
  HemisphereLight,
  InstancedMesh,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  OrthographicCamera,
  PlaneGeometry,
  Quaternion,
  Raycaster,
  SRGBColorSpace,
  Scene,
  Shape,
  ShapeGeometry,
  SphereGeometry,
  Sprite,
  SpriteMaterial,
  Vector2,
  Vector3,
  WebGLRenderer,
} from "three";
import { growSkeleton } from "./skeleton";
import type { LeafAnchor, Skeleton, V3 } from "./types";
import { growTree, type TreeElement, type TreeModel } from "../tree";

export interface HeroColors {
  ink: string;
  leaf: string;
  sage: string;
  sageSoft: string;
  teal: string;
  tealSoft: string;
  rose: string;
  roseSoft: string;
  amber: string;
  clay: string;
}

export interface HeroOptions {
  seed: string;
  read: number;
  total: number;
  /** Height in CSS px; the canvas is 0.9 x this wide. */
  size: number;
  animate: boolean;
  colors: HeroColors;
  /** Scale the whole tree in once (mature moment on first visit). */
  entrance?: boolean;
}

export interface HeroHandle {
  setProgress(read: number, total: number): void;
  setHighlight(page: number | null): void;
  setColors(colors: HeroColors): void;
  resize(size: number): void;
  setAnimate(animate: boolean): void;
  setVisible(visible: boolean): void;
  /** Page index under the pointer, or -1. */
  pick(clientX: number, clientY: number): number;
  dispose(): void;
}

const ASPECT = 0.9;
const DPR_MAX = 1.75;
const TWEEN_MS = 500;
const ENTRANCE_FROM = 0.86;
const BRANCH_FROM = 0.8;
const LEAF_FROM = 0.7;
const EPS = 1e-6;
const RADIAL_SEGMENTS = 5;
const BARK_SCALE = 0.55;
const HIT_SCALE = 2.6;
const YAW = 0.35;
const ELEVATION = 0.12;
/** Unit vector from the tree towards the fixed camera. */
const VIEW = new Vector3(
  Math.sin(YAW) * Math.cos(ELEVATION),
  Math.sin(ELEVATION),
  Math.cos(YAW) * Math.cos(ELEVATION),
).normalize();

interface BranchView {
  group: Group;
  mesh: Mesh;
  page: number;
  threshold: number;
  visible: boolean;
  /** Timestamp when the grow-in tween started, or 0. */
  appear: number;
}

interface LeafView {
  position: Vector3;
  quaternion: Quaternion;
  size: number;
  threshold: number;
  page: number;
  tone: number;
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
const finite = (value: number, fallback: number) => (Number.isFinite(value) ? value : fallback);
const easeOut = (t: number) => 1 - Math.pow(1 - t, 3);
const now = () => (typeof performance !== "undefined" ? performance.now() : Date.now());

const TONES: Array<keyof HeroColors> = ["sageSoft", "tealSoft", "sage", "leaf", "teal"];

function toneColor(tone: number, colors: HeroColors): Color {
  const key = TONES[clamp(Math.round(tone), 0, TONES.length - 1)] ?? "leaf";
  return new Color(colors[key]);
}

function seedHash(text: string): number {
  let hash = 0;
  for (let i = 0; i < text.length; i++) hash = (Math.imul(hash, 31) + text.charCodeAt(i)) >>> 0;
  return hash;
}

/**
 * A tapered tube through `points` with per-point `radii`. Positions are local to
 * `origin`; the caller places a Group at that base. Frames use parallel
 * transport so the five-sided rings do not twist along the limb.
 */
function makeTube(points: V3[], radii: number[], origin: V3, thickness = 1): BufferGeometry {
  const geometry = new BufferGeometry();
  const count = points.length;
  if (count < 2) return geometry;

  const radial = RADIAL_SEGMENTS;
  const positions = new Float32Array(count * radial * 3);
  const indices: number[] = [];
  const base = new Vector3(origin[0], origin[1], origin[2]);
  const tangent = new Vector3();
  const normal = new Vector3();
  const binormal = new Vector3();
  const ref = new Vector3();
  const offset = new Vector3();
  const point = new Vector3();

  for (let i = 0; i < count; i++) {
    const prev = points[Math.max(0, i - 1)];
    const next = points[Math.min(count - 1, i + 1)];
    tangent.set(next[0] - prev[0], next[1] - prev[1], next[2] - prev[2]);
    if (tangent.lengthSq() < 1e-12) {
      // Duplicate points: look for the nearest non-zero neighbour direction.
      for (let k = i + 1; k < count; k++) {
        tangent.set(points[k][0] - points[i][0], points[k][1] - points[i][1], points[k][2] - points[i][2]);
        if (tangent.lengthSq() >= 1e-12) break;
      }
      if (tangent.lengthSq() < 1e-12) {
        for (let k = i - 1; k >= 0; k--) {
          tangent.set(points[k][0] - points[i][0], points[k][1] - points[i][1], points[k][2] - points[i][2]);
          if (tangent.lengthSq() >= 1e-12) break;
        }
      }
      if (tangent.lengthSq() < 1e-12) tangent.set(0, 1, 0);
    }
    tangent.normalize();

    if (i === 0) {
      ref.set(Math.abs(tangent.y) > 0.9 ? 1 : 0, Math.abs(tangent.y) > 0.9 ? 0 : 1, 0);
      normal.crossVectors(ref, tangent).normalize();
      if (normal.lengthSq() < 0.5) normal.set(1, 0, 0);
    } else {
      normal.addScaledVector(tangent, -normal.dot(tangent));
      if (normal.lengthSq() < 1e-8) {
        ref.set(Math.abs(tangent.y) > 0.9 ? 1 : 0, Math.abs(tangent.y) > 0.9 ? 0 : 1, 0);
        normal.crossVectors(ref, tangent).normalize();
      }
      normal.normalize();
    }
    binormal.crossVectors(tangent, normal).normalize();

    const radius = (radii[Math.min(i, radii.length - 1)] ?? 0.1) * thickness * BARK_SCALE;
    point.set(points[i][0], points[i][1], points[i][2]).sub(base);
    for (let j = 0; j < radial; j++) {
      const angle = (j / radial) * Math.PI * 2;
      offset.copy(normal).multiplyScalar(Math.cos(angle)).addScaledVector(binormal, Math.sin(angle));
      const at = (i * radial + j) * 3;
      positions[at] = point.x + offset.x * radius;
      positions[at + 1] = point.y + offset.y * radius;
      positions[at + 2] = point.z + offset.z * radius;
    }
  }

  for (let i = 0; i < count - 1; i++) {
    for (let j = 0; j < radial; j++) {
      const a = i * radial + j;
      const b = i * radial + ((j + 1) % radial);
      const c = (i + 1) * radial + j;
      const d = (i + 1) * radial + ((j + 1) % radial);
      indices.push(a, b, d, a, d, c);
    }
  }

  geometry.setAttribute("position", new BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

/** Teardrop in XY: origin at the stem, tip at (0, 1), normal +Z. */
function makeLeafGeometry(): ShapeGeometry {
  const shape = new Shape();
  shape.moveTo(0, 0);
  shape.bezierCurveTo(0.36, 0.26, 0.36, 0.74, 0, 1);
  shape.bezierCurveTo(-0.36, 0.74, -0.36, 0.26, 0, 0);
  return new ShapeGeometry(shape, 8);
}

function makeShadowTexture(): CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 256;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    const gradient = ctx.createRadialGradient(128, 128, 8, 128, 128, 126);
    gradient.addColorStop(0, "rgba(0, 0, 0, 0.34)");
    gradient.addColorStop(0.55, "rgba(0, 0, 0, 0.16)");
    gradient.addColorStop(1, "rgba(0, 0, 0, 0)");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, 256, 256);
  }
  return new CanvasTexture(canvas);
}

function makeSunTexture(): CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 128;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    const gradient = ctx.createRadialGradient(64, 64, 2, 64, 64, 62);
    gradient.addColorStop(0, "rgba(255, 255, 255, 0.95)");
    gradient.addColorStop(0.35, "rgba(255, 255, 255, 0.55)");
    gradient.addColorStop(1, "rgba(255, 255, 255, 0)");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, 128, 128);
  }
  return new CanvasTexture(canvas);
}

/** Local Y along `dir`, local Z outward horizontal, then fan around Y. */
function leafQuaternion(dir: Vector3, at: V3, part: number): Quaternion {
  const y = dir.clone().normalize();
  // Face the fixed camera, with a little outward bias so the crown keeps depth
  // instead of reading as a flat billboard.
  const outward = new Vector3(at[0], 0, at[2]);
  if (outward.lengthSq() < 1e-8) outward.set(0, 0, 1);
  outward.normalize();
  const facing = VIEW.clone().addScaledVector(outward, 0.35).normalize();
  const z = facing.clone().addScaledVector(y, -facing.dot(y));
  if (z.lengthSq() < 1e-8) z.set(0, 0, 1).addScaledVector(y, -y.z);
  if (z.lengthSq() < 1e-8) z.set(1, 0, 0);
  z.normalize();
  const x = new Vector3().crossVectors(y, z).normalize();
  const basis = new Matrix4().makeBasis(x, y, z);
  const quaternion = new Quaternion().setFromRotationMatrix(basis);
  // Fan the three cards around their own normal, so they read as a cluster from
  // the camera instead of stacking on the same long axis.
  const spin = new Quaternion().setFromAxisAngle(new Vector3(0, 0, 1), (part - 1) * 0.55);
  return quaternion.multiply(spin);
}

function blossomBasis(dir: Vector3): { u: Vector3; v: Vector3 } {
  const normal = new Vector3(dir.x, dir.y * 0.35 + 0.65, dir.z).normalize();
  const u = new Vector3(1, 0, 0).addScaledVector(normal, -normal.x);
  if (u.lengthSq() < 1e-6) u.set(0, 0, 1).addScaledVector(normal, -normal.z);
  if (u.lengthSq() < 1e-6) u.set(0, 1, 0);
  u.normalize();
  const v = new Vector3().crossVectors(normal, u).normalize();
  return { u, v };
}

function fitSkeleton(skel: Skeleton): { center: Vector3; radius: number } {
  let minX = Infinity;
  let minY = Infinity;
  let minZ = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  let maxZ = -Infinity;
  const expand = (x: number, y: number, z: number) => {
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (z < minZ) minZ = z;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
    if (z > maxZ) maxZ = z;
  };
  for (const branch of skel.branches) for (const point of branch.points) expand(point[0], point[1], point[2]);
  for (const leaf of skel.leaves) expand(leaf.at[0], leaf.at[1], leaf.at[2]);
  expand(skel.base[0], skel.base[1], skel.base[2]);
  if (minX === Infinity) {
    minX = minY = minZ = 0;
    maxX = maxY = maxZ = 0;
  }

  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;
  const cz = (minZ + maxZ) / 2;
  let radius = 0;
  let leafSize = 0;
  const reach = (x: number, y: number, z: number) => {
    const distance = Math.hypot(x - cx, y - cy, z - cz);
    if (distance > radius) radius = distance;
  };
  for (const branch of skel.branches) for (const point of branch.points) reach(point[0], point[1], point[2]);
  for (const leaf of skel.leaves) {
    reach(leaf.at[0], leaf.at[1], leaf.at[2]);
    if (leaf.size > leafSize) leafSize = leaf.size;
  }
  reach(skel.base[0], skel.base[1], skel.base[2]);
  return { center: new Vector3(cx, cy, cz), radius: Math.max(1, radius + leafSize) };
}

/** Highest anchors with minimum spacing; spacing relaxes until at least six fit. */
function pickBlossoms(skel: Skeleton, count: number): LeafAnchor[] {
  if (skel.leaves.length === 0) return [];
  const sorted = [...skel.leaves].sort(
    (a, b) => b.at[1] - a.at[1] || a.at[0] - b.at[0] || a.at[2] - b.at[2],
  );
  const target = Math.min(count, sorted.length);
  let spacing = Math.max(3, skel.crown.rx * 0.24);
  for (let attempt = 0; attempt < 5; attempt++) {
    const picked: LeafAnchor[] = [];
    for (const leaf of sorted) {
      if (picked.length >= target) break;
      let clear = true;
      for (const other of picked) {
        const dx = leaf.at[0] - other.at[0];
        const dy = leaf.at[1] - other.at[1];
        const dz = leaf.at[2] - other.at[2];
        if (dx * dx + dy * dy + dz * dz < spacing * spacing) {
          clear = false;
          break;
        }
      }
      if (clear) picked.push(leaf);
    }
    if (picked.length >= Math.min(6, target) || attempt === 4) return picked;
    spacing *= 0.7;
  }
  return [];
}

export function createHero(canvas: HTMLCanvasElement, options: HeroOptions): HeroHandle {
  const seed = options.seed;
  let size = clamp(finite(options.size, 360), 1, 8192);
  let total = Math.max(0, Math.floor(finite(options.total, 0)));
  let progress = total > 0 ? clamp(finite(options.read, 0), 0, total) / total : 0;
  let animate = !!options.animate;
  const entrance = !!options.entrance;
  let visible = true;
  let disposed = false;
  let contextLost = false;
  let highlight: number | null = null;
  let colors: HeroColors = { ...options.colors };

  const skel = growSkeleton(seed);
  let model: TreeModel = growTree(seed, total, "full");

  const renderer = new WebGLRenderer({
    canvas,
    antialias: true,
    alpha: true,
    powerPreference: "low-power",
  });
  renderer.setPixelRatio(Math.min(typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1, DPR_MAX));
  renderer.setSize(size * ASPECT, size, false);
  renderer.outputColorSpace = SRGBColorSpace;
  renderer.setClearColor(0x000000, 0);

  const scene = new Scene();
  const fit = fitSkeleton(skel);
  const center = fit.center;
  const radius = fit.radius;
  const distance = radius * 3;
  const halfH = radius * 1.08;
  const halfW = halfH * ASPECT;
  const camera = new OrthographicCamera(-halfW, halfW, halfH, -halfH, 0.1, distance + radius * 4);
  const viewDir = new Vector3(
    Math.sin(YAW) * Math.cos(ELEVATION),
    Math.sin(ELEVATION),
    Math.cos(YAW) * Math.cos(ELEVATION),
  ).normalize();
  camera.position.copy(center).addScaledVector(viewDir, distance);
  camera.lookAt(center.x, center.y + radius * 0.05, center.z);
  camera.updateProjectionMatrix();

  scene.add(new HemisphereLight(0xfff6e8, 0x6f6154, 0.9));
  const keyLight = new DirectionalLight(0xfff1d6, 1.35);
  keyLight.position.set(center.x - radius * 1.1, center.y + radius * 1.6, center.z + radius * 0.9);
  keyLight.target.position.copy(center);
  scene.add(keyLight, keyLight.target);
  scene.add(new AmbientLight(0xffffff, 0.28));

  const barkMat = new MeshStandardMaterial({
    color: colors.ink,
    roughness: 0.9,
    metalness: 0,
    flatShading: true,
  });
  const barkDimMat = barkMat.clone();
  barkDimMat.color.copy(barkMat.color).multiplyScalar(0.45);
  const hitMat = new MeshBasicMaterial({
    transparent: true,
    opacity: 0,
    depthWrite: false,
    visible: false,
    side: DoubleSide,
  });
  const leafMat = new MeshBasicMaterial({
    color: 0xffffff,
    side: DoubleSide,
    toneMapped: false,
  });
  const blossomMat = new MeshStandardMaterial({
    color: 0xffffff,
    roughness: 0.9,
    metalness: 0,
    flatShading: true,
  });
  const moundMat = new MeshStandardMaterial({
    color: colors.clay,
    roughness: 1,
    metalness: 0,
    flatShading: true,
  });
  const seedMat = new MeshStandardMaterial({
    color: colors.ink,
    roughness: 0.85,
    metalness: 0,
    flatShading: true,
  });
  const sproutMats = [
    new MeshStandardMaterial({
      color: colors.sage,
      roughness: 0.9,
      metalness: 0,
      side: DoubleSide,
      flatShading: true,
    }),
    new MeshStandardMaterial({
      color: colors.leaf,
      roughness: 0.9,
      metalness: 0,
      side: DoubleSide,
      flatShading: true,
    }),
  ];
  const sunTexture = makeSunTexture();
  const sunMat = new SpriteMaterial({
    map: sunTexture,
    transparent: true,
    depthWrite: false,
    opacity: 0.45,
    color: colors.amber,
  });
  const shadowTexture = makeShadowTexture();
  const shadowMat = new MeshBasicMaterial({
    map: shadowTexture,
    transparent: true,
    depthWrite: false,
    opacity: 0.6,
  });

  const leafGeometry = makeLeafGeometry();
  const blossomGeometry = new SphereGeometry(1, 8, 6);
  const seedGeometry = new SphereGeometry(1, 12, 8);
  const moundGeometry = new SphereGeometry(1, 18, 10);
  const shadowGeometry = new PlaneGeometry(1, 1);
  shadowGeometry.rotateX(-Math.PI / 2);

  const baseGroup = new Group();
  scene.add(baseGroup);
  const shadowMesh = new Mesh(shadowGeometry, shadowMat);
  shadowMesh.scale.set(radius * 1.2, 1, radius * 1.2);
  shadowMesh.position.set(0, 0.005, 0);
  shadowMesh.renderOrder = -1;
  baseGroup.add(shadowMesh);
  const moundMesh = new Mesh(moundGeometry, moundMat);
  moundMesh.scale.set(radius * 0.22, radius * 0.06, radius * 0.22);
  moundMesh.position.set(0, 0, 0);
  baseGroup.add(moundMesh);

  const sun = new Sprite(sunMat);
  sun.scale.set(radius * 0.9, radius * 0.9, 1);
  sun.position.copy(center).addScaledVector(viewDir, -radius * 1.35);
  sun.position.y += radius * 0.35;
  sun.visible = false;
  scene.add(sun);

  const treeGroup = new Group();
  treeGroup.scale.setScalar(entrance && animate ? ENTRANCE_FROM : 1);
  scene.add(treeGroup);

  let branchGroups: BranchView[] = [];
  let branchMeshes: Mesh[] = [];
  let hitProxies: Mesh[] = [];
  let branchGeometries: BufferGeometry[] = [];
  let leafMesh: InstancedMesh | null = null;
  let leafEntries: LeafView[] = [];
  let leafPages: number[] = [];
  let leafBaseColors: Color[] = [];
  let leafScales: number[] = [];
  let leafAppears: number[] = [];
  let leafVisible: boolean[] = [];
  let blossomMesh: InstancedMesh | null = null;
  let blossomCount = 0;
  let seedMesh: Mesh | null = null;
  let sproutGroup: Group | null = null;

  const matrix = new Matrix4();
  const scaleVector = new Vector3();
  const tint = new Color();
  const raycaster = new Raycaster();
  const pointer = new Vector2();
  let raf = 0;
  let entranceStart = 0;
  let renderedOnce = false;

  function writeLeaf(index: number) {
    if (!leafMesh) return;
    const entry = leafEntries[index];
    matrix.compose(entry.position, entry.quaternion, scaleVector.setScalar(entry.size * leafScales[index]));
    leafMesh.setMatrixAt(index, matrix);
  }

  function applyLeafColors() {
    if (!leafMesh) return;
    const page = highlight;
    for (let i = 0; i < leafEntries.length; i++) {
      tint.copy(leafBaseColors[i]);
      if (page !== null && leafEntries[i].page !== page) tint.multiplyScalar(0.5);
      leafMesh.setColorAt(i, tint);
    }
    if (leafMesh.instanceColor) leafMesh.instanceColor.needsUpdate = true;
  }

  function applyHighlight() {
    const page = highlight;
    for (const branch of branchGroups) {
      branch.mesh.material = page === null || branch.page === page ? barkMat : barkDimMat;
    }
    applyLeafColors();
  }

  function clearTree() {
    for (const geometry of branchGeometries) geometry.dispose();
    branchGeometries = [];
    branchGroups = [];
    branchMeshes = [];
    hitProxies = [];
    leafMesh?.dispose();
    blossomMesh?.dispose();
    leafMesh = null;
    leafEntries = [];
    leafPages = [];
    leafBaseColors = [];
    leafScales = [];
    leafAppears = [];
    leafVisible = [];
    blossomMesh = null;
    blossomCount = 0;
    seedMesh = null;
    sproutGroup = null;
    treeGroup.clear();
  }

  function buildBranches() {
    // One tube per skeleton branch: the 2D model splits the trunk and limbs for
    // growth, but restarting the tube frames at those cuts leaves a visible step.
    const byRef = new Map<number, TreeElement>();
    for (const element of model.elements) {
      if (element.kind !== "branch" || element.ref === undefined) continue;
      const current = byRef.get(element.ref);
      if (!current || element.threshold < current.threshold) byRef.set(element.ref, element);
    }

    for (const [ref, element] of byRef) {
      const branch = skel.branches[ref];
      if (!branch || branch.points.length < 2) continue;
      const points = branch.points;
      const radii = branch.radii;
      const origin = points[0];
      const geometry = makeTube(points, radii, origin);
      const mesh = new Mesh(geometry, barkMat);
      mesh.userData.page = element.page;
      mesh.userData.ref = ref;
      mesh.userData.depth = element.depth;

      const group = new Group();
      group.position.set(origin[0], origin[1], origin[2]);
      group.add(mesh);
      group.visible = false;
      treeGroup.add(group);

      branchGeometries.push(geometry);
      branchMeshes.push(mesh);
      branchGroups.push({
        group,
        mesh,
        page: element.page,
        threshold: element.threshold,
        visible: false,
        appear: 0,
      });

      if (element.depth === 1) {
        const hitGeometry = makeTube(points, radii, origin, HIT_SCALE);
        const hit = new Mesh(hitGeometry, hitMat);
        hit.userData.page = element.page;
        hit.userData.depth = element.depth;
        group.add(hit);
        hitProxies.push(hit);
        branchGeometries.push(hitGeometry);
      }
    }
  }

  function buildLeaves() {
    const elements: TreeElement[] = [];
    for (const element of model.elements) {
      if (element.kind !== "leaf" || element.ref === undefined || element.hideAt !== undefined) continue;
      if (!skel.leaves[element.ref]) continue;
      elements.push(element);
    }
    elements.sort((a, b) => a.threshold - b.threshold || a.i - b.i);

    const mesh = new InstancedMesh(leafGeometry, leafMat, Math.max(1, elements.length));
    mesh.instanceMatrix.setUsage(DynamicDrawUsage);
    mesh.frustumCulled = false;
    mesh.count = 0;
    leafMesh = mesh;
    leafEntries = [];
    leafPages = [];
    leafBaseColors = [];
    leafScales = [];
    leafAppears = [];
    leafVisible = [];

    let instance = 0;
    for (let i = 0; i < elements.length; i++) {
      const element = elements[i];
      const ref = element.ref;
      if (ref === undefined) continue;
      const anchor = skel.leaves[ref];
      if (!anchor) continue;
      const part = element.part ?? 0;
      const direction = new Vector3(anchor.dir[0], anchor.dir[1], anchor.dir[2]).normalize();
      const quaternion = leafQuaternion(direction, anchor.at, part);
      const position = new Vector3(anchor.at[0], anchor.at[1], anchor.at[2]);
      const leafSize = anchor.size * 1.05;
      const baseColor = toneColor(element.tone, colors);

      matrix.compose(position, quaternion, scaleVector.setScalar(leafSize));
      mesh.setMatrixAt(instance, matrix);
      mesh.setColorAt(instance, baseColor);
      leafEntries.push({
        position,
        quaternion,
        size: leafSize,
        threshold: element.threshold,
        page: element.page,
        tone: element.tone,
      });
      leafPages.push(element.page);
      leafBaseColors.push(baseColor);
      leafScales.push(1);
      leafAppears.push(0);
      leafVisible.push(false);
      instance++;
    }

    mesh.instanceMatrix.needsUpdate = true;
    if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
    treeGroup.add(mesh);
  }

  function buildBlossoms() {
    const anchors = pickBlossoms(skel, 6 + (seedHash(seed) % 3));
    const capacity = Math.max(1, anchors.length * 6);
    const mesh = new InstancedMesh(blossomGeometry, blossomMat, capacity);
    mesh.frustumCulled = false;
    mesh.count = anchors.length * 6;
    mesh.visible = false;
    blossomMesh = mesh;
    blossomCount = mesh.count;

    const petal = new Color(colors.rose);
    const heart = new Color(colors.amber);
    const identity = new Quaternion();
    const position = new Vector3();
    const scale = new Vector3();
    let index = 0;

    for (const anchor of anchors) {
      const direction = new Vector3(anchor.dir[0], anchor.dir[1], anchor.dir[2]).normalize();
      const centre = new Vector3(anchor.at[0], anchor.at[1], anchor.at[2]).addScaledVector(
        direction,
        anchor.size * 0.55,
      );
      const { u, v } = blossomBasis(direction);
      for (let petalIndex = 0; petalIndex < 5; petalIndex++) {
        const angle = (petalIndex / 5) * Math.PI * 2;
        position
          .copy(centre)
          .addScaledVector(u, Math.cos(angle) * 0.95)
          .addScaledVector(v, Math.sin(angle) * 0.95);
        matrix.compose(position, identity, scale.set(0.46, 0.38, 0.46));
        mesh.setMatrixAt(index, matrix);
        mesh.setColorAt(index, petal);
        index++;
      }
      matrix.compose(centre, identity, scale.set(0.3, 0.4, 0.3));
      mesh.setMatrixAt(index, matrix);
      mesh.setColorAt(index, heart);
      index++;
    }

    mesh.instanceMatrix.needsUpdate = true;
    if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
    treeGroup.add(mesh);
  }

  function buildBaseArt() {
    const seedSphere = new Mesh(seedGeometry, seedMat);
    const seedScale = radius * 0.045;
    seedSphere.scale.setScalar(seedScale);
    seedSphere.position.set(radius * 0.05, radius * 0.09, 0);
    seedSphere.visible = false;
    seedMesh = seedSphere;
    treeGroup.add(seedSphere);

    const sprout = new Group();
    const stemPoints = skel.branches[0]?.points.slice(0, 2) ?? [];
    const stemRadii = skel.branches[0]?.radii.slice(0, 2) ?? [];
    if (stemPoints.length >= 2) {
      const stemGeometry = makeTube(stemPoints, stemRadii, stemPoints[0]);
      const stem = new Mesh(stemGeometry, barkMat);
      sprout.add(stem);
      branchGeometries.push(stemGeometry);

      const tip = stemPoints[1];
      const top = new Vector3(tip[0], tip[1], tip[2]);
      const directions: V3[] = [
        [0.55, 0.55, 0.2],
        [-0.55, 0.55, -0.15],
      ];
      for (let i = 0; i < directions.length; i++) {
        const direction = new Vector3(directions[i][0], directions[i][1], directions[i][2]).normalize();
        const leaf = new Mesh(leafGeometry, sproutMats[i]);
        leaf.position.copy(top);
        leaf.quaternion.copy(leafQuaternion(direction, tip, i === 0 ? 0 : 1));
        leaf.scale.setScalar(radius * 0.1);
        sprout.add(leaf);
      }
    }
    sprout.visible = false;
    sproutGroup = sprout;
    treeGroup.add(sprout);
  }

  function buildTree() {
    buildBranches();
    buildLeaves();
    buildBlossoms();
    buildBaseArt();
  }

  function rebuildTree() {
    clearTree();
    buildTree();
    applyHighlight();
  }

  function renderOnce() {
    if (disposed || contextLost || !visible) return;
    try {
      renderer.render(scene, camera);
      renderedOnce = true;
    } catch (error) {
      contextLost = true;
      cancelRaf();
      if (!renderedOnce) throw error;
      canvas.dispatchEvent(new Event("webglcontextlost"));
    }
  }

  function cancelRaf() {
    if (raf !== 0) {
      if (typeof cancelAnimationFrame === "function") cancelAnimationFrame(raf);
      raf = 0;
    }
  }

  function startRaf() {
    if (raf !== 0 || !visible || disposed || contextLost || !animate) return;
    if (typeof requestAnimationFrame !== "function") return;
    raf = requestAnimationFrame(tick);
  }

  function tick(time: number) {
    raf = 0;
    if (disposed || contextLost || !visible) return;

    let active = false;
    if (entranceStart > 0) {
      const k = clamp((time - entranceStart) / TWEEN_MS, 0, 1);
      treeGroup.scale.setScalar(ENTRANCE_FROM + (1 - ENTRANCE_FROM) * easeOut(k));
      if (k < 1) active = true;
      else entranceStart = 0;
    }

    for (const branch of branchGroups) {
      if (branch.appear === 0) continue;
      const k = clamp((time - branch.appear) / TWEEN_MS, 0, 1);
      branch.group.scale.setScalar(BRANCH_FROM + (1 - BRANCH_FROM) * easeOut(k));
      if (k < 1) active = true;
      else {
        branch.appear = 0;
        branch.group.scale.setScalar(1);
      }
    }

    if (leafMesh) {
      let changed = false;
      for (let i = 0; i < leafEntries.length; i++) {
        if (leafAppears[i] === 0) continue;
        const k = clamp((time - leafAppears[i]) / TWEEN_MS, 0, 1);
        leafScales[i] = LEAF_FROM + (1 - LEAF_FROM) * easeOut(k);
        writeLeaf(i);
        changed = true;
        if (k < 1) active = true;
        else {
          leafAppears[i] = 0;
          leafScales[i] = 1;
          writeLeaf(i);
        }
      }
      if (changed) leafMesh.instanceMatrix.needsUpdate = true;
    }

    renderOnce();
    if (active && !disposed && !contextLost) raf = requestAnimationFrame(tick);
  }

  function applyGrowth(animateNew: boolean, paint = true) {
    const time = Math.max(now(), 0.001);
    let changed = false;
    let tweening = false;

    for (const branch of branchGroups) {
      const shown = progress + EPS >= branch.threshold;
      if (shown === branch.visible) continue;
      changed = true;
      branch.visible = shown;
      branch.group.visible = shown;
      if (!shown) {
        branch.appear = 0;
        branch.group.scale.setScalar(1);
        continue;
      }
      if (animate && animateNew && visible) {
        branch.appear = time;
        branch.group.scale.setScalar(BRANCH_FROM);
        tweening = true;
      } else {
        branch.appear = 0;
        branch.group.scale.setScalar(1);
      }
    }

    if (leafMesh) {
      let count = 0;
      while (count < leafEntries.length && leafEntries[count].threshold <= progress + EPS) count++;
      if (count !== leafMesh.count) {
        leafMesh.count = count;
        leafMesh.boundingSphere = null;
        changed = true;
      }
      let matricesChanged = false;
      for (let i = 0; i < count; i++) {
        if (leafVisible[i]) continue;
        leafVisible[i] = true;
        if (animate && animateNew && visible) {
          leafAppears[i] = time;
          leafScales[i] = LEAF_FROM;
          tweening = true;
        } else {
          leafAppears[i] = 0;
          leafScales[i] = 1;
        }
        writeLeaf(i);
        matricesChanged = true;
      }
      for (let i = count; i < leafEntries.length; i++) {
        if (!leafVisible[i]) continue;
        leafVisible[i] = false;
        leafAppears[i] = 0;
        leafScales[i] = 1;
      }
      if (matricesChanged) leafMesh.instanceMatrix.needsUpdate = true;
    }

    const mature = progress >= 1 - EPS;
    if (blossomMesh && blossomMesh.visible !== mature) {
      blossomMesh.visible = mature;
      changed = true;
    }
    if (sun.visible !== mature) {
      sun.visible = mature;
      changed = true;
    }
    const seedVisible = progress <= EPS;
    if (seedMesh && seedMesh.visible !== seedVisible) {
      seedMesh.visible = seedVisible;
      changed = true;
    }
    if (sproutGroup && sproutGroup.visible !== seedVisible) {
      sproutGroup.visible = seedVisible;
      changed = true;
    }

    if (tweening) startRaf();
    else if (changed && paint && visible) renderOnce();
  }

  function snapTweens() {
    entranceStart = 0;
    treeGroup.scale.setScalar(1);
    for (const branch of branchGroups) {
      branch.appear = 0;
      branch.group.scale.setScalar(1);
    }
    if (leafMesh) {
      let changed = false;
      for (let i = 0; i < leafEntries.length; i++) {
        if (leafAppears[i] === 0) continue;
        leafAppears[i] = 0;
        leafScales[i] = 1;
        writeLeaf(i);
        changed = true;
      }
      if (changed) leafMesh.instanceMatrix.needsUpdate = true;
    }
  }

  function setProgress(nextRead: number, nextTotal: number) {
    if (disposed) return;
    const next = Math.max(0, Math.floor(finite(nextTotal, 0)));
    const read = Math.max(0, finite(nextRead, 0));
    const nextProgress = next > 0 ? Math.min(read, next) / next : 0;
    let rebuilt = false;
    if (next !== total) {
      total = next;
      model = growTree(seed, total, "full");
      rebuildTree();
      rebuilt = true;
    }
    if (!rebuilt && nextProgress === progress) return;
    progress = nextProgress;
    applyGrowth(!rebuilt);
  }

  function setHighlight(next: number | null) {
    if (disposed) return;
    const page = next === null || !Number.isFinite(next) || next < 0 ? null : Math.floor(next);
    if (page === highlight) return;
    highlight = page;
    applyHighlight();
    if (visible) renderOnce();
  }

  function setColors(next: HeroColors) {
    if (disposed) return;
    colors = { ...next };
    barkMat.color.set(colors.ink);
    barkDimMat.color.copy(barkMat.color).multiplyScalar(0.45);
    moundMat.color.set(colors.clay);
    seedMat.color.set(colors.ink);
    sunMat.color.set(colors.amber);
    sproutMats[0].color.set(colors.sage);
    sproutMats[1].color.set(colors.leaf);
    for (let i = 0; i < leafBaseColors.length; i++) {
      leafBaseColors[i] = toneColor(leafEntries[i].tone, colors);
    }
    applyLeafColors();
    if (blossomMesh) {
      const petal = new Color(colors.rose);
      const heart = new Color(colors.amber);
      for (let i = 0; i < blossomCount; i++) {
        blossomMesh.setColorAt(i, i % 6 === 5 ? heart : petal);
      }
      if (blossomMesh.instanceColor) blossomMesh.instanceColor.needsUpdate = true;
    }
    if (visible) renderOnce();
  }

  function resize(nextSize: number) {
    if (disposed) return;
    size = clamp(finite(nextSize, size), 1, 8192);
    renderer.setPixelRatio(Math.min(typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1, DPR_MAX));
    renderer.setSize(size * ASPECT, size, false);
    if (visible) renderOnce();
  }

  function setAnimate(next: boolean) {
    if (disposed) return;
    animate = !!next;
    if (!animate) {
      snapTweens();
      cancelRaf();
      if (visible) renderOnce();
    }
  }

  function setVisible(next: boolean) {
    if (disposed) return;
    visible = !!next;
    if (!visible) {
      snapTweens();
      cancelRaf();
    } else if (!contextLost) {
      renderOnce();
    }
  }

  function pick(clientX: number, clientY: number): number {
    if (disposed || contextLost || !visible) return -1;
    const rect = canvas.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return -1;
    pointer.set(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
    camera.updateMatrixWorld();
    scene.updateMatrixWorld(true);
    raycaster.setFromCamera(pointer, camera);

    // Shapes under the pointer win; limb proxies are only the fallback for
    // clicks that miss every leaf and branch (matching the SVG).
    const shapes: { distance: number; page: number }[] = [];
    const addShape = (distance: number, page: unknown) => {
      if (typeof page === "number" && page >= 0) shapes.push({ distance, page });
    };
    if (leafMesh && leafMesh.count > 0) {
      leafMesh.boundingSphere = null;
      for (const hit of raycaster.intersectObject(leafMesh, false)) {
        const id = hit.instanceId;
        if (id !== undefined) addShape(hit.distance, leafPages[id]);
      }
    }
    for (const hit of raycaster.intersectObjects(branchMeshes, false)) {
      if (hit.object.parent && !hit.object.parent.visible) continue;
      if (hit.object.userData.depth === 0) continue;
      addShape(hit.distance, hit.object.userData.page);
    }
    shapes.sort((a, b) => a.distance - b.distance);
    if (shapes.length > 0) return shapes[0].page;

    const proxies: { distance: number; page: number }[] = [];
    for (const hit of raycaster.intersectObjects(hitProxies, false)) {
      if (hit.object.parent && !hit.object.parent.visible) continue;
      const page = hit.object.userData.page;
      if (typeof page === "number" && page >= 0) proxies.push({ distance: hit.distance, page });
    }
    proxies.sort((a, b) => a.distance - b.distance);
    return proxies[0]?.page ?? -1;
  }

  function dispose() {
    if (disposed) return;
    disposed = true;
    cancelRaf();
    canvas.removeEventListener("webglcontextlost", onContextLost);
    clearTree();
    leafGeometry.dispose();
    blossomGeometry.dispose();
    seedGeometry.dispose();
    moundGeometry.dispose();
    shadowGeometry.dispose();
    sunTexture.dispose();
    shadowTexture.dispose();
    barkMat.dispose();
    barkDimMat.dispose();
    hitMat.dispose();
    leafMat.dispose();
    blossomMat.dispose();
    moundMat.dispose();
    seedMat.dispose();
    sproutMats[0].dispose();
    sproutMats[1].dispose();
    sunMat.dispose();
    shadowMat.dispose();
    renderer.dispose();
    renderer.forceContextLoss();
  }

  function onContextLost() {
    contextLost = true;
    cancelRaf();
  }

  canvas.addEventListener("webglcontextlost", onContextLost);

  try {
    buildTree();
    applyHighlight();
    applyGrowth(false, false);
    if (entrance && animate && typeof requestAnimationFrame === "function") {
      entranceStart = Math.max(now(), 0.001);
      renderOnce();
      startRaf();
    } else {
      treeGroup.scale.setScalar(1);
      renderOnce();
    }
  } catch (error) {
    dispose();
    throw error;
  }

  return {
    setProgress,
    setHighlight,
    setColors,
    resize,
    setAnimate,
    setVisible,
    pick,
    dispose,
  };
}
