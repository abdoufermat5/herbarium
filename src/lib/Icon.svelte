<script lang="ts">
  import { icons, type IconName } from "./icons";

  let {
    name,
    size = 16,
    class: className = "",
  }: { name: IconName; size?: number; class?: string } = $props();
</script>

<svg
  class={className}
  width={size}
  height={size}
  viewBox="0 0 256 256"
  fill="currentColor"
  aria-hidden="true"
  focusable="false">{@html icons[name]}</svg
>

<style>
  svg {
    display: block;
    flex: none;
  }
</style>
