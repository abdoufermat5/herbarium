<script lang="ts">
  import { onDestroy } from "svelte";
  import { app } from "../lib/state.svelte";
  import { t } from "../lib/i18n.svelte";
  import { fmtDuration, fmtDurationShort } from "../lib/format";

  // The vault's preset review intervals as quick-pick buttons. `current` marks
  // the interval the page is on now (when it matches a preset).
  let {
    onpick,
    disabled = false,
    current = null,
  }: { onpick: (minutes: number) => void; disabled?: boolean; current?: number | null } = $props();

  /** The preset just picked: it pops once, springing back. */
  let picked = $state<number | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function pick(minutes: number) {
    picked = minutes;
    clearTimeout(timer);
    timer = setTimeout(() => (picked = null), 380);
    onpick(minutes);
  }

  onDestroy(() => clearTimeout(timer));
</script>

{#each app.review.presets as minutes (minutes)}
  <button
    class="btn btn-xs preset"
    class:current={current === minutes}
    class:picked={picked === minutes}
    aria-pressed={current == null ? undefined : current === minutes}
    title={t("review.againIn", { when: fmtDuration(minutes) })}
    {disabled}
    onclick={() => pick(minutes)}
  >
    {fmtDurationShort(minutes)}
  </button>
{/each}

<style>
  .preset.current {
    background: var(--c-sage-soft);
    border-color: var(--c-sage);
    color: var(--c-sage);
  }
  .preset.picked {
    animation: preset-pop 360ms var(--ease-spring);
  }
  @keyframes preset-pop {
    from {
      transform: scale(0.9);
    }
  }
</style>
