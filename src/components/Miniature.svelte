<script lang="ts">
  import type { PreviewDigest } from "../lib/types";

  /**
   * A page's first screen drawn from its layout digest: background boxes,
   * text as lines, media as panels, and the one sampled image. Cropped to
   * the card's aspect from the top.
   */
  let { digest, aspect = 0.62 }: { digest: PreviewDigest; aspect?: number } = $props();

  const w = $derived(digest.w);
  const h = $derived(Math.min(digest.h, digest.w * aspect));

  function lines(b: PreviewDigest["blocks"][number]) {
    const size = b.s ?? 14;
    const lh = Math.max(size, b.lh ?? size * 1.4);
    const count = Math.max(1, Math.min(12, Math.round(b.h / lh)));
    const bar = Math.max(2, size * 0.5);
    return Array.from({ length: count }, (_, i) => ({
      y: b.y + i * lh + (lh - bar) / 2,
      w: count > 1 && i === count - 1 ? b.w * 0.6 : b.w,
      h: bar,
    }));
  }
</script>

<svg class="mini" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="xMidYMin slice" aria-hidden="true">
  <rect x="0" y="0" width={w} height={h} fill={digest.bg} />
  {#each digest.blocks as b, i (i)}
    {#if b.y < h}
      {#if b.k === "box"}
        <rect x={b.x} y={b.y} width={b.w} height={b.h} rx={b.r ?? 0} fill={b.c} />
      {:else if b.k === "img"}
        <rect x={b.x} y={b.y} width={b.w} height={b.h} rx={b.r ?? 0} class="media" />
      {:else}
        {#each lines(b) as l, j (j)}
          <rect x={b.x} y={l.y} width={l.w} height={l.h} rx={l.h / 2} fill={b.c} opacity="0.55" />
        {/each}
      {/if}
    {/if}
  {/each}
  {#if digest.image}
    <image href={digest.image.src} x={digest.image.x} y={digest.image.y} width={digest.image.w} height={digest.image.h} preserveAspectRatio="xMidYMid slice" />
  {/if}
</svg>

<style>
  .mini {
    display: block;
    width: 100%;
    height: 100%;
  }
  .media {
    fill: currentColor;
    opacity: 0.12;
  }
</style>
