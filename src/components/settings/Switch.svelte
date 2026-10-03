<script lang="ts">
  let {
    checked,
    label,
    disabled = false,
    onchange,
  }: { checked: boolean; label: string; disabled?: boolean; onchange: (checked: boolean) => void } = $props();
</script>

<button
  type="button"
  role="switch"
  class="switch"
  class:on={checked}
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="thumb"></span>
</button>

<style>
  .switch {
    flex: none;
    position: relative;
    width: 38px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--sunken);
    cursor: pointer;
    transition:
      background var(--t-fast) var(--ease-out),
      border-color var(--t-fast) var(--ease-out),
      box-shadow var(--t-fast) var(--ease-out);
  }
  .switch:hover:not(:disabled) {
    border-color: var(--border-hover);
  }
  .switch:focus-visible {
    outline: none;
    box-shadow: 0 0 0 3px var(--accent-ring);
  }
  .switch:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .switch.on {
    background: var(--accent-fill);
    border-color: var(--accent-fill);
  }
  .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--muted);
    transition:
      transform var(--t-fast) var(--ease-out),
      background var(--t-fast) var(--ease-out);
  }
  .on .thumb {
    transform: translateX(16px);
    background: var(--on-accent);
  }
</style>
