<script module lang="ts">
  /** Where each page settled, kept for the session: coming back to the graph
   *  shows the same picture at once instead of laying it out again. */
  const remembered = new Map<string, { x: number; y: number }>();
</script>

<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { api } from "../lib/api";
  import { app, errorMessage, openPage } from "../lib/state.svelte";
  import { t } from "../lib/i18n.svelte";
  import { folderLook } from "../lib/appearance";
  import { Simulation, groupAnchors, seedPosition, type SimNode } from "../lib/graph-sim";
  import type { PageGraph } from "../lib/types";
  import Icon from "../lib/Icon.svelte";

  // Identity colour is the top-level folder. A folder the user coloured keeps
  // its colour; the others take the first three categorical slots (the ones
  // that stay apart for every pair, since any node can sit next to any other),
  // and the rest share a neutral "Other".
  const SLOTS = 3;
  const MAX_LABELS = 160;
  const MIN_ZOOM = 0.04;
  const MAX_ZOOM = 6;
  /** Above this many links, a zoomed-out map bundles those between folders. */
  const BUNDLE_ABOVE = 1200;
  /** How far links bow, as a share of their length. */
  const BEND = 0.08;

  interface Group {
    key: string;
    label: string;
    count: number;
    /** CSS colour expression (a custom property). */
    css: string;
    other: boolean;
  }

  interface Item {
    id: string;
    title: string;
    group: number;
    out: number[];
    in: number[];
    /** Neighbours either way, without repeats. */
    near: number[];
  }

  let graph = $state<PageGraph | null>(null);
  let error = $state<string | null>(null);
  let showAll = $state(readShowAll());
  let groups = $state<Group[]>([]);
  let linkCount = $state(0);
  let unlinked = $state(0);
  let selected = $state<number | null>(null);
  let hovered = $state<number | null>(null);
  let tip = $state({ x: 0, y: 0 });
  let isolated = $state<number | null>(null);
  let query = $state("");
  let activeMatch = $state(0);
  let arranging = $state(false);
  let dragging = $state(false);

  let stage: HTMLDivElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();

  // Drawing state lives outside Svelte's reactivity: it changes every frame.
  // Raw state: replaced whole on each load, never changed in place.
  let items = $state.raw<Item[]>([]);
  let sim: Simulation | null = null;
  let edgeA = new Int32Array(0);
  let edgeB = new Int32Array(0);
  /** 1: a→b, 2: b→a, 3: both ways. */
  let edgeDir = new Uint8Array(0);
  /** Node indices by importance, for labels. */
  let byDegree: number[] = [];
  let labelWidth = new Float32Array(0);
  let cam = { k: 1, tx: 0, ty: 0 };
  let size = { w: 0, h: 0, dpr: 1 };
  let raf = 0;
  let dirty = true;
  let lastFrame = 0;
  let flight: { from: typeof cam; to: typeof cam; start: number; ms: number } | null = null;
  let userMoved = false;
  let colors = readColorsFallback();
  const reducedMotion =
    typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  const matches = $derived.by(() => {
    const q = fold(query.trim());
    if (!q || !graph) return [] as number[];
    const out: number[] = [];
    for (let i = 0; i < items.length && out.length < 200; i++) {
      if (fold(items[i].title).includes(q)) out.push(i);
    }
    // Best-connected first: the page you most likely mean.
    return out.sort((a, b) => items[b].near.length - items[a].near.length);
  });

  const summary = $derived(
    graph
      ? [
          t("unit.page", { count: graph.nodes.length }),
          t("graph.degree", { count: linkCount }),
          ...(showAll && unlinked > 0 ? [t("graph.unlinked", { count: unlinked })] : []),
        ].join(" · ")
      : "",
  );

  const card = $derived.by(() => {
    if (selected === null || !items[selected]) return null;
    const it = items[selected];
    const brief = (i: number) => ({ index: i, title: items[i].title, color: groups[items[i].group]?.css });
    return {
      ...it,
      group: groups[it.group],
      out: it.out.map(brief).sort((a, b) => a.title.localeCompare(b.title)),
      in: it.in.map(brief).sort((a, b) => a.title.localeCompare(b.title)),
    };
  });

  function readShowAll(): boolean {
    try {
      return localStorage.getItem("herbarium.graph.all") === "1";
    } catch {
      return false;
    }
  }

  /** Lowercase without accents, for search. */
  function fold(s: string): string {
    return s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
  }

  function topFolder(folder: string | null): string {
    return folder ? folder.split("/")[0] : "";
  }

  function radius(degree: number): number {
    return 4 + Math.min(12, Math.sqrt(degree) * 2.2);
  }

  /* ------------------------------------------------------------- colours */

  function readColorsFallback() {
    return {
      surface: "#fff",
      text: "#2f3437",
      soft: "#50555a",
      muted: "#6b6a68",
      edge: "rgba(107,106,104,0.3)",
      font: "sans-serif",
      groups: [] as string[],
    };
  }

  /** Resolve the theme's tokens once per theme, not per frame. */
  function readColors() {
    if (!stage) return;
    const cs = getComputedStyle(stage);
    const v = (name: string) => cs.getPropertyValue(name).trim();
    colors = {
      surface: v("--surface") || "#fff",
      text: v("--text") || "#2f3437",
      soft: v("--text-soft") || "#50555a",
      muted: v("--muted") || "#6b6a68",
      edge: v("--graph-edge") || "rgba(107,106,104,0.3)",
      font: v("--font") || "sans-serif",
      groups: groups.map((g) => {
        const m = /^var\((--[\w-]+)\)$/.exec(g.css);
        return (m ? v(m[1]) : g.css) || v("--series-other");
      }),
    };
    dirty = true;
    schedule();
  }

  /* -------------------------------------------------------------- data */

  async function load() {
    try {
      const data = await api.pageGraph(showAll);
      error = null;
      build(data);
      graph = data;
      await tick();
      readColors();
      resize();
      if (!userMoved) fit(false);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  function build(data: PageGraph) {
    // Groups, biggest first.
    const counts = new Map<string, number>();
    for (const n of data.nodes) counts.set(topFolder(n.folder), (counts.get(topFolder(n.folder)) ?? 0) + 1);
    const ordered = [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    let slot = 0;
    const list: Group[] = ordered.map(([key, count]) => {
      const own = key ? folderLook(key).color : undefined;
      if (own) return { key, label: key || t("graph.root"), count, css: `var(--c-${own})`, other: false };
      if (slot < SLOTS) {
        slot++;
        return { key, label: key || t("graph.root"), count, css: `var(--series-${slot})`, other: false };
      }
      return { key, label: key || t("graph.root"), count, css: "var(--series-other)", other: true };
    });
    const groupIndex = new Map(list.map((g, i) => [g.key, i]));

    // Items and their links; two pages linking both ways are one line.
    const index = new Map(data.nodes.map((n, i) => [n.id, i]));
    const next: Item[] = data.nodes.map((n) => ({
      id: n.id,
      title: n.title || n.id,
      group: groupIndex.get(topFolder(n.folder)) ?? 0,
      out: [],
      in: [],
      near: [],
    }));
    const pairs = new Map<string, number>();
    const ea: number[] = [];
    const eb: number[] = [];
    const ed: number[] = [];
    for (const [from, to] of data.edges) {
      const a = index.get(from);
      const b = index.get(to);
      if (a === undefined || b === undefined || a === b) continue;
      next[a].out.push(b);
      next[b].in.push(a);
      const key = a < b ? `${a}:${b}` : `${b}:${a}`;
      const at = pairs.get(key);
      const dir = a < b ? 1 : 2;
      if (at === undefined) {
        pairs.set(key, ea.length);
        ea.push(Math.min(a, b));
        eb.push(Math.max(a, b));
        ed.push(dir);
      } else {
        ed[at] |= dir;
      }
    }
    for (const it of next) it.near = [...new Set([...it.out, ...it.in])];

    // Layout: remembered places first, new pages near their folder.
    const anchors = groupAnchors(list.map((g) => g.count));
    let fresh = 0;
    const nodes: SimNode[] = next.map((it) => {
      const known = remembered.get(it.id);
      const p = known ?? seedPosition(it.id, anchors[it.group], 30 * Math.sqrt(list[it.group].count));
      if (!known) fresh++;
      return {
        id: it.id,
        group: it.group,
        degree: it.near.length,
        r: radius(it.near.length),
        x: p.x,
        y: p.y,
        vx: 0,
        vy: 0,
        fx: null,
        fy: null,
      };
    });
    sim = new Simulation(
      nodes,
      ea.map((a, i) => ({ a, b: eb[i] })),
      anchors,
    );
    sim.alpha = fresh === 0 ? 0 : fresh === nodes.length ? 1 : 0.5;

    items = next;
    edgeA = Int32Array.from(ea);
    edgeB = Int32Array.from(eb);
    edgeDir = Uint8Array.from(ed);
    byDegree = next.map((_, i) => i).sort((a, b) => next[b].near.length - next[a].near.length);
    labelWidth = new Float32Array(next.length).fill(-1);
    groups = list;
    linkCount = ea.length;
    unlinked = next.filter((it) => it.near.length === 0).length;
    // Keep the selection when the page is still there.
    const keep = selected !== null ? graph?.nodes[selected]?.id : undefined;
    selected = keep !== undefined && index.has(keep) ? index.get(keep)! : null;
    hovered = null;
    isolated = isolated !== null && isolated < list.length ? isolated : null;
    arranging = sim.running && reducedMotion;
    dirty = true;
    schedule();
  }

  /* ------------------------------------------------------------ the loop */

  function schedule() {
    if (!raf) raf = requestAnimationFrame(frame);
  }

  function frame(now: number) {
    raf = 0;
    if (sim?.running) {
      // Spend ~7 ms of each frame on the layout; when painting is what makes
      // frames slow, more layout per frame settles it in fewer of them.
      const budget = now - lastFrame > 40 ? 24 : 7;
      lastFrame = now;
      const start = performance.now();
      do sim.tick();
      while (sim.running && performance.now() - start < budget);
      dirty = true;
      // Until the user takes over, the camera eases along as the picture grows.
      const to = !userMoved && !flight ? fitTarget() : null;
      if (to) {
        const ease = 0.12;
        cam = {
          k: Math.exp(Math.log(cam.k) + (Math.log(to.k) - Math.log(cam.k)) * ease),
          tx: cam.tx + (to.tx - cam.tx) * ease,
          ty: cam.ty + (to.ty - cam.ty) * ease,
        };
      }
      if (!sim.running) {
        for (const n of sim.nodes) remembered.set(n.id, { x: n.x, y: n.y });
        if (arranging) arranging = false;
        if (!userMoved) fit(true);
      }
    }
    if (flight) {
      const p = Math.min(1, (now - flight.start) / flight.ms);
      const e = 1 - Math.pow(1 - p, 3);
      // Zoom moves in log space, so it feels even at every scale.
      const k = Math.exp(Math.log(flight.from.k) + (Math.log(flight.to.k) - Math.log(flight.from.k)) * e);
      cam = {
        k,
        tx: flight.from.tx + (flight.to.tx - flight.from.tx) * e,
        ty: flight.from.ty + (flight.to.ty - flight.from.ty) * e,
      };
      if (p >= 1) flight = null;
      dirty = true;
    }
    if (dirty && !(arranging && sim?.running)) draw();
    if (sim?.running || flight) schedule();
  }

  function invalidate() {
    dirty = true;
    schedule();
  }

  /* ------------------------------------------------------------- drawing */

  /** Which nodes stand out: a page and its neighbours, search hits or a folder. */
  function emphasis(): Uint8Array | null {
    const focus = hovered ?? selected;
    if (focus !== null && items[focus]) {
      const lit = new Uint8Array(items.length);
      lit[focus] = 2;
      for (const j of items[focus].near) lit[j] = 1;
      return lit;
    }
    if (matches.length > 0) {
      const lit = new Uint8Array(items.length);
      for (const i of matches) lit[i] = 1;
      return lit;
    }
    if (isolated !== null) {
      const lit = new Uint8Array(items.length);
      items.forEach((it, i) => {
        if (it.group === isolated) lit[i] = 1;
      });
      return lit;
    }
    return null;
  }

  function screenRadius(r: number): number {
    const { k } = cam;
    return k < 1 ? Math.max(2, r * Math.sqrt(k)) : r * (1 + (k - 1) * 0.3);
  }

  function draw() {
    dirty = false;
    const ctx = canvas?.getContext("2d");
    if (!ctx || !sim) return;
    const { w, h, dpr } = size;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const nodes = sim.nodes;
    const { k, tx, ty } = cam;
    const sx = (x: number) => x * k + tx;
    const sy = (y: number) => y * k + ty;
    const lit = emphasis();
    const focus = hovered ?? selected;
    const margin = 40;
    const visible = (i: number) => {
      const x = sx(nodes[i].x);
      const y = sy(nodes[i].y);
      return x > -margin && y > -margin && x < w + margin && y < h + margin;
    };

    // Where each folder's island is, for bundles and region names.
    const islands = groups.map(() => ({ x: 0, y: 0, n: 0, spread: 0 }));
    for (const n of nodes) {
      const g = islands[n.group];
      g.x += n.x;
      g.y += n.y;
      g.n++;
    }
    for (const g of islands) {
      if (!g.n) continue;
      g.x /= g.n;
      g.y /= g.n;
    }
    for (const n of nodes) {
      const g = islands[n.group];
      g.spread += (n.x - g.x) ** 2 + (n.y - g.y) ** 2;
    }
    for (const g of islands) g.spread = g.n ? Math.sqrt(g.spread / g.n) : 0;

    // Zoomed out on a busy map, links between folders become one ribbon per
    // pair of folders: thousands of crossing lines read as noise (and cost a
    // lot to paint), the ribbons say which folders talk to each other.
    const overview = edgeA.length > BUNDLE_ABOVE && cam.k < 0.6 && groups.length > 1;
    ctx.lineCap = "round";
    const quiet = new Path2D();
    const loud = new Path2D();
    const inside = groups.map(() => new Path2D());
    const bundles = new Map<number, number>();
    for (let e = 0; e < edgeA.length; e++) {
      const ia = edgeA[e];
      const ib = edgeB[e];
      const strong = lit && focus !== null ? ia === focus || ib === focus : lit ? lit[ia] && lit[ib] : false;
      const ga = items[ia].group;
      const gb = items[ib].group;
      if (overview && !strong && ga !== gb) {
        const key = Math.min(ga, gb) * 4096 + Math.max(ga, gb);
        bundles.set(key, (bundles.get(key) ?? 0) + 1);
        continue;
      }
      const a = nodes[ia];
      const b = nodes[ib];
      const x1 = sx(a.x);
      const y1 = sy(a.y);
      const x2 = sx(b.x);
      const y2 = sy(b.y);
      if ((x1 < 0 && x2 < 0) || (y1 < 0 && y2 < 0) || (x1 > w && x2 > w) || (y1 > h && y2 > h)) continue;
      const path = strong ? loud : overview ? inside[ga] : quiet;
      // A slight bend: straight lines read as a mesh, curves as connections.
      path.moveTo(x1, y1);
      path.quadraticCurveTo((x1 + x2) / 2 - (y2 - y1) * BEND, (y1 + y2) / 2 + (x2 - x1) * BEND, x2, y2);
    }
    const fade = lit ? 0.25 : 1;
    if (bundles.size) {
      ctx.strokeStyle = colors.muted;
      for (const [key, count] of bundles) {
        const a = islands[Math.floor(key / 4096)];
        const b = islands[key % 4096];
        const x1 = sx(a.x);
        const y1 = sy(a.y);
        const x2 = sx(b.x);
        const y2 = sy(b.y);
        ctx.globalAlpha = fade * Math.min(0.4, 0.1 + count / 400);
        ctx.lineWidth = Math.min(14, 1 + Math.sqrt(count) * 0.7) * Math.max(0.6, Math.sqrt(cam.k * 2));
        ctx.beginPath();
        ctx.moveTo(x1, y1);
        ctx.quadraticCurveTo((x1 + x2) / 2 - (y2 - y1) * BEND * 2, (y1 + y2) / 2 + (x2 - x1) * BEND * 2, x2, y2);
        ctx.stroke();
      }
    }
    ctx.lineWidth = 1;
    if (overview) {
      inside.forEach((p, g) => {
        ctx.globalAlpha = fade * 0.45;
        ctx.strokeStyle = colors.groups[g] ?? colors.muted;
        ctx.stroke(p);
      });
    } else {
      ctx.strokeStyle = colors.edge;
      ctx.globalAlpha = fade;
      ctx.stroke(quiet);
    }
    ctx.globalAlpha = 1;
    if (lit) {
      ctx.strokeStyle = colors.soft;
      ctx.lineWidth = 1.5;
      ctx.stroke(loud);
      if (focus !== null) drawArrows(ctx, focus, sx, sy);
    }

    // Pages, a path per colour; the faded ones first.
    for (const pass of lit ? [0, 1] : [1]) {
      ctx.globalAlpha = lit && pass === 0 ? 0.18 : 1;
      const paths = groups.map(() => new Path2D());
      for (let i = 0; i < nodes.length; i++) {
        if (lit && (lit[i] ? 1 : 0) !== pass) continue;
        if (!visible(i)) continue;
        const r = screenRadius(nodes[i].r);
        const x = sx(nodes[i].x);
        const y = sy(nodes[i].y);
        paths[items[i].group].moveTo(x + r, y);
        paths[items[i].group].arc(x, y, r, 0, Math.PI * 2);
      }
      // A thin gap around each disc; tiny discs get none, it would hide them.
      ctx.lineWidth = Math.min(1.5, Math.max(0, screenRadius(5) - 2.5) * 0.6);
      ctx.strokeStyle = colors.surface;
      paths.forEach((p, g) => {
        ctx.fillStyle = colors.groups[g] ?? colors.muted;
        ctx.fill(p);
        if (ctx.lineWidth > 0.2) ctx.stroke(p);
      });
    }
    ctx.globalAlpha = 1;

    // Rings: the page in focus, the selection, search hits.
    const ring = (i: number, gap: number, width: number, color: string) => {
      const r = screenRadius(nodes[i].r) + gap;
      ctx.beginPath();
      ctx.arc(sx(nodes[i].x), sy(nodes[i].y), r, 0, Math.PI * 2);
      ctx.lineWidth = width;
      ctx.strokeStyle = color;
      ctx.stroke();
    };
    if (focus === null) for (const i of matches.slice(0, 50)) ring(i, 3, 1.5, colors.text);
    if (selected !== null && items[selected]) ring(selected, 4, 2, colors.text);
    if (hovered !== null && hovered !== selected && items[hovered]) ring(hovered, 3, 1.5, colors.soft);

    drawLabels(ctx, lit, sx, sy, visible, islands);
  }

  /** Small heads on the focused page's links, showing which way each one goes. */
  function drawArrows(ctx: CanvasRenderingContext2D, focus: number, sx: (x: number) => number, sy: (y: number) => number) {
    const nodes = sim!.nodes;
    ctx.fillStyle = colors.soft;
    for (let e = 0; e < edgeA.length; e++) {
      if (edgeA[e] !== focus && edgeB[e] !== focus) continue;
      for (const [, to, bit] of [
        [edgeA[e], edgeB[e], 1],
        [edgeB[e], edgeA[e], 2],
      ] as const) {
        if (!(edgeDir[e] & bit)) continue;
        const a = nodes[edgeA[e]];
        const b = nodes[edgeB[e]];
        const x1 = sx(a.x);
        const y1 = sy(a.y);
        const x2 = sx(b.x);
        const y2 = sy(b.y);
        if (Math.hypot(x2 - x1, y2 - y1) < 24) continue;
        // The curve ends along the line from its control point to the end.
        const cx = (x1 + x2) / 2 - (y2 - y1) * BEND;
        const cy = (y1 + y2) / 2 + (x2 - x1) * BEND;
        const end = nodes[to];
        const ex = sx(end.x);
        const ey = sy(end.y);
        const dx = ex - cx;
        const dy = ey - cy;
        const d = Math.hypot(dx, dy) || 1;
        const ux = dx / d;
        const uy = dy / d;
        const tipX = ex - ux * (screenRadius(end.r) + 3);
        const tipY = ey - uy * (screenRadius(end.r) + 3);
        ctx.beginPath();
        ctx.moveTo(tipX, tipY);
        ctx.lineTo(tipX - ux * 7 - uy * 3.5, tipY - uy * 7 + ux * 3.5);
        ctx.lineTo(tipX - ux * 7 + uy * 3.5, tipY - uy * 7 - ux * 3.5);
        ctx.closePath();
        ctx.fill();
      }
    }
  }

  /** Each folder's name over its island, faint, while the map is zoomed out. */
  function drawRegions(
    ctx: CanvasRenderingContext2D,
    sx: (x: number) => number,
    sy: (y: number) => number,
    placed: Array<[number, number, number, number]>,
    font: string,
    islands: Array<{ x: number; y: number; n: number; spread: number }>,
  ) {
    if (groups.length < 2 || cam.k > 1.6) return;
    ctx.font = `600 11px ${font}`;
    ctx.globalAlpha = Math.min(1, (1.6 - cam.k) / 0.5) * 0.9;
    groups.forEach((group, i) => {
      const g = islands[i];
      if (!g || g.n < 3) return;
      const text = group.label.toUpperCase();
      const x = sx(g.x);
      // Above the bulk of the island, not above its furthest stray page.
      const y = sy(g.y - g.spread * 1.25) - 18;
      const width = ctx.measureText(text).width + text.length * 1.2;
      if (x < 0 || x > size.w || y < 0 || y > size.h) return;
      const box: [number, number, number, number] = [x - width / 2 - 4, y - 2, x + width / 2 + 4, y + 16];
      if (placed.some((p) => box[0] < p[2] && box[2] > p[0] && box[1] < p[3] && box[3] > p[1])) return;
      placed.push(box);
      ctx.letterSpacing = "1.2px";
      ctx.lineWidth = 4;
      ctx.strokeStyle = colors.surface;
      ctx.strokeText(text, x, y);
      ctx.fillStyle = colors.muted;
      ctx.fillText(text, x, y);
      ctx.letterSpacing = "0px";
    });
    ctx.globalAlpha = 1;
  }

  /** Titles, most important first, never on top of each other. */
  function drawLabels(
    ctx: CanvasRenderingContext2D,
    lit: Uint8Array | null,
    sx: (x: number) => number,
    sy: (y: number) => number,
    visible: (i: number) => boolean,
    islands: Array<{ x: number; y: number; n: number; spread: number }>,
  ) {
    const nodes = sim!.nodes;
    const focus = hovered ?? selected;
    const placed: Array<[number, number, number, number]> = [];
    const font = colors.font;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    ctx.lineJoin = "round";
    // Zoomed out, only the hubs speak; zoomed in, everyone does.
    const budget = cam.k >= 1.4 ? MAX_LABELS : Math.max(5, Math.round(MAX_LABELS * cam.k * cam.k * 0.18));
    const order = lit
      ? [...(focus !== null ? [focus] : []), ...byDegree.filter((i) => lit[i] && i !== focus)]
      : byDegree;
    let shown = 0;
    // Folder names first: they are the map's regions.
    if (!lit) drawRegions(ctx, sx, sy, placed, font, islands);
    for (const i of order) {
      if (shown >= (lit ? MAX_LABELS : budget)) break;
      if (!visible(i)) continue;
      if (!lit && nodes.length > 12 && items[i].near.length === 0 && cam.k < 1.2) continue;
      const strong = i === focus;
      ctx.font = `${strong ? 600 : 500} ${strong ? 13 : 12}px ${font}`;
      const text = items[i].title.length > 34 ? `${items[i].title.slice(0, 33)}…` : items[i].title;
      if (labelWidth[i] < 0 || strong) labelWidth[i] = ctx.measureText(text).width;
      const width = labelWidth[i];
      const x = sx(nodes[i].x);
      const y = sy(nodes[i].y) + screenRadius(nodes[i].r) + 4;
      const box: [number, number, number, number] = [x - width / 2 - 3, y - 1, x + width / 2 + 3, y + 16];
      if (!strong && placed.some((p) => box[0] < p[2] && box[2] > p[0] && box[1] < p[3] && box[3] > p[1])) continue;
      placed.push(box);
      ctx.lineWidth = 4;
      ctx.strokeStyle = colors.surface;
      ctx.strokeText(text, x, y);
      ctx.fillStyle = strong || (lit && lit[i]) ? colors.text : colors.soft;
      ctx.fillText(text, x, y);
      shown++;
    }
  }

  /* -------------------------------------------------------------- camera */

  function resize() {
    if (!stage || !canvas) return;
    const rect = stage.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    // Keep the middle of the picture where it was.
    if (size.w > 0) {
      cam.tx += (rect.width - size.w) / 2;
      cam.ty += (rect.height - size.h) / 2;
    }
    size = { w: rect.width, h: rect.height, dpr };
    canvas.width = Math.max(1, Math.round(rect.width * dpr));
    canvas.height = Math.max(1, Math.round(rect.height * dpr));
    canvas.style.width = `${rect.width}px`;
    canvas.style.height = `${rect.height}px`;
    invalidate();
  }

  function flyTo(to: typeof cam, animate = true) {
    if (!animate || reducedMotion) {
      cam = to;
      flight = null;
      invalidate();
      return;
    }
    flight = { from: { ...cam }, to, start: performance.now(), ms: 520 };
    schedule();
  }

  /** Frame `indices` (every page when empty). */
  function fit(animate = true, indices: number[] = []) {
    const to = fitTarget(indices);
    if (to) flyTo(to, animate);
  }

  /** The camera that frames `indices` (every page when empty). */
  function fitTarget(indices: number[] = []): typeof cam | null {
    if (!sim || sim.nodes.length === 0 || size.w === 0) return null;
    const list = indices.length ? indices.map((i) => sim!.nodes[i]) : sim.nodes;
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const n of list) {
      x0 = Math.min(x0, n.x - n.r);
      y0 = Math.min(y0, n.y - n.r);
      x1 = Math.max(x1, n.x + n.r);
      y1 = Math.max(y1, n.y + n.r);
    }
    const pad = 64;
    const k = Math.min(
      MAX_ZOOM,
      indices.length ? 2 : 1.6,
      Math.max(MIN_ZOOM, Math.min((size.w - pad * 2) / Math.max(1, x1 - x0), (size.h - pad * 2) / Math.max(1, y1 - y0))),
    );
    return { k, tx: size.w / 2 - ((x0 + x1) / 2) * k, ty: size.h / 2 - ((y0 + y1) / 2) * k };
  }

  function zoomAt(factor: number, px = size.w / 2, py = size.h / 2) {
    const k = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, cam.k * factor));
    const f = k / cam.k;
    cam = { k, tx: px - (px - cam.tx) * f, ty: py - (py - cam.ty) * f };
    flight = null;
    userMoved = true;
    invalidate();
  }

  function zoomButton(factor: number) {
    userMoved = true;
    const k = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, cam.k * factor));
    const f = k / cam.k;
    flyTo({ k, tx: size.w / 2 - (size.w / 2 - cam.tx) * f, ty: size.h / 2 - (size.h / 2 - cam.ty) * f });
  }

  function refit() {
    userMoved = false;
    fit(true, isolated !== null ? items.flatMap((it, i) => (it.group === isolated ? [i] : [])) : []);
  }

  /** Bring a page into view and select it. */
  function focusNode(i: number) {
    selected = i;
    userMoved = true;
    const n = sim?.nodes[i];
    if (!n) return;
    const k = Math.max(cam.k, 1.1);
    // Leave room for the card on the right.
    const offset = size.w > 720 ? 150 : 0;
    flyTo({ k, tx: size.w / 2 - offset - n.x * k, ty: size.h / 2 - n.y * k });
  }

  /* --------------------------------------------------------- interaction */

  function nodeAt(px: number, py: number): number | null {
    if (!sim) return null;
    let best: number | null = null;
    let bestD = Infinity;
    const nodes = sim.nodes;
    for (let i = 0; i < nodes.length; i++) {
      const dx = nodes[i].x * cam.k + cam.tx - px;
      const dy = nodes[i].y * cam.k + cam.ty - py;
      const d = dx * dx + dy * dy;
      const reach = screenRadius(nodes[i].r) + 5;
      if (d < reach * reach && d < bestD) {
        best = i;
        bestD = d;
      }
    }
    return best;
  }

  function local(e: PointerEvent | WheelEvent | MouseEvent) {
    const rect = canvas!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  let press: { x: number; y: number; node: number | null; tx: number; ty: number; moved: boolean } | null = null;

  function onDown(e: PointerEvent) {
    if (e.button !== 0 || !canvas) return;
    const p = local(e);
    press = { ...p, node: nodeAt(p.x, p.y), tx: cam.tx, ty: cam.ty, moved: false };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    flight = null;
  }

  function onMove(e: PointerEvent) {
    if (!canvas || !sim) return;
    const p = local(e);
    if (press) {
      if (!press.moved && Math.hypot(p.x - press.x, p.y - press.y) < 4) return;
      press.moved = true;
      dragging = true;
      userMoved = true;
      if (press.node !== null) {
        const n = sim.nodes[press.node];
        n.fx = (p.x - cam.tx) / cam.k;
        n.fy = (p.y - cam.ty) / cam.k;
        hovered = press.node;
        sim.reheat(0.12);
      } else {
        cam = { ...cam, tx: press.tx + (p.x - press.x), ty: press.ty + (p.y - press.y) };
      }
      invalidate();
      return;
    }
    const hit = nodeAt(p.x, p.y);
    if (hit !== hovered) {
      hovered = hit;
      invalidate();
    }
    tip = p;
    (e.currentTarget as HTMLElement).style.cursor = hit !== null ? "pointer" : "";
  }

  function onUp(e: PointerEvent) {
    if (!press || !sim) return;
    const was = press;
    press = null;
    dragging = false;
    (e.currentTarget as Element | null)?.releasePointerCapture?.(e.pointerId);
    if (was.node !== null) {
      const n = sim.nodes[was.node];
      n.fx = null;
      n.fy = null;
      remembered.set(n.id, { x: n.x, y: n.y });
    }
    if (!was.moved) {
      // A click: select the page under the pointer, or clear the selection.
      selected = was.node;
    }
    invalidate();
  }

  function onLeave() {
    if (press) return;
    hovered = null;
    invalidate();
  }

  function onDouble(e: MouseEvent) {
    const p = local(e);
    const hit = nodeAt(p.x, p.y);
    if (hit !== null) void openPage(items[hit].id);
    else zoomAt(1.8, p.x, p.y);
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const p = local(e);
    // Trackpad pinches arrive as ctrl+wheel with small deltas.
    const speed = e.ctrlKey ? 0.012 : 0.0016;
    const delta = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    zoomAt(Math.exp(-delta * speed), p.x, p.y);
  }

  function onCanvasKey(e: KeyboardEvent) {
    const step = 60;
    const keys: Record<string, () => void> = {
      "+": () => zoomButton(1.4),
      "=": () => zoomButton(1.4),
      "-": () => zoomButton(1 / 1.4),
      "0": refit,
      ArrowLeft: () => flyTo({ ...cam, tx: cam.tx + step }, false),
      ArrowRight: () => flyTo({ ...cam, tx: cam.tx - step }, false),
      ArrowUp: () => flyTo({ ...cam, ty: cam.ty + step }, false),
      ArrowDown: () => flyTo({ ...cam, ty: cam.ty - step }, false),
      Enter: () => selected !== null && void openPage(items[selected].id),
      Escape: () => (selected = null),
    };
    const run = keys[e.key];
    if (!run || e.ctrlKey || e.metaKey || e.altKey) return;
    e.preventDefault();
    e.stopPropagation();
    userMoved = true;
    run();
    invalidate();
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (matches.length === 0) return;
      const n = Math.min(matches.length, 8);
      activeMatch = (activeMatch + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    } else if (e.key === "Enter") {
      e.preventDefault();
      const i = matches[activeMatch];
      if (i === undefined) return;
      if (selected === i) void openPage(items[i].id);
      else focusNode(i);
    } else if (e.key === "Escape" && query) {
      e.preventDefault();
      e.stopPropagation();
      query = "";
    }
  }

  function toggleIsolate(g: number) {
    isolated = isolated === g ? null : g;
    selected = null;
    if (isolated !== null) fit(true, items.flatMap((it, i) => (it.group === g ? [i] : [])));
    invalidate();
  }

  async function toggleAll() {
    showAll = !showAll;
    try {
      localStorage.setItem("herbarium.graph.all", showAll ? "1" : "0");
    } catch {
      /* the choice just won't persist */
    }
    userMoved = false;
    await load();
  }

  // Anything shown differently: redraw.
  $effect(() => {
    void matches;
    void selected;
    void isolated;
    invalidate();
  });
  $effect(() => {
    activeMatch = matches.length ? Math.min(activeMatch, Math.min(matches.length, 8) - 1) : 0;
  });
  $effect(() => {
    void app.theme;
    void app.appearance;
    // Wait for the theme's tokens to land on the page.
    requestAnimationFrame(() => readColors());
  });
  $effect(() => {
    void app.vaultRevision;
    void app.library.length;
    void load();
  });

  let observer: ResizeObserver | null = null;
  onMount(() => {
    observer = new ResizeObserver(() => resize());
    if (stage) observer.observe(stage);
  });

  onDestroy(() => {
    observer?.disconnect();
    if (raf) cancelAnimationFrame(raf);
    if (sim) for (const n of sim.nodes) remembered.set(n.id, { x: n.x, y: n.y });
  });
</script>

<div class="pane">
  <header class="head">
    <div>
      <p class="eyebrow">{t("graph.eyebrow")}</p>
      <h1 class="display">{t("graph.title")}</h1>
      {#if graph}<p class="sub">{summary}</p>{/if}
    </div>
    <label class="toggle">
      <input type="checkbox" checked={showAll} onchange={toggleAll} />
      {t("graph.showAll")}
    </label>
  </header>

  {#if error}
    <p class="notice notice-error" role="alert"><Icon name="info" size={14} />{error}</p>
  {:else if graph && graph.nodes.length === 0}
    <div class="empty card">
      <p><strong>{t("graph.emptyTitle")}</strong></p>
      <p class="muted">{t("graph.emptyText")}</p>
    </div>
  {/if}

  <div class="stage" class:hidden={!graph || graph.nodes.length === 0 || !!error} bind:this={stage}>
    <!-- A pan-and-zoom surface driven by pointer and keys (arrows, +, -, 0,
         Enter, Escape): ARIA's "application" role, which Svelte lists as
         non-interactive. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="surface"
      class:dragging
      tabindex="0"
      role="application"
      aria-roledescription="graph"
      aria-label={graph ? t("graph.label", { pages: graph.nodes.length, links: linkCount }) : ""}
      onpointerdown={onDown}
      onpointermove={onMove}
      onpointerup={onUp}
      onpointercancel={onUp}
      onpointerleave={onLeave}
      ondblclick={onDouble}
      onwheel={onWheel}
      onkeydown={onCanvasKey}
    >
      <canvas bind:this={canvas} aria-hidden="true"></canvas>
    </div>

    {#if arranging}
      <p class="arranging" role="status">{t("graph.arranging")}</p>
    {/if}

    <div class="toolbar">
      <div class="search" role="combobox" aria-expanded={matches.length > 0} aria-haspopup="listbox" aria-controls="graph-matches">
        <Icon name="search" size={13} />
        <input
          bind:value={query}
          type="search"
          placeholder={t("graph.searchPlaceholder")}
          aria-label={t("graph.search")}
          aria-autocomplete="list"
          aria-activedescendant={matches.length ? `graph-match-${activeMatch}` : undefined}
          spellcheck="false"
          onkeydown={onSearchKey}
        />
      </div>
      {#if query.trim()}
        <ul class="matches card" id="graph-matches" role="listbox" aria-label={t("graph.search")}>
          {#each matches.slice(0, 8) as i, n (i)}
            <li
              id={`graph-match-${n}`}
              role="option"
              aria-selected={n === activeMatch}
              class:active={n === activeMatch}
            >
              <button type="button" tabindex="-1" onmousedown={(e) => e.preventDefault()} onclick={() => focusNode(i)}>
                <span class="dot" style={`background:${groups[items[i].group]?.css}`}></span>
                <span class="title">{items[i].title}</span>
                <span class="meta">{t("graph.degree", { count: items[i].near.length })}</span>
              </button>
            </li>
          {:else}
            <li class="none">{t("graph.noMatch")}</li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="zoom" class:beside={!!card}>
      <button class="btn btn-icon btn-ghost" onclick={() => zoomButton(1.4)} aria-label={t("graph.zoomIn")} title={t("graph.zoomIn")}>
        <Icon name="plus" size={14} />
      </button>
      <button class="btn btn-icon btn-ghost" onclick={() => zoomButton(1 / 1.4)} aria-label={t("graph.zoomOut")} title={t("graph.zoomOut")}>
        <Icon name="minus" size={14} />
      </button>
      <button class="btn btn-icon btn-ghost" onclick={refit} aria-label={t("graph.fit")} title={t("graph.fit")}>
        <Icon name="corners-out" size={14} />
      </button>
    </div>

    {#if groups.length > 1}
      <ul class="legend" class:beside={!!card} aria-label={t("graph.legend")}>
        {#each groups as g, i (g.key)}
          {#if !g.other}
            <li>
              <button
                type="button"
                class:on={isolated === i}
                class:off={isolated !== null && isolated !== i}
                aria-pressed={isolated === i}
                title={t("graph.isolate", { folder: g.label })}
                onclick={() => toggleIsolate(i)}
              >
                <span class="dot" style={`background:${g.css}`}></span>{g.label}<span class="count">{g.count}</span>
              </button>
            </li>
          {/if}
        {/each}
        {#if groups.some((g) => g.other)}
          <li class="other">
            <span class="dot" style="background:var(--series-other)"></span>{t("graph.other", {
              count: groups.filter((g) => g.other).length,
            })}
          </li>
        {/if}
      </ul>
    {/if}

    {#if card}
      <aside class="side card" aria-label={card.title}>
        <header>
          <span class="dot big" style={`background:${card.group?.css}`}></span>
          <div class="who">
            <strong>{card.title}</strong>
            <span>{card.group?.label} · {t("graph.degree", { count: card.near.length })}</span>
          </div>
          <button class="btn btn-icon btn-ghost" onclick={() => (selected = null)} aria-label={t("graph.closeCard")}>
            <Icon name="x" size={14} />
          </button>
        </header>
        <div class="lists">
          {#if card.near.length === 0}
            <p class="muted small">{t("graph.noLinks")}</p>
          {/if}
          {#each [{ label: t("graph.linksTo"), list: card.out }, { label: t("graph.linkedFrom"), list: card.in }] as part (part.label)}
            {#if part.list.length}
              <p class="eyebrow">{part.label}</p>
              <ul>
                {#each part.list as n (n.index)}
                  <li>
                    <button type="button" onclick={() => focusNode(n.index)}>
                      <span class="dot" style={`background:${n.color}`}></span>{n.title}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          {/each}
        </div>
        <button class="btn btn-primary open" onclick={() => void openPage(card.id)}>{t("graph.open")}</button>
      </aside>
    {/if}

    {#if hovered !== null && hovered !== selected && items[hovered] && !dragging}
      <div class="tip" style={`left:${tip.x + 14}px;top:${tip.y + 14}px`} role="tooltip">
        <strong>{items[hovered].title}</strong>
        <span>{groups[items[hovered].group]?.label} · {t("graph.degree", { count: items[hovered].near.length })}</span>
      </div>
    {/if}
  </div>

  {#if graph && graph.nodes.length > 0}
    <p class="hint">{t("graph.hint")}</p>
  {/if}
</div>

<style>
  .pane {
    /* Categorical slots 1–3 of the validated reference palette, and a neutral "Other". */
    --series-1: #2a78d6;
    --series-2: #eb6834;
    --series-3: #1baf7a;
    --series-other: #9a9893;
    --graph-edge: rgba(107, 106, 104, 0.32);
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 32px 40px 20px;
    overflow: hidden;
  }
  :global(:root[data-theme="dark"]) .pane {
    --series-1: #3987e5;
    --series-2: #d95926;
    --series-3: #199e70;
    --series-other: #75736e;
    --graph-edge: rgba(196, 195, 192, 0.2);
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  .head h1 {
    margin: 4px 0 0;
  }
  .sub {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--text-soft);
  }

  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background:
      radial-gradient(circle, color-mix(in srgb, var(--muted) 16%, transparent) 1px, transparent 1.2px) 0 0 / 22px 22px,
      var(--surface);
    overflow: hidden;
  }
  .stage.hidden {
    display: none;
  }
  .surface {
    position: absolute;
    inset: 0;
    cursor: grab;
    touch-action: none;
    outline: none;
  }
  .surface.dragging {
    cursor: grabbing;
  }
  canvas {
    display: block;
  }
  .surface:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
    border-radius: var(--radius-lg);
  }

  /* Floating controls sit on the surface, never over the middle of the picture. */
  .toolbar {
    position: absolute;
    top: 12px;
    left: 12px;
    width: min(300px, calc(100% - 120px));
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--muted);
    box-shadow: var(--shadow);
  }
  .search:focus-within {
    border-color: var(--border-input-hover);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
  }
  .matches {
    list-style: none;
    margin: 6px 0 0;
    padding: 4px;
    max-height: 300px;
    overflow: auto;
    box-shadow: var(--shadow-lg);
  }
  .matches li.none {
    padding: 8px 10px;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .matches button,
  .lists button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
  }
  .matches li.active button,
  .matches button:hover,
  .lists button:hover {
    background: var(--accent-soft);
  }
  .matches .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .matches .meta {
    color: var(--muted);
    font-size: var(--fs-xs);
    white-space: nowrap;
  }

  .zoom {
    position: absolute;
    right: 12px;
    bottom: 12px;
    display: flex;
    flex-direction: column;
    padding: 2px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface);
    box-shadow: var(--shadow);
  }

  .zoom.beside {
    right: 314px;
  }
  .legend {
    position: absolute;
    left: 12px;
    bottom: 12px;
    max-width: calc(100% - 80px);
    list-style: none;
    margin: 0;
    padding: 4px;
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface);
    box-shadow: var(--shadow);
    font-size: var(--fs-xs);
  }
  .legend button,
  .legend .other {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-soft);
    font: inherit;
  }
  .legend button {
    cursor: pointer;
  }
  .legend button:hover {
    background: var(--accent-soft);
  }
  .legend button.on {
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 500;
  }
  .legend button.off {
    opacity: 0.5;
  }
  .legend.beside {
    max-width: calc(100% - 380px);
  }
  .legend .count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .legend .other {
    color: var(--muted);
  }
  .dot {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  .dot.big {
    width: 12px;
    height: 12px;
    margin-top: 4px;
  }

  .side {
    position: absolute;
    top: 12px;
    right: 12px;
    bottom: 12px;
    width: min(290px, calc(100% - 24px));
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    box-shadow: var(--shadow-lg);
    animation: slide 0.18s ease-out;
  }
  @keyframes slide {
    from {
      opacity: 0;
      transform: translateX(8px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .side {
      animation: none;
    }
  }
  .side header {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .who strong {
    font-family: var(--font-display);
    font-size: 18px;
    font-weight: 500;
    line-height: 1.25;
    overflow-wrap: anywhere;
  }
  .who span {
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .lists {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0 -6px;
    padding: 0 6px;
  }
  .lists .eyebrow {
    margin: 10px 0 4px 8px;
  }
  .lists ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .small {
    font-size: var(--fs-sm);
  }
  .open {
    width: 100%;
    justify-content: center;
  }

  .tip {
    position: absolute;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-width: 260px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    box-shadow: var(--shadow-lg);
    font-size: var(--fs-sm);
    pointer-events: none;
  }
  .tip span {
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .arranging {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    margin: 0;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .hint {
    margin: 0;
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .muted {
    color: var(--muted);
  }
  .empty {
    padding: 24px;
  }
  .empty p {
    margin: 0 0 6px;
  }
</style>
