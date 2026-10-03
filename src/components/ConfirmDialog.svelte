<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import { settleConfirm, type ConfirmOptions } from "../lib/confirm.svelte";

  let { options }: { options: ConfirmOptions } = $props();

  let dialogEl: HTMLDivElement | undefined = $state();
  let cancelBtn: HTMLButtonElement | undefined = $state();
  const prevFocus = document.activeElement as HTMLElement | null;

  function answer(ok: boolean) {
    settleConfirm(ok);
  }

  // Capture phase: the dialog owns Escape/Tab before App's shortcuts see them.
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      answer(false);
      return;
    }
    if (e.key !== "Tab" || !dialogEl) return;
    const buttons = Array.from(dialogEl.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
    if (buttons.length === 0) return;
    const first = buttons[0];
    const last = buttons[buttons.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || !dialogEl.contains(active))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !dialogEl.contains(active))) {
      e.preventDefault();
      first.focus();
    }
  }

  onMount(() => {
    // Cancel is the default: Enter right after a right-click must not destroy anything.
    cancelBtn?.focus();
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (prevFocus?.isConnected) prevFocus.focus();
    };
  });
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && answer(false)}>
  <div
    class="dialog card"
    class:danger={options.danger}
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="confirm-title"
    aria-describedby="confirm-message"
    tabindex="-1"
    bind:this={dialogEl}
  >
    <div class="head">
      {#if options.danger}
        <span class="badge-icon" aria-hidden="true"><Icon name="trash-2" size={18} /></span>
      {/if}
      <h2 id="confirm-title" class="display">{options.title}</h2>
    </div>

    {#if options.subject}
      <div class="subject">
        <span class="subject-icon" aria-hidden="true"><Icon name={options.subject.icon} size={15} /></span>
        <div class="subject-text">
          <strong class="ellipsis" title={options.subject.label}>{options.subject.label}</strong>
          {#if options.subject.meta}
            <span class="subject-meta ellipsis" title={options.subject.meta}>
              <Icon name="folder" size={11} />
              {options.subject.meta}
            </span>
          {/if}
        </div>
      </div>
    {/if}

    <p id="confirm-message" class="message">{options.message}</p>

    <footer class="actions">
      <button class="btn" bind:this={cancelBtn} onclick={() => answer(false)}>{t("common.cancel")}</button>
      <button class="btn" class:btn-primary={!options.danger} class:btn-destroy={options.danger} onclick={() => answer(true)}>
        {options.confirmLabel}
      </button>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 18vh 24px 24px;
    background: var(--scrim);
    animation: confirm-fade var(--t-fast) var(--ease-out);
  }

  .dialog {
    width: 420px;
    max-width: 100%;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 22px 24px 20px;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    outline: none;
    animation: confirm-pop var(--t-med) var(--ease-out);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .badge-icon {
    flex: none;
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    background: var(--danger-soft);
    color: var(--danger);
  }

  h2 {
    font-size: var(--fs-xl);
  }

  .subject {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--sunken);
    min-width: 0;
  }

  .subject-icon {
    flex: none;
    display: grid;
    place-items: center;
    color: var(--muted);
  }

  .subject-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .subject-text strong {
    font-weight: 500;
    color: var(--text);
  }

  .subject-meta {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .message {
    font-size: var(--fs-sm);
    line-height: 1.55;
    color: var(--text-soft);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .btn-destroy {
    background: var(--danger-fill);
    border-color: var(--danger-fill);
    color: var(--on-danger);
  }

  .btn-destroy:hover:not(:disabled) {
    background: var(--danger-fill);
    border-color: var(--danger-fill);
    filter: brightness(1.08);
  }

  .btn-destroy:focus-visible {
    box-shadow: 0 0 0 3px var(--danger-ring);
  }

  @keyframes confirm-fade {
    from {
      opacity: 0;
    }
  }

  @keyframes confirm-pop {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .overlay,
    .dialog {
      animation: none;
    }
  }
</style>
