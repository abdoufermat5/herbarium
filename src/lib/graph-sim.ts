// The force layout behind the graph view. Plain arrays and numbers, no DOM:
// the view runs a few ticks per animation frame, so the layout settles while
// the window stays responsive, and the tests run it headless.
//
// Forces, scaled by `alpha` as it cools: nearby nodes push apart (found
// through a grid, so a tick costs about O(n) instead of O(n²)), linked nodes
// pull to a resting distance, and each node is drawn toward its folder's
// anchor, so folders settle as islands. Starting positions come from the page
// id, so the same vault lays out the same way every time.

export interface SimNode {
  id: string;
  /** Index of the node's group (folder) in the anchor list. */
  group: number;
  /** Links in and out, for sizing and spring strength. */
  degree: number;
  /** Radius in layout units. */
  r: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  /** Pinned position while the user drags the node. */
  fx: number | null;
  fy: number | null;
}

export interface SimLink {
  a: number;
  b: number;
}

/** Nodes further apart than this do not push each other. */
const CUTOFF = 160;
const REPULSION = 900;
const LINK_DISTANCE = 64;
const ANCHOR_PULL = 0.12;
/** How hard a link between two folders pulls, next to one inside a folder. */
const CROSS_GROUP = 0.08;
const VELOCITY_DECAY = 0.42;
const ALPHA_MIN = 0.004;
/** Ticks for alpha to cool from 1 to ALPHA_MIN. */
const COOLING_TICKS = 260;

/** A number in [0, 1) from a string, stable across runs. */
export function hash01(s: string, salt = 0): number {
  let h = 2166136261 ^ salt;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  h ^= h >>> 13;
  h = Math.imul(h, 0x5bd1e995);
  h ^= h >>> 15;
  return (h >>> 0) / 4294967296;
}

/** Where each group settles: the biggest in the middle, the others on a sunflower spiral around it. */
export function groupAnchors(sizes: number[]): Array<{ x: number; y: number }> {
  const total = sizes.reduce((a, b) => a + b, 0) || 1;
  const spread = 46 * Math.sqrt(total);
  const golden = Math.PI * (3 - Math.sqrt(5));
  return sizes.map((_, i) => {
    if (i === 0) return { x: 0, y: 0 };
    const radius = spread * Math.sqrt(i / Math.max(1, sizes.length - 1)) + 40 * Math.sqrt(sizes[0]);
    return { x: Math.cos(i * golden) * radius, y: Math.sin(i * golden) * radius };
  });
}

export class Simulation {
  alpha = 1;
  private readonly decay = 1 - Math.pow(ALPHA_MIN, 1 / COOLING_TICKS);
  private readonly linkStrength: number[];
  private readonly grid = new Map<number, number[]>();

  constructor(
    readonly nodes: SimNode[],
    readonly links: SimLink[],
    readonly anchors: Array<{ x: number; y: number }>,
  ) {
    const count = new Array<number>(nodes.length).fill(0);
    for (const l of links) {
      count[l.a]++;
      count[l.b]++;
    }
    // Hubs give a little so their many springs do not crush them, and links
    // between folders pull gently, so each folder stays an island.
    this.linkStrength = links.map(
      (l) =>
        (nodes[l.a].group === nodes[l.b].group ? 1 : CROSS_GROUP) /
        Math.max(1, Math.min(count[l.a], count[l.b])),
    );
  }

  get running(): boolean {
    return this.alpha > ALPHA_MIN;
  }

  /** Warm the layout up again (after a drag or a change), up to `alpha`. */
  reheat(alpha = 0.3) {
    this.alpha = Math.max(this.alpha, alpha);
  }

