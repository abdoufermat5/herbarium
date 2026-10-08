<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { app, errorMessage, openPage } from "../lib/state.svelte";
  import { t } from "../lib/i18n.svelte";
  import type { PageGraph } from "../lib/types";
  import Icon from "../lib/Icon.svelte";

  // Node colour is identity (top-level folder). Nodes can sit next to any
  // other node, so only the first three categorical slots are used (they
  // validate all-pairs in both themes); every other folder is "Other".
  const SLOTS = 3;

  interface Node {
    id: string;
    title: string;
    group: string;
    degree: number;
    x: number;
    y: number;
    vx: number;
    vy: number;
  }

  let graph = $state<PageGraph | null>(null);
  let showAll = $state(false);
  let error = $state<string | null>(null);
  let nodes = $state<Node[]>([]);
  let edges = $state<Array<[Node, Node]>>([]);
  let hover = $state<Node | null>(null);
  let hoverPos = $state({ x: 0, y: 0 });
  let view = $state({ x: -500, y: -350, w: 1000, h: 700 });
  let svgEl: SVGSVGElement | undefined = $state();

  const groups = $derived.by(() => {
    if (!graph) return [] as string[];
    const counts = new Map<string, number>();
    for (const n of graph.nodes) {
      const g = topFolder(n.folder);
      counts.set(g, (counts.get(g) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([g]) => g);
  });
  const slotOf = $derived(new Map(groups.slice(0, SLOTS).map((g, i) => [g, i + 1])));
  const hasOther = $derived(groups.length > SLOTS);
  const summary = $derived(
    t("graph.summary", {
      pages: t("unit.page", { count: nodes.length }),
      links: t("graph.degree", { count: edges.length }),
    }),
  );

  function topFolder(folder: string | null): string {
    return folder ? folder.split("/")[0] : "";
  }

  function fill(group: string): string {
    const slot = slotOf.get(group);
    return slot ? `var(--series-${slot})` : "var(--series-other)";
  }

  function groupLabel(group: string): string {
    return group || t("graph.root");
  }

  /** Distance beyond which nodes no longer push each other apart. */
  const CUTOFF = 240;
  /** The cell itself and half its neighbours, so each pair of cells meets once. */
  const NEIGHBOURS: Array<[number, number]> = [
    [0, 0],
    [1, 0],
    [-1, 1],
    [0, 1],
    [1, 1],
  ];

  /** A force layout run to rest up front: repulsion, springs on links, a pull to the centre. */
  function layout(data: PageGraph) {
    const count = data.nodes.length;
    const radius = 40 + Math.sqrt(count) * 40;
    const byId = new Map<string, Node>();
    const list: Node[] = data.nodes.map((n, i) => {
      const angle = (i / Math.max(1, count)) * Math.PI * 2;
      const node: Node = {
        id: n.id,
        title: n.title,
        group: topFolder(n.folder),
        degree: n.degree,
        x: Math.cos(angle) * radius,
        y: Math.sin(angle) * radius,
        vx: 0,
        vy: 0,
      };
      byId.set(n.id, node);
      return node;
    });
    // Pages that link both ways are one line on the map, pulled once.
    const seen = new Set<string>();
    const links = data.edges
      .map(([a, b]) => [byId.get(a), byId.get(b)] as const)
      .filter((e): e is readonly [Node, Node] => !!e[0] && !!e[1])
      .filter(([a, b]) => {
        const key = a.id < b.id ? `${a.id}\n${b.id}` : `${b.id}\n${a.id}`;
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      })
      .map((e) => [e[0], e[1]] as [Node, Node]);
    // Fewer passes for big graphs: each one costs more, and they settle sooner.
    const iterations = Math.min(400, 80 + count * 2, 60 + Math.round(300_000 / Math.max(1, count)));
    for (let step = 0; step < iterations; step++) {
      const cool = 1 - step / iterations;
      // Repulsion between nearby nodes only: beyond CUTOFF the push is far
      // weaker than the pull to the centre, and comparing every pair froze
      // the window on large vaults. Nodes are bucketed in a grid of
      // CUTOFF-sized cells and each one meets those in its own and the next cells.
      const grid = new Map<string, Node[]>();
      for (const n of list) {
        const key = `${Math.floor(n.x / CUTOFF)},${Math.floor(n.y / CUTOFF)}`;
        const cell = grid.get(key);
        if (cell) cell.push(n);
        else grid.set(key, [n]);
      }
      for (const [key, cell] of grid) {
        const [cx, cy] = key.split(",").map(Number);
        for (const [ox, oy] of NEIGHBOURS) {
          const other = ox === 0 && oy === 0 ? cell : grid.get(`${cx + ox},${cy + oy}`);
          if (!other) continue;
          for (let i = 0; i < cell.length; i++) {
            const a = cell[i];
            for (let j = other === cell ? i + 1 : 0; j < other.length; j++) {
              const b = other[j];
              let dx = a.x - b.x;
              let dy = a.y - b.y;
              let d2 = dx * dx + dy * dy;
              if (d2 > CUTOFF * CUTOFF) continue;
              if (d2 < 0.01) {
                dx = Math.random() - 0.5;
                dy = Math.random() - 0.5;
                d2 = 0.5;
              }
              const force = 2400 / d2;
              const d = Math.sqrt(d2);
              a.vx += (dx / d) * force;
              a.vy += (dy / d) * force;
              b.vx -= (dx / d) * force;
              b.vy -= (dy / d) * force;
            }
          }
        }
      }
      for (const [a, b] of links) {
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const d = Math.sqrt(dx * dx + dy * dy) || 1;
        const pull = (d - 90) * 0.02;
        a.vx += (dx / d) * pull * d * 0.05;
        a.vy += (dy / d) * pull * d * 0.05;
        b.vx -= (dx / d) * pull * d * 0.05;
        b.vy -= (dy / d) * pull * d * 0.05;
      }
      for (const n of list) {
        n.vx -= n.x * 0.01;
        n.vy -= n.y * 0.01;
        const speed = Math.min(30 * cool + 1, Math.hypot(n.vx, n.vy));
        const len = Math.hypot(n.vx, n.vy) || 1;
        n.x += (n.vx / len) * speed;
        n.y += (n.vy / len) * speed;
        n.vx *= 0.5;
        n.vy *= 0.5;
      }
    }
    nodes = list;
    edges = links;
    fit();
  }

  function fit() {
    if (nodes.length === 0) return;
    const xs = nodes.map((n) => n.x);
    const ys = nodes.map((n) => n.y);
    const pad = 80;
    const minX = Math.min(...xs) - pad;
    const minY = Math.min(...ys) - pad;
    const w = Math.max(...xs) + pad - minX;
    const h = Math.max(...ys) + pad - minY;
    // Never zoom in past life size: a small graph stays small, centred.
    const vw = Math.max(w, 900);
    const vh = Math.max(h, 560);
    view = { x: minX - (vw - w) / 2, y: minY - (vh - h) / 2, w: vw, h: vh };
  }

  function radius(n: Node): number {
    return 5 + Math.min(10, Math.sqrt(n.degree) * 2.4);
  }

  async function load() {
    try {
      graph = await api.pageGraph(showAll);
      error = null;
      layout(graph);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    if (!svgEl) return;
    const rect = svgEl.getBoundingClientRect();
    const px = view.x + ((e.clientX - rect.left) / rect.width) * view.w;
    const py = view.y + ((e.clientY - rect.top) / rect.height) * view.h;
    const k = Math.exp(e.deltaY * 0.0015);
    const w = Math.min(20000, Math.max(120, view.w * k));
    const h = (view.h / view.w) * w;
    view = { x: px - ((px - view.x) / view.w) * w, y: py - ((py - view.y) / view.h) * h, w, h };
  }

  let drag: { x: number; y: number; vx: number; vy: number } | null = null;
  function onDown(e: PointerEvent) {
    if ((e.target as Element).closest(".node")) return;
    drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  }
  function onMove(e: PointerEvent) {
    if (!drag || !svgEl) return;
    const rect = svgEl.getBoundingClientRect();
    view = {
      ...view,
      x: drag.vx - ((e.clientX - drag.x) / rect.width) * view.w,
      y: drag.vy - ((e.clientY - drag.y) / rect.height) * view.h,
    };
  }
  function onUp() {
    drag = null;
  }

  function enter(n: Node, e: PointerEvent | FocusEvent) {
    hover = n;
    if ("clientX" in e && svgEl) {
      const rect = svgEl.getBoundingClientRect();
      hoverPos = { x: e.clientX - rect.left, y: e.clientY - rect.top };
    } else if (svgEl) {
      const rect = svgEl.getBoundingClientRect();
      hoverPos = {
        x: ((n.x - view.x) / view.w) * rect.width,
        y: ((n.y - view.y) / view.h) * rect.height,
      };
    }
  }

  $effect(() => {
    void app.vaultRevision;
    void app.library.length;
    void load();
  });

  onMount(() => void load());
</script>

<div class="pane">
  <header class="head">
    <div>
      <p class="eyebrow">{t("graph.eyebrow")}</p>
      <h1 class="display">{t("graph.title")}</h1>
      {#if graph}
        <p class="sub">{summary}</p>
      {/if}
    </div>
    <div class="tools">
      <label class="toggle">
        <input type="checkbox" bind:checked={showAll} onchange={load} />
        {t("graph.showAll")}
      </label>
      <button class="btn btn-sm" onclick={fit} disabled={nodes.length === 0}>{t("graph.fit")}</button>
    </div>
  </header>

  {#if error}
    <p class="notice notice-error" role="alert"><Icon name="info" size={14} />{error}</p>
  {:else if graph && graph.nodes.length === 0}
    <div class="empty card">
      <p><strong>{t("graph.emptyTitle")}</strong></p>
      <p class="muted">{t("graph.emptyText")}</p>
    </div>
  {:else if graph}
    {#if groups.length > 1}
      <ul class="legend" aria-label={t("graph.legend")}>
        {#each groups.slice(0, SLOTS) as g (g)}
          <li><span class="dot" style={`background:${fill(g)}`}></span>{groupLabel(g)}</li>
        {/each}
        {#if hasOther}
          <li><span class="dot" style="background:var(--series-other)"></span>{t("graph.other", { count: groups.length - SLOTS })}</li>
        {/if}
      </ul>
    {/if}
    <div class="stage">
      <svg
        bind:this={svgEl}
        viewBox={`${view.x} ${view.y} ${view.w} ${view.h}`}
        role="img"
        aria-label={t("graph.label", { pages: graph.nodes.length, links: edges.length })}
        onwheel={onWheel}
        onpointerdown={onDown}
        onpointermove={onMove}
        onpointerup={onUp}
        onpointercancel={onUp}
      >
        <g class="edges">
          {#each edges as [a, b], i (i)}
            <line
              x1={a.x}
              y1={a.y}
              x2={b.x}
              y2={b.y}
              class:lit={hover && (hover.id === a.id || hover.id === b.id)}
            />
          {/each}
        </g>
        <g>
          {#each nodes as n (n.id)}
            <g
              class="node"
              transform={`translate(${n.x} ${n.y})`}
              role="link"
              tabindex="0"
              aria-label={n.title}
              onpointerenter={(e) => enter(n, e)}
              onpointerleave={() => (hover = null)}
              onfocus={(e) => enter(n, e)}
              onblur={() => (hover = null)}
              onclick={() => void openPage(n.id)}
              onkeydown={(e) => e.key === "Enter" && void openPage(n.id)}
            >
              <circle r={radius(n) + 6} class="hit" />
              <circle r={radius(n)} fill={fill(n.group)} class="dot-mark" />
              {#if n.degree >= 3 || nodes.length <= 12}
                <text y={radius(n) + 14} text-anchor="middle">{n.title.length > 28 ? `${n.title.slice(0, 27)}…` : n.title}</text>
              {/if}
            </g>
          {/each}
        </g>
      </svg>
      {#if hover}
        <div class="tip" style={`left:${hoverPos.x + 12}px;top:${hoverPos.y + 12}px`} role="tooltip">
          <strong>{hover.title}</strong>
          <span>{groupLabel(hover.group)} · {t("graph.degree", { count: hover.degree })}</span>
        </div>
      {/if}
    </div>
    <p class="hint muted">{t("graph.hint")}</p>
  {/if}
</div>

<style>
  .pane {
    /* Categorical slots 1–3 of the validated reference palette, and a neutral "Other". */
    --series-1: #2a78d6;
    --series-2: #eb6834;
    --series-3: #1baf7a;
    --series-other: #9a9893;
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 32px 40px 24px;
    overflow: hidden;
  }
  :global(:root[data-theme="dark"]) .pane {
    --series-1: #3987e5;
    --series-2: #d95926;
    --series-3: #199e70;
    --series-other: #75736e;
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
    margin: 4px 0 0;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--text-soft);
  }
  .legend {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    font-size: var(--fs-sm);
    color: var(--text-soft);
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    overflow: hidden;
  }
  svg {
    width: 100%;
    height: 100%;
    display: block;
    cursor: grab;
    touch-action: none;
  }
  .edges line {
    stroke: var(--border-strong, var(--border));
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .edges line.lit {
    stroke: var(--text-soft);
  }
  .node {
    cursor: pointer;
    outline: none;
  }
  .node .hit {
    fill: transparent;
  }
  .node .dot-mark {
    stroke: var(--surface);
    stroke-width: 2;
  }
  .node:hover .dot-mark,
  .node:focus-visible .dot-mark {
    stroke: var(--text);
  }
  .node text {
    font-size: 12px;
    fill: var(--text-soft);
    pointer-events: none;
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
    box-shadow: var(--shadow-md, var(--shadow-lg));
    font-size: var(--fs-sm);
    pointer-events: none;
  }
  .tip span {
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .hint {
    margin: 0;
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
