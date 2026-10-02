<script lang="ts">
  import { icons, type IconName } from "./icons";

  let {
    name,
    size = 16,
    stroke = 1.8,
    class: className = "",
  }: { name: IconName; size?: number; stroke?: number; class?: string } = $props();
</script>

<svg
  class={className}
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false">{@html icons[name]}</svg
>

<style>
  svg {
    display: block;
    flex: none;
  }
</style>
