<script lang="ts">
  // The Herbarium mark, drawn from docs/brand/icon-*.svg. Its colours come
  // from the --logo-* tokens in app.css, so it follows the light or dark
  // theme. Below 48 px it uses the simplified leaf, like the PNG icons.
  let { size = 28 }: { size?: number } = $props();

  const uid = $props.id();
  const full = $derived(size >= 48);
  const blade = "M512 150C690 250 770 450 712 630C668 770 584 832 512 842C440 832 356 770 312 630C254 450 334 250 512 150Z";
  const veins = $derived(
    (full ? [320, 430, 540, 650] : [380, 590]).flatMap((y) => {
      const [reach, rise] = full ? [168, 105] : [175, 95];
      return [1, -1].map((s) => `M512 ${y + 40}Q${512 + s * 70} ${y + 30} ${512 + s * reach} ${y + 40 - rise}`);
    }),
  );
</script>

<svg class="logo" width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
  <defs>
    <linearGradient id="{uid}-bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" style="stop-color: var(--logo-bg-top)" />
      <stop offset="1" style="stop-color: var(--logo-bg-bottom)" />
    </linearGradient>
    <linearGradient id="{uid}-leaf" x1="1" y1="0" x2="0" y2="1">
      <stop offset="0" style="stop-color: var(--logo-leaf-light)" />
      <stop offset="1" style="stop-color: var(--logo-leaf-deep)" />
    </linearGradient>
    <clipPath id="{uid}-blade"><path d={blade} /></clipPath>
  </defs>
  <rect width="1024" height="1024" rx="228" fill="url(#{uid}-bg)" />
  <!-- About one pixel at sidebar size: the light tile would melt into cream surfaces. -->
  <rect x="18" y="18" width="988" height="988" rx="212" fill="none" stroke="var(--logo-edge)" stroke-width="36" />
  <g transform="translate(500 532) rotate(45) scale(1.06) translate(-512 -512)">
    <path d={blade} fill="url(#{uid}-leaf)" />
    <g clip-path="url(#{uid}-blade)" fill="none" stroke="var(--logo-vein)" stroke-width={full ? 15 : 34} stroke-linecap="round">
      {#each veins as d (d)}<path {d} />{/each}
    </g>
    <path d={full ? "M512 205V930" : "M512 240V905"} stroke="var(--logo-vein)" stroke-width={full ? 22 : 44} stroke-linecap="round" />
  </g>
  {#if full}
    <path d="M246 818l32 21-32 21" fill="none" stroke="var(--logo-chevron)" stroke-width="15" stroke-linecap="round" stroke-linejoin="round" />
  {/if}
</svg>

<style>
  .logo {
    display: block;
    flex: none;
  }
</style>
