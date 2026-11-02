<script lang="ts">
  // The tree that a reading path grows. Geometry comes from lib/tree.ts (one
  // mature model, thresholded); this component shows the elements whose
  // threshold has been reached and animates only what just crossed it.
  //
  // A page-sized step grows limb by limb: the fresh shapes are grouped per
  // (layer, limb) and each group springs in from its attachment point. A bigger
  // step, and the moment a path is fully grown, blooms as one picture: a single
  // animation on the <svg>, which the compositor can handle, because WebKit
  // redraws every path inside an animated SVG group on every frame.
  // Everything is CSS (one-shot, no idle animation) and honours
  // prefers-reduced-motion.
  //
  // Viewbox is 90 x 100, so the rendered width is 0.9 x `size`.
  import {
    LAYER,
    LAYER_COUNT,
    TREE_ASPECT,
    TREE_H,
    TREE_W,
    growTree,
    isGrown,
    treeProgress,
    treeStage,
    type TreeDetail,
    type TreeElement,
  } from "../lib/tree";
  import { t } from "../lib/i18n.svelte";

  let {
    seed,
    read,
    total,
    size = 160,
    detail = "full",
    label,
    highlight = null,
    onlimbclick,
    animate = true,
    growOnMount = false,
  }: {
    seed: string;
    read: number;
    total: number;
    /** Height in CSS px; the width is `size * 0.9`. */
    size?: number;
    detail?: TreeDetail;
    label?: string;
    /** Page index whose limb is emphasised; the rest dims. */
    highlight?: number | null;
    onlimbclick?: (pageIndex: number) => void;
    animate?: boolean;
    growOnMount?: boolean;
  } = $props();

  /** A run of sibling shapes that appears together: one animation for the run. */
  interface Band {
    key: string;
    /** Stagger, by limb order. */
    delay: number;
    /** Limb attachment point on the trunk: the band springs in from there. */
    x: number;
    y: number;
    items: TreeElement[];
  }
  interface LayerView {
    /** Settled shapes drawn before and after the fresh run of this paint layer. */
    before: TreeElement[];
    fresh: Band[];
    after: TreeElement[];
  }

  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  const model = $derived(growTree(seed, total, detail));
  const progress = $derived(treeProgress(read, total));
  const stage = $derived(treeStage(read, total));
  const mature = $derived(stage === "mature");
  const name = $derived(label ?? t(`tree.stage.${stage}`));
  const clickable = $derived(detail === "full" && !!onlimbclick);
  const moving = $derived(animate && !reduced);

  // The plant grows taller as pages are read. At progress 0 only the seed is
  // there, so it keeps its natural size; from then on the trunk starts short
  // and reaches full height when every page is read.
  const floor = $derived(detail === "mini" ? 0.7 : 0.55);
  const plantScale = $derived(progress <= 0 ? 1 : floor + (1 - floor) * (1 - (1 - progress) ** 2));

  // Progress already on screen. Plain variables on purpose: they must not
  // trigger anything, they only tell the next render what is new.
  let shownProgress = -1;
  let shownModel: unknown = null;

  const STAGGER_MS = 90; // between limb groups
  const DRAW_MAX = 60; // fresh branches still worth their own draw-in
  // Animating an SVG group makes WebKit redraw every path inside it each frame,
  // so only a step of up to about two pages of the deepest model gets per-limb
  // motion; a bigger one moves as one svg. A page-sized step of a short path
  // stays limb by limb; only a first visit or a mature entrance blooms the
  // whole svg.
  const STEP_MAX = 160; // fresh shapes still animated limb by limb

  const view = $derived.by(() => {
    const m = model;
    const p = progress;
    const from = !moving ? p : shownModel === m && shownProgress >= 0 ? shownProgress : growOnMount && shownProgress < 0 ? 0 : p;

    // Which limbs grow in this step: the stagger follows their order.
    const limbs: number[] = [];
    for (const el of m.elements) {
      const fresh = isGrown(el, p) && el.threshold > from + 1e-9;
      if (fresh && !limbs.includes(el.limb)) limbs.push(el.limb);
    }
    limbs.sort((a, b) => a - b);
    const rank = new Map(limbs.map((l, i) => [l, i]));

    const out: LayerView[] = Array.from({ length: LAYER_COUNT }, () => ({ before: [], fresh: [], after: [] }));
    for (const el of m.elements) {
      if (!isGrown(el, p)) continue;
      const layer = out[el.layer];
      if (el.threshold <= from + 1e-9) {
        (layer.fresh.length ? layer.after : layer.before).push(el);
        continue;
      }
      const last = layer.fresh[layer.fresh.length - 1];
      if (last && last.items[0].limb === el.limb) {
        last.items.push(el);
        continue;
      }
      // Base art, the trunk and the blossoms hang off no limb: they spring in
      // from the foot of the tree.
      const base = el.limb >= 0 ? m.limbs[el.limb] : undefined;
      layer.fresh.push({
        key: `${el.layer}:${el.limb}:${el.i}`,
        delay: STAGGER_MS * (rank.get(el.limb) ?? 0),
        x: Math.round((base ? base.x : m.baseX) * 10) / 10,
        y: Math.round((base ? base.y : m.ground) * 10) / 10,
        items: [el],
      });
    }
    return { layers: out, from };
  });

  const fresh = $derived.by(() => {
    let n = 0;
    for (const layer of view.layers) for (const band of layer.fresh) n += band.items.length;
    return n;
  });

  // A growth step small enough to animate limb by limb; a bigger one blooms in
  // as a whole picture (cheap: the svg composites, its groups do not).
  const stepped = $derived(moving && fresh > 0 && fresh <= STEP_MAX);
  // The plant itself blooms in: from nothing, or on the moment a path is grown.
  const entrance = $derived(moving && fresh > 0 && view.from <= 0 && (fresh > STEP_MAX || growOnMount));

  // A big step (a first visit to a mature path) would fire hundreds of draw-ins;
  // past sixty the groups carry the motion alone.
  const draw = $derived.by(() => {
    let n = 0;
    for (const layer of view.layers) for (const band of layer.fresh) for (const el of band.items) if (el.kind === "branch") n++;
    return n > 0 && n <= DRAW_MAX;
  });

  $effect(() => {
    shownProgress = progress;
    shownModel = model;
  });

  const dim = (page: number) => highlight != null && page !== highlight;
  let svgEl = $state<SVGSVGElement | null>(null);

  /** The grown limb nearest to a screen point, for clicks that miss every shape. */
  const nearest = (clientX: number, clientY: number): number => {
    const svg = svgEl;
    if (!svg) return -1;
    let best = -1;
    let bestD = 14;
    for (const path of svg.querySelectorAll<SVGPathElement>(".hits path[data-p]")) {
      const ctm = path.getScreenCTM();
      if (!ctm) continue;
      const page = Number(path.getAttribute("data-p"));
      const len = path.getTotalLength();
      for (let k = 0; k <= 12; k++) {
        const p = path.getPointAtLength((len * k) / 12).matrixTransform(ctm);
        const d = Math.hypot(p.x - clientX, p.y - clientY);
        if (d < bestD) {
          bestD = d;
          best = page;
        }
      }
    }
    return best;
  };

  const hit = (e: MouseEvent) => {
    if (!onlimbclick) return;
    const target = (e.target as Element | null)?.closest?.("[data-p]");
    const raw = target?.getAttribute("data-p");
    // The trunk is a growth marker, not a limb; a seed or base shape is not one either.
    const trunk = !!target?.classList.contains("branch") && target.classList.contains("d0");
    const page = raw != null ? Number(raw) : -1;
    if (page >= 0 && !trunk) {
      onlimbclick(page);
      return;
    }
    if (raw === "-1") return;
    const near = nearest(e.clientX, e.clientY);
    if (near >= 0) onlimbclick(near);
  };
  // The wide strokes are geometry proxies only: pointer-events stays off, so a
  // click always resolves the shape under the pointer first.
  const hits = $derived(
    clickable ? model.elements.filter((el) => el.layer === LAYER.branch && el.depth === 1 && el.page >= 0 && isGrown(el, progress)) : [],
  );
