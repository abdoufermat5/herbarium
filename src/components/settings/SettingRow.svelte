<script lang="ts">
  import type { Snippet } from "svelte";

  // One labelled setting. The control sits in a fixed-width column on the
  // right (controls fill it), or under the text when `stacked`; narrow
  // containers always stack so nothing can overflow.
  let {
    title,
    hint,
    stacked = false,
    children,
  }: { title: string; hint?: string; stacked?: boolean; children: Snippet } = $props();
</script>

<div class="row" class:stacked>
  <div class="copy">
    <h3>{title}</h3>
    {#if hint}<p>{hint}</p>{/if}
  </div>
  <div class="control">{@render children()}</div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 220px;
    align-items: center;
    gap: 12px 28px;
    padding: 20px 28px;
  }
  .row.stacked {
    grid-template-columns: minmax(0, 1fr);
    align-items: start;
  }
  .copy {
    min-width: 0;
  }
  h3 {
    font-size: 14px;
    font-weight: 500;
    margin-bottom: 2px;
  }
  p {
    color: var(--muted);
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .control {
    min-width: 0;
  }

  @container settings (max-width: 560px) {
    .row {
      grid-template-columns: minmax(0, 1fr);
      align-items: start;
      padding: 18px 20px;
    }
  }
</style>
