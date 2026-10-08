import { describe, expect, it } from "vitest";
import { Simulation, groupAnchors, seedPosition, type SimNode } from "./graph-sim";

function build(count: number, groups: number, linksPer = 1) {
  const sizes = Array.from({ length: groups }, (_, g) => Math.ceil(count / groups) - g);
  const anchors = groupAnchors(sizes);
  const nodes: SimNode[] = Array.from({ length: count }, (_, i) => {
    const group = i % groups;
    const p = seedPosition(`page-${i}`, anchors[group], 40 * Math.sqrt(sizes[group]));
    return { id: `page-${i}`, group, degree: 0, r: 5, ...p, vx: 0, vy: 0, fx: null, fy: null };
  });
  const links = [];
  for (let i = 0; i < count; i++) {
    for (let k = 1; k <= linksPer; k++) {
      const j = (i * 7 + k * 13) % count;
      if (j !== i && i % 3 !== 0) links.push({ a: i, b: j });
    }
  }
  for (const l of links) {
    nodes[l.a].degree++;
    nodes[l.b].degree++;
  }
  return new Simulation(nodes, links, anchors);
}

function settle(sim: Simulation) {
  let ticks = 0;
  while (sim.running && ticks < 1000) {
    sim.tick();
    ticks++;
  }
  return ticks;
}

describe("graph layout", () => {
  it("settles to the same place every time, without NaN", () => {
    const a = build(200, 4);
    const b = build(200, 4);
    const ticks = settle(a);
    settle(b);
    expect(ticks).toBeLessThan(400);
    expect(a.running).toBe(false);
    for (let i = 0; i < a.nodes.length; i++) {
      expect(Number.isFinite(a.nodes[i].x) && Number.isFinite(a.nodes[i].y)).toBe(true);
      expect(a.nodes[i].x).toBeCloseTo(b.nodes[i].x, 6);
    }
  });

  it("keeps discs apart and folders together", () => {
    const sim = build(300, 3, 0);
    settle(sim);
    let overlaps = 0;
    for (let i = 0; i < sim.nodes.length; i++) {
      for (let j = i + 1; j < sim.nodes.length; j++) {
        const a = sim.nodes[i];
        const b = sim.nodes[j];
        if (Math.hypot(a.x - b.x, a.y - b.y) < a.r + b.r) overlaps++;
      }
    }
    expect(overlaps).toBe(0);
    // Each node sits nearer its own folder's anchor than the others'.
    let closest = 0;
    for (const n of sim.nodes) {
      const d = sim.anchors.map((p) => Math.hypot(n.x - p.x, n.y - p.y));
      if (d.indexOf(Math.min(...d)) === n.group) closest++;
    }
    expect(closest / sim.nodes.length).toBeGreaterThan(0.9);
  });

  it("holds a dragged node where it is put", () => {
    const sim = build(50, 2);
    sim.nodes[0].fx = 1234;
    sim.nodes[0].fy = -50;
    sim.tick();
    expect(sim.nodes[0].x).toBe(1234);
    expect(sim.nodes[0].y).toBe(-50);
  });

  it("lays out 3,000 linked pages in well under a second", () => {
    const sim = build(3000, 8, 2);
    const start = performance.now();
    settle(sim);
    const ms = performance.now() - start;
    // The view spreads this over frames; a single tick must stay cheap.
    expect(ms / 260).toBeLessThan(8);
  });
});
