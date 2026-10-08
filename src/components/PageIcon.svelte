<script lang="ts">
  import type { PageMeta } from "../lib/types";
  import Icon from "../lib/Icon.svelte";

  /** A page's emoji icon, or the plain page glyph. */
  let { page, size = 14 }: { page: Pick<PageMeta, "ext">; size?: number } = $props();
  const icon = $derived(page.ext?.look?.icon);
</script>

{#if icon}
  <span class="emoji" style={`font-size:${size}px`} aria-hidden="true">{icon}</span>
{:else}
  <span class="glyph" aria-hidden="true"><Icon name="file-text" {size} /></span>
{/if}

<style>
  .emoji {
    flex: none;
    line-height: 1;
    width: 1.15em;
    text-align: center;
  }
  .glyph {
    flex: none;
    display: inline-flex;
    color: var(--muted);
  }
</style>
