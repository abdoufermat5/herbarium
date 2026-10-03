<script lang="ts">
  import { app, dismissToast, type ToastKind } from "../lib/state.svelte";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import type { IconName } from "../lib/icons";

  const ICONS: Record<ToastKind, IconName> = {
    info: "info",
    success: "circle-check",
    error: "x",
  };
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast {toast.kind}">
      <span class="toast-icon"><Icon name={ICONS[toast.kind]} size={15} /></span>
      <span class="toast-msg">{toast.message}</span>
      <button class="toast-x" aria-label={t("toast.dismiss")} onclick={() => dismissToast(toast.id)}>
        <Icon name="x" size={13} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 14px;
    bottom: 14px;
    z-index: 70;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(360px, calc(100vw - 28px));
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-lg);
    animation: toast-in var(--t-slow) var(--ease-out);
  }
  .toast-icon {
    display: grid;
    place-items: center;
    flex: none;
    color: var(--info);
  }
  .toast.success .toast-icon {
    color: var(--ok);
  }
  .toast.error .toast-icon {
    color: var(--danger);
  }
  .toast-msg {
    flex: 1;
    font-size: 13px;
    color: var(--text-soft);
    overflow-wrap: anywhere;
  }
  .toast-x {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: var(--radius-xs);
    color: var(--muted);
  }
  .toast-x:hover {
    background: var(--sunken);
    color: var(--text);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(12px);
    }
  }
</style>
