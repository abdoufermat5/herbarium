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
    border: 1px solid var(--border-input);
    border-radius: 999px;
    background: var(--sunken);
    cursor: pointer;
    transition:
      background var(--t-fast) var(--ease-out),
      border-color var(--t-fast) var(--ease-out),
      box-shadow var(--t-fast) var(--ease-out);
  }
  .switch:hover:not(:disabled) {
    border-color: var(--border-input-hover);
  }
  .switch:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
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
    transform-origin: left center;
    transition:
      transform var(--t-enter) var(--ease-spring, cubic-bezier(0.34, 1.56, 0.64, 1)),
      background var(--t-fast) var(--ease-out);
  }
  .on .thumb {
    transform: translateX(16px);
    background: var(--on-accent);
    transform-origin: right center;
  }
  /* Pressing stretches the thumb away from the edge it rests against. */
  .switch:active:not(:disabled) .thumb {
    transform: scaleX(1.15);
  }
  .switch:active:not(:disabled).on .thumb {
    transform: translateX(16px) scaleX(1.15);
  }
</style>
