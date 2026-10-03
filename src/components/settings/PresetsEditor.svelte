<script lang="ts">
  import { app, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { fmtDuration, UNIT_MINUTES, type DurationUnit } from "../../lib/format";
  import Icon from "../../lib/Icon.svelte";
  import DurationField from "./DurationField.svelte";

  let {
    disabled = false,
    onchange,
  }: { disabled?: boolean; onchange: (presets: number[]) => boolean | Promise<boolean> } = $props();

  let amount = $state("");
  let unit = $state<DurationUnit>("day");

  async function add() {
    const n = Number(amount);
    if (!Number.isInteger(n) || n < 1) {
      toast(t("review.cfg.intervalInvalid"), "error");
      return;
    }
    const minutes = n * UNIT_MINUTES[unit];
    if (app.review.presets.includes(minutes) || (await onchange([...app.review.presets, minutes]))) {
      amount = "";
    }
  }

  function remove(minutes: number) {
    if (app.review.presets.length > 1) void onchange(app.review.presets.filter((m) => m !== minutes));
  }
</script>

<div class="presets">
  <ul class="chips" aria-label={t("review.cfg.intervals")}>
    {#each app.review.presets as minutes (minutes)}
      <li class="chip-item">
        <span class="chip-label">{fmtDuration(minutes)}</span>
        <button
          type="button"
          class="chip-x"
          aria-label={t("review.cfg.removeInterval", { when: fmtDuration(minutes) })}
          disabled={disabled || app.review.presets.length <= 1}
          onclick={() => remove(minutes)}
        >
          <Icon name="x" size={11} />
        </button>
      </li>
    {/each}
  </ul>

  <form
    class="add"
    onsubmit={(e) => {
      e.preventDefault();
      void add();
    }}
  >
    <div class="add-field">
      <DurationField bind:amount bind:unit label={t("review.cfg.amount")} {disabled} />
    </div>
    <button type="submit" class="btn" disabled={disabled || amount.trim() === ""}>
      <Icon name="plus" size={13} />{t("review.cfg.addInterval")}
    </button>
  </form>
</div>

<style>
  .presets {
    display: flex;
    flex-direction: column;
    gap: 14px;
    width: 100%;
    min-width: 0;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .chip-item {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    max-width: 100%;
    padding: 3px 4px 3px 11px;
    background: var(--sunken);
    border: 1px solid var(--border);
    border-radius: 999px;
    font-family: var(--mono);
    font-size: var(--fs-xs);
  }
  .chip-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip-x {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    margin: -2px -2px -2px 0;
    border-radius: 50%;
    color: var(--muted);
  }
  .chip-x:hover:not(:disabled) {
    background: var(--raised);
    color: var(--text);
  }
  .chip-x:disabled {
    opacity: 0.35;
  }
  .add {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .add-field {
    flex: 1 1 220px;
    max-width: 300px;
    min-width: 0;
  }
</style>