</script>

{#snippet elem(el: TreeElement, fresh: boolean)}
  <path
    class="e {el.kind} t{el.tone} d{el.depth}"
    class:dim={dim(el.page)}
    class:lit={highlight != null && el.page === highlight}
    class:draw={fresh && draw && el.kind === "branch"}
    d={el.d}
    stroke-width={el.w || undefined}
    pathLength={el.kind === "branch" ? 1 : undefined}
    data-p={el.page}
  />
{/snippet}

{#snippet paint(l: number)}
  <g>
    {#each view.layers[l].before as el (el.i)}{@render elem(el, false)}{/each}
  </g>
  {#each view.layers[l].fresh as band (band.key)}
    <g class="band" class:grow={stepped} style:--d="{band.delay}ms" style:--ox="{band.x}px" style:--oy="{band.y}px">
      {#each band.items as el (el.i)}{@render elem(el, true)}{/each}
    </g>
  {/each}
  <g>
    {#each view.layers[l].after as el (el.i)}{@render elem(el, false)}{/each}
  </g>
{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<svg
  bind:this={svgEl}
  class="tree {detail}"
  class:mature
  class:animated={moving}
  class:entrance={entrance}
  class:hover-rustle={detail === "full" && moving}
  class:clickable
  width={size * TREE_ASPECT}
  height={size}
  viewBox="0 0 {TREE_W} {TREE_H}"
  fill="none"
  role="img"
  aria-label={name}
  focusable="false"
  style:--bx="{model.baseX}px"
  style:--gy="{model.ground}px"
  style:--s={plantScale}
  style:--s0={floor}
  onclick={clickable ? hit : undefined}
>
  <title>{name}</title>
  {@render paint(0)}
  {@render paint(1)}
  <g class="plant">
    {@render paint(2)}
    {@render paint(3)}
  </g>
  {@render paint(4)}
  <g class="plant">
    {@render paint(5)}
    {@render paint(6)}
    {#if hits.length}
      <g class="hits">
        {#each hits as el (el.i)}
          <path class="hit" d={el.d} data-p={el.page} />
        {/each}
      </g>
    {/if}
  </g>
</svg>

<style>
  .tree {
    display: block;
    flex: none;
    overflow: visible;
    color: var(--text-soft);
  }
  .e {
    transition: opacity var(--t-med) var(--ease-out);
  }
  .e.dim {
    opacity: 0.45;
  }

  /* ink */
  .branch,
  .ground,
  .mound {
    stroke: currentColor;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .branch {
    fill: none;
  }
  /* aerial perspective: finer wood recedes; inline stroke-width is untouched */
  .branch.d0 {
    stroke-opacity: 1;
  }
  .branch.d1 {
    stroke-opacity: 0.94;
  }
  .branch.d2 {
    stroke-opacity: 0.88;
  }
  .branch.d3 {
    stroke-opacity: 0.78;
  }
  .branch.lit {
    stroke: var(--text);
    stroke-opacity: 1;
  }
  .ground {
    fill: none;
    opacity: 0.85;
  }
  .mound {
    fill: var(--c-clay-soft);
  }
  .seed {
    fill: var(--c-clay-soft);
    stroke: currentColor;
    stroke-width: 1;
    stroke-linejoin: round;
  }
  .sun {
    fill: var(--c-amber-soft);
  }

  /* foliage: soft clusters behind, richer ones in front */
  .leaf {
    stroke-width: 0.45;
    stroke-linejoin: round;
  }
  .leaf.t0 {
    fill: var(--c-sage-soft);
    stroke: var(--c-sage);
    stroke-opacity: 0.45;
  }
  .leaf.t1 {
    fill: var(--c-teal-soft);
    stroke: var(--c-teal);
    stroke-opacity: 0.4;
  }
  .full .leaf.t0 {
    fill-opacity: 0.42;
  }
  .full .leaf.t1 {
    fill-opacity: 0.55;
  }
  .leaf.t2 {
    fill: var(--c-sage);
    fill-opacity: 0.5;
    stroke: var(--leaf);
    stroke-opacity: 0.6;
  }
  .leaf.t3 {
    fill: var(--leaf);
    fill-opacity: 0.72;
    stroke: var(--leaf);
    stroke-opacity: 0.8;
  }
  .leaf.t4 {
    fill: var(--c-teal);
    fill-opacity: 0.5;
    stroke: var(--c-teal);
    stroke-opacity: 0.7;
  }
  .leaf.d3:not(.dim) {
    opacity: 0.88;
  }
  .mini .leaf {
    stroke-width: 1;
  }

  /* blossoms: a wash of rose with a warm heart, quiet enough to read as flowers */
  .petals {
    fill: var(--c-rose-soft);
    stroke: var(--c-rose);
    stroke-linejoin: round;
    opacity: 0.85;
  }
  .heart {
    fill: var(--c-amber);
    stroke: none;
  }

  /* limb geometry proxies: never the click target, only distance samples */
  .hit {
    fill: none;
    stroke: transparent;
    stroke-width: 7;
    stroke-linecap: round;
    pointer-events: none;
  }
  .clickable .leaf,
  .clickable .petals,
  .clickable .branch:not(.d0) {
    cursor: pointer;
  }

  /* growth: one one-shot animation per limb group, none per shape */
  .grow {
    transform-box: view-box;
    transform-origin: var(--ox) var(--oy);
    animation: grow-in var(--t-slow) var(--ease-out) var(--d, 0ms) backwards;
  }
  .grow .draw {
    stroke-dasharray: 1;
    animation: draw var(--t-slow) var(--ease-out) var(--d, 0ms) backwards;
  }
  @keyframes grow-in {
    from {
      opacity: 0;
      transform: scale(0.82);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  @keyframes draw {
    from {
      stroke-dashoffset: 1;
    }
    to {
      stroke-dashoffset: 0;
    }
  }

  /* the plant gets taller with progress, rooted at the trunk base */
  .plant {
    transform-box: view-box;
    transform-origin: var(--bx) var(--gy);
    transform: scale(var(--s, 1));
  }
  .animated .plant {
    transition: transform var(--t-slow) var(--ease-spring);
  }

  /* a whole tree blooming in (a first visit to a mature path): one animation on
     the svg, which the compositor can handle, instead of one per limb */
  .tree.entrance {
    transform-box: view-box;
    transform-origin: var(--bx) var(--gy);
    animation: plant-in var(--t-slow) var(--ease-spring);
  }
  @keyframes plant-in {
    from {
      opacity: 0;
      transform: scale(var(--s0));
    }
  }

  /* a single gentle rustle when the pointer arrives */
  @media (prefers-reduced-motion: no-preference) {
    .hover-rustle:hover .plant {
      animation: rustle 800ms var(--ease-in-out);
    }
  }
  @keyframes rustle {
    0%,
    100% {
      transform: rotate(0) scale(var(--s, 1));
    }
    25% {
      transform: rotate(-1.3deg) scale(var(--s, 1));
    }
    55% {
      transform: rotate(1deg) scale(var(--s, 1));
    }
    80% {
      transform: rotate(-0.4deg) scale(var(--s, 1));
    }
  }
</style>
