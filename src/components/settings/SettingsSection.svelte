<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    id,
    title,
    note,
    children,
  }: { id: string; title: string; note?: string; children: Snippet } = $props();
</script>

<section aria-labelledby={id}>
  <h2 {id}>{title}</h2>
  <div class="card rows">{@render children()}</div>
  {#if note}<p class="note">{note}</p>{/if}
</section>

<style>
  h2 {
    margin-bottom: 10px;
    font-family: var(--font-display);
    font-variation-settings: var(--font-display-settings);
    font-size-adjust: none;
    font-size: calc(var(--fs-lg) * var(--display-size-adjust));
    font-weight: var(--display-weight);
    letter-spacing: var(--display-tracking);
    line-height: var(--display-leading);
    color: var(--text);
  }
  .rows {
    display: flex;
    flex-direction: column;
    /* clip nothing: dropdown menus are allowed to extend past the card */
    overflow: visible;
  }
  .rows > :global(* + *) {
    border-top: 1px solid var(--border);
  }
  .note {
    margin-top: 10px;
    color: var(--muted);
    font-size: var(--fs-xs);
  }
</style>