  tick() {
    const { nodes, alpha } = this;
    this.repel(alpha);
    this.pullLinks(alpha);
    for (const n of nodes) {
      const anchor = this.anchors[n.group] ?? { x: 0, y: 0 };
      // Unlinked pages lean harder on their folder, so they gather round it.
      const pull = n.degree === 0 ? ANCHOR_PULL * 1.6 : ANCHOR_PULL;
      n.vx += (anchor.x - n.x) * pull * alpha;
      n.vy += (anchor.y - n.y) * pull * alpha;
    }
    for (const n of nodes) {
      if (n.fx !== null && n.fy !== null) {
        n.x = n.fx;
        n.y = n.fy;
        n.vx = 0;
        n.vy = 0;
        continue;
      }
      n.vx *= 1 - VELOCITY_DECAY;
      n.vy *= 1 - VELOCITY_DECAY;
      n.x += n.vx;
      n.y += n.vy;
    }
    this.alpha += (0 - this.alpha) * this.decay;
  }

  private repel(alpha: number) {
    const { nodes, grid } = this;
    grid.clear();
    const key = (cx: number, cy: number) => (cx + 32768) * 65536 + (cy + 32768);
    for (let i = 0; i < nodes.length; i++) {
      const k = key(Math.floor(nodes[i].x / CUTOFF), Math.floor(nodes[i].y / CUTOFF));
      const cell = grid.get(k);
      if (cell) cell.push(i);
      else grid.set(k, [i]);
    }
    const cut2 = CUTOFF * CUTOFF;
    for (const [k, cell] of grid) {
      const cx = Math.floor(k / 65536) - 32768;
      const cy = (k % 65536) - 32768;
      // The cell itself and half its neighbours: each pair of cells meets once.
      for (const [ox, oy] of NEIGHBOURS) {
        const other = ox === 0 && oy === 0 ? cell : grid.get(key(cx + ox, cy + oy));
        if (!other) continue;
        for (let i = 0; i < cell.length; i++) {
          const a = nodes[cell[i]];
          for (let j = other === cell ? i + 1 : 0; j < other.length; j++) {
            const b = nodes[other[j]];
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let d2 = dx * dx + dy * dy;
            if (d2 > cut2) continue;
            if (d2 < 1e-6) {
              // Exactly on top of each other: part them along a stable direction.
              const angle = hash01(a.id + b.id) * Math.PI * 2;
              dx = Math.cos(angle) * 0.1;
              dy = Math.sin(angle) * 0.1;
              d2 = 0.01;
            }
            const minGap = a.r + b.r + 6;
            const w = (REPULSION * alpha) / Math.max(d2, 25);
            a.vx -= dx * w * 0.02;
            a.vy -= dy * w * 0.02;
            b.vx += dx * w * 0.02;
            b.vy += dy * w * 0.02;
            // Never let two discs overlap, whatever the temperature.
            if (d2 < minGap * minGap) {
              const d = Math.sqrt(d2);
              const push = ((minGap - d) / d) * 0.5;
              a.x -= dx * push * 0.5;
              a.y -= dy * push * 0.5;
              b.x += dx * push * 0.5;
              b.y += dy * push * 0.5;
            }
          }
        }
      }
    }
  }

  private pullLinks(alpha: number) {
    const { nodes, links, linkStrength } = this;
    for (let i = 0; i < links.length; i++) {
      const a = nodes[links[i].a];
      const b = nodes[links[i].b];
      const dx = b.x + b.vx - a.x - a.vx;
      const dy = b.y + b.vy - a.y - a.vy;
      const d = Math.sqrt(dx * dx + dy * dy) || 1;
      const rest = LINK_DISTANCE + a.r + b.r;
      const f = ((d - rest) / d) * alpha * linkStrength[i] * 0.5;
      a.vx += dx * f;
      a.vy += dy * f;
      b.vx -= dx * f;
      b.vy -= dy * f;
    }
  }
}

const NEIGHBOURS: Array<[number, number]> = [
  [0, 0],
  [1, 0],
  [-1, 1],
  [0, 1],
  [1, 1],
];

/** A node's starting place: near its group's anchor, at a spot set by its id. */
export function seedPosition(id: string, anchor: { x: number; y: number }, spread: number) {
  const angle = hash01(id, 1) * Math.PI * 2;
  const dist = Math.sqrt(hash01(id, 2)) * spread;
  return { x: anchor.x + Math.cos(angle) * dist, y: anchor.y + Math.sin(angle) * dist };
}
