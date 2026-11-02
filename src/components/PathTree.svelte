<script lang="ts">
  // The tree that a reading path grows. Geometry comes from lib/tree.ts (one
  // mature model, thresholded); this component only shows the elements whose
  // threshold has been reached and animates the ones that just crossed it.
  // Viewbox is 90 x 100, so the rendered width is 0.9 x `size`.
  // Growth is CSS-only (one-shot animations); nothing here ever runs idle.
  import { LAYER_COUNT, TREE_ASPECT, TREE_H, TREE_W, growTree, isGrown, treeProgress, treeStage, type TreeDetail, type TreeElement } from "../lib/tree";
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

  interface Item {
    el: TreeElement;
    fresh: boolean;
    delay: number;
  }

  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  const model = $derived(growTree(seed, total, detail));
  const progress = $derived(treeProgress(read, total));
  const stage = $derived(treeStage(read, total));
  const mature = $derived(stage === "mature");
  const name = $derived(label ?? t(`tree.stage.${stage}`));
  const clickable = $derived(detail === "full" && !!onlimbclick);

  // The plant grows taller as pages are read. At progress 0 only the seed is
  // there, so it keeps its natural size; from then on the trunk starts short
  // and reaches full height when every page is read.
  const floor = $derived(detail === "mini" ? 0.7 : 0.55);
  const plantScale = $derived(progress <= 0 ? 1 : floor + (1 - floor) * (1 - (1 - progress) ** 2));

  // Progress already on screen. Plain variables on purpose: they must not
  // trigger anything, they only tell the next render what is new.
  let shownProgress = -1;
  let shownModel: unknown = null;

  const GROW_MS = 600; // spread of the stagger; each element then takes <= 500ms

  const layers = $derived.by(() => {
    const m = model;
    const p = progress;
    const from = !animate || reduced ? p : shownModel === m && shownProgress >= 0 ? shownProgress : growOnMount && shownProgress < 0 ? 0 : p;
    const span = Math.max(p - from, 1e-9);
    const out: Item[][] = Array.from({ length: LAYER_COUNT }, () => []);
    for (const el of m.elements) {
      if (!isGrown(el, p)) continue;
      const fresh = el.threshold > from + 1e-9;
      const delay = fresh ? Math.round(GROW_MS * Math.min(1, (el.threshold - from) / span) * 0.85 + 100 * el.seq) : 0;
      out[el.layer].push({ el, fresh, delay });
    }
    return out;
  });

  $effect(() => {
    shownProgress = progress;
    shownModel = model;
  });

  const dim = (page: number) => highlight != null && page !== highlight;
  const hit = (e: Event) => {
    if (!onlimbclick) return;
    const target = (e.target as Element).closest("[data-p]");
    const page = target ? Number(target.getAttribute("data-p")) : -1;
    if (page >= 0) onlimbclick(page);
  };
  const hits = $derived(clickable ? layers[3].filter((i) => i.el.page >= 0) : []);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<svg
  class="tree {detail}"
  class:mature
  class:animated={animate && !reduced}
  class:mount={animate && !reduced && growOnMount}
  class:hover-rustle={detail === "full" && animate && !reduced}
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
  {#each [0, 1] as layer (layer)}
    <g>
      {#each layers[layer] as it (it.el.i)}
        <path
          class="e {it.el.kind}"
          class:fresh={it.fresh}
          class:dim={dim(it.el.page)}
          d={it.el.d}
          stroke-width={it.el.w || undefined}
          style:--d={it.fresh ? `${it.delay}ms` : undefined}
        />
      {/each}
    </g>
  {/each}
  <g class="plant">
    {#each [2, 3] as layer (layer)}
      <g>
        {#each layers[layer] as it (it.el.i)}
          <path
            class="e {it.el.kind} t{it.el.tone} d{it.el.depth}"
            class:fresh={it.fresh}
            class:dim={dim(it.el.page)}
            class:lit={highlight != null && it.el.page === highlight}
            d={it.el.d}
            stroke-width={it.el.w || undefined}
            pathLength={it.el.kind === "branch" ? 1 : undefined}
            data-p={it.el.page}
            style:--d={it.fresh ? `${it.delay}ms` : undefined}
          />
        {/each}
      </g>
    {/each}
  </g>
  {#each [4] as layer (layer)}
    <g>
      {#each layers[layer] as it (it.el.i)}
        <path
          class="e {it.el.kind} t{it.el.tone} d{it.el.depth}"
          class:fresh={it.fresh}
          class:dim={dim(it.el.page)}
          d={it.el.d}
          stroke-width={it.el.w || undefined}
          style:--d={it.fresh ? `${it.delay}ms` : undefined}
        />
      {/each}
    </g>
  {/each}
  <g class="plant">
    {#each [5, 6] as layer (layer)}
      <g>
        {#each layers[layer] as it (it.el.i)}
          <path
            class="e {it.el.kind} t{it.el.tone} d{it.el.depth}"
            class:fresh={it.fresh}
            class:dim={dim(it.el.page)}
            class:lit={highlight != null && it.el.page === highlight}
            d={it.el.d}
            stroke-width={it.el.w || undefined}
            pathLength={it.el.kind === "branch" ? 1 : undefined}
            data-p={it.el.page}
            style:--d={it.fresh ? `${it.delay}ms` : undefined}
          />
        {/each}
      </g>
    {/each}
    {#if hits.length}
      <g class="hits">
        {#each hits as it (it.el.i)}
          <path class="hit" d={it.el.d} data-p={it.el.page} />
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
  .branch.lit {
    stroke: var(--text);
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

  /* hit strokes: invisible, wide */
  .hit {
    fill: none;
    stroke: transparent;
    stroke-width: 7;
    stroke-linecap: round;
    pointer-events: stroke;
    cursor: pointer;
  }
  .clickable .leaf,
  .clickable .petals {
    cursor: pointer;
  }

  /* growth: one-shot, only on elements that just appeared */
  .fresh {
    animation-duration: 500ms;
    animation-delay: var(--d, 0ms);
    animation-fill-mode: backwards;
  }
  .branch.fresh {
    stroke-dasharray: 1;
    animation-name: draw;
    animation-timing-function: var(--ease-out);
  }
  .leaf.fresh,
  .petals.fresh,
  .heart.fresh,
  .sun.fresh {
    transform-box: fill-box;
    transform-origin: center;
    animation-name: pop;
    animation-timing-function: var(--ease-spring);
  }
  .sun.fresh {
    animation-duration: 700ms;
    animation-name: rise;
    animation-timing-function: var(--ease-out);
  }
  @keyframes draw {
    from {
      stroke-dashoffset: 1;
      opacity: 0;
    }
    2% {
      opacity: 1;
    }
    to {
      stroke-dashoffset: 0;
      opacity: 1;
    }
  }
  @keyframes pop {
    from {
      transform: scale(0);
    }
    to {
      transform: scale(1);
    }
  }
  @keyframes rise {
    from {
      transform: scale(0.6);
      opacity: 0;
    }
    to {
      transform: scale(1);
      opacity: 1;
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
  .mount .plant {
    animation: plant-in var(--t-slow) var(--ease-spring);
  }
  @keyframes plant-in {
    from {
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
