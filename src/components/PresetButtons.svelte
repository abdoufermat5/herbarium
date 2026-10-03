<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { t } from "../lib/i18n.svelte";
  import { fmtDuration, fmtDurationShort } from "../lib/format";

  // The vault's preset review intervals as quick-pick buttons.
  let { onpick, disabled = false }: { onpick: (minutes: number) => void; disabled?: boolean } = $props();
</script>

{#each app.review.presets as minutes (minutes)}
  <button
    class="btn btn-xs"
    title={t("review.againIn", { when: fmtDuration(minutes) })}
    {disabled}
    onclick={() => onpick(minutes)}
  >
    {fmtDurationShort(minutes)}
  </button>
{/each}
