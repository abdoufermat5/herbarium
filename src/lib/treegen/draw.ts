// Path builders shared by the reading-path tree renderer. Pure strings, no DOM.

const n = (v: number) => Math.round(v * 10) / 10;

/** Quadratic midpoint smoothing through a polyline. */
export function smoothPath(points: { x: number; y: number }[]): string {
  if (points.length < 2) return "";
  let d = `M${n(points[0].x)} ${n(points[0].y)}`;
  for (let i = 1; i < points.length - 1; i++) {
    const p = points[i];
    const q = points[i + 1];
    d += `Q${n(p.x)} ${n(p.y)} ${n((p.x + q.x) / 2)} ${n((p.y + q.y) / 2)}`;
  }
  const last = points[points.length - 1];
  return `${d}L${n(last.x)} ${n(last.y)}`;
}

/** One teardrop leaf from (x, y), pointing at `angle`, length L, half-width w. */
export function leafPath(x: number, y: number, angle: number, length: number, halfWidth: number): string {
  const dx = Math.cos(angle);
  const dy = Math.sin(angle);
  const px = -dy;
  const py = dx;
  const lx = dx * length;
  const ly = dy * length;
  const wx = px * halfWidth;
  const wy = py * halfWidth;
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

export function circlePath(cx: number, cy: number, r: number): string {
  return `M${n(cx - r)} ${n(cy)}a${n(r)} ${n(r)} 0 1 0 ${n(2 * r)} 0a${n(r)} ${n(r)} 0 1 0 ${n(-2 * r)} 0Z`;
}

/** Three leaves fanning around `angle`, sized as a cluster. */
export function clusterPaths(x: number, y: number, angle: number, size: number): [string, string, string] {
  const left = leafPath(x, y, angle - 0.7, 0.85 * size, 0.38 * 0.85 * size);
  const centre = leafPath(x, y, angle, size, 0.38 * size);
  const right = leafPath(x, y, angle + 0.7, 0.9 * size, 0.38 * 0.9 * size);
  return [left, centre, right];
}
