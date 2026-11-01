<script lang="ts">
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { app, dismissToast, pauseToasts, resumeToasts, type ToastKind } from "../lib/state.svelte";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import { reducedMotion } from "../lib/motion";
  import type { IconName } from "../lib/icons";

  const ICONS: Record<Exclude<ToastKind, "success">, IconName> = {
    info: "info",
    error: "x",
  };

  /** Vertical gap between cards when the stack is open. */
  const GAP = 8;
  /** Only this many cards show behind the front one while collapsed. */
  const VISIBLE = 3;
  /** Pull this far to the right to dismiss. */
  const SWIPE_AT = 72;

  let hovered = $state(false);
  let focused = $state(false);
  const expanded = $derived(hovered || focused);

  /** Natural height of each card's content, measured so the open stack can lay out exactly. */
  let heights = $state<Record<number, number>>({});

  /** Oldest to newest, so a new card mounts last; `i` counts back from the front card. */
  const layout = $derived.by(() => {
    const n = app.toasts.length;
    const rows = new Array<{ toast: (typeof app.toasts)[number]; i: number; offset: number }>(n);
    let offset = 0;
    for (let idx = n - 1; idx >= 0; idx--) {
      const toast = app.toasts[idx];
      rows[idx] = { toast, i: n - 1 - idx, offset };
      offset += (heights[toast.id] ?? 48) + GAP;
    }
    return rows;
  });

  const frontHeight = $derived(app.toasts.length ? (heights[app.toasts[app.toasts.length - 1].id] ?? 48) : 0);
  const openHeight = $derived(
    app.toasts.reduce((sum, toast) => sum + (heights[toast.id] ?? 48) + GAP, 0),
  );
  // The hover target: it grows with the stack so moving between cards never lets go of it.
  const zoneHeight = $derived(
    app.toasts.length === 0 ? 0 : expanded ? openHeight : frontHeight + Math.min(app.toasts.length - 1, VISIBLE - 1) * 12 + GAP,
  );

  // Swipe to dismiss.
  let drag = $state<{ id: number; dx: number } | null>(null);
  let dragStart = 0;

  function onPointerDown(e: PointerEvent, id: number) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    dragStart = e.clientX;
    drag = { id, dx: 0 };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    drag = { id: drag.id, dx: Math.max(0, e.clientX - dragStart) };
  }

  function onPointerEnd() {
    if (!drag) return;
    const { id, dx } = drag;
    drag = null;
    if (dx > SWIPE_AT) dismissToast(id);
  }

  // Esc dismisses the newest toast, unless something else owns the key.
  function onKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented || app.toasts.length === 0) return;
    if (document.querySelector('[aria-modal="true"], [role="menu"], [role="listbox"]')) return;
    const el = e.target as HTMLElement | null;
    if (el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName))) return;
    e.preventDefault();
    e.stopPropagation();
    dismissToast(app.toasts[app.toasts.length - 1].id);
  }

  function onFocusOut(e: FocusEvent) {
    if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node | null)) focused = false;
  }

  // Hold the countdown while the stack is open, resume when it closes.
  $effect(() => {
    if (expanded) pauseToasts();
    else resumeToasts();
  });

  function leave(node: Element) {
    return fly(node, { x: 56, duration: reducedMotion() ? 0 : 170, opacity: 0, easing: cubicOut });
  }
</script>

<svelte:window onkeydowncapture={onKeydown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="toasts"
  class:expanded
  style:height="{zoneHeight}px"
  onpointerenter={() => (hovered = true)}
  onpointerleave={() => (hovered = false)}
  onfocusin={() => (focused = true)}
  onfocusout={onFocusOut}
>
  <div class="live" role="status" aria-live="polite">
    {#each layout as { toast, i, offset } (toast.id)}
      {@const dx = drag?.id === toast.id ? drag.dx : 0}
      <div
        class="toast {toast.kind}"
        class:front={i === 0}
        class:hidden={i >= VISIBLE}
        class:dragging={drag?.id === toast.id}
        style:--i={i}
        style:--offset="{offset}px"
        style:--front-h="{frontHeight}px"
        style:--dx="{dx}px"
        style:--fade={1 - Math.min(dx / 240, 0.8)}
        onpointerdown={(e) => onPointerDown(e, toast.id)}
        onpointermove={onPointerMove}
        onpointerup={onPointerEnd}
        onpointercancel={onPointerEnd}
        out:leave
      >
        <div class="row" bind:clientHeight={heights[toast.id]}>
          <span class="toast-icon">
            {#if toast.kind === "success"}
              <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <circle class="ring" cx="8" cy="8" r="6.4" pathLength="1" />
                <path class="tick" d="M5.1 8.3l2 2 3.8-4.4" pathLength="1" />
              </svg>
            {:else}
              <Icon name={ICONS[toast.kind]} size={14} />
            {/if}
          </span>
          <span class="toast-msg">{toast.message}</span>
          {#if toast.action}
            {@const action = toast.action}
            <button
              class="toast-action"
              onclick={() => {
                dismissToast(toast.id);
                action.run();
              }}>{action.label}</button
            >
          {/if}
          <button class="toast-x" aria-label={t("toast.dismiss")} title={t("toast.dismiss")} onclick={() => dismissToast(toast.id)}>
            <Icon name="x" size={13} />
          </button>
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .toasts {
    position: fixed;
    right: 14px;
    bottom: 14px;
    z-index: var(--z-toast);
    width: min(360px, calc(100vw - 28px));
  }
  .live {
    position: absolute;
    inset: auto 0 0 0;
  }

  .toast {
    --accent-soft: var(--info-soft);
    --accent-ink: var(--info);
    position: absolute;
    right: 0;
    bottom: 0;
    width: 100%;
    overflow: hidden;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
    touch-action: pan-y;
    user-select: none;
    transform-origin: 50% 100%;
    /* Collapsed: older cards sit behind the newest, smaller and a little higher. */
    transform: translate3d(var(--dx), calc(var(--i) * -12px), 0) scale(calc(1 - var(--i) * 0.05));
    opacity: var(--fade, 1);
    z-index: calc(10 - var(--i));
    transition:
      transform var(--t-enter) var(--ease-spring),
      opacity var(--t-med) var(--ease-out);
    animation: toast-in var(--t-enter) var(--ease-spring) backwards;
  }
  .toast.success {
    --accent-soft: var(--ok-soft);
    --accent-ink: var(--ok);
  }
  .toast.error {
    --accent-soft: var(--danger-soft);
    --accent-ink: var(--danger);
  }
  .toast.dragging {
    transition: none;
    cursor: grabbing;
  }

  /* Open: the stack unfolds upward at full size. */
  .toasts.expanded .toast {
    transform: translate3d(var(--dx), calc(var(--offset) * -1), 0);
    transition:
      transform var(--t-enter) var(--ease-spring),
      opacity var(--t-med) var(--ease-out);
  }
  .toasts.expanded .toast.dragging {
    transition: none;
  }

  /* Collapsed back cards are only a edge: same height as the front one, content hidden. */
  .toasts:not(.expanded) .toast:not(.front) {
    height: var(--front-h);
  }
  .toasts:not(.expanded) .toast:not(.front) .row {
    opacity: 0;
  }
  .toasts:not(.expanded) .toast.hidden {
    opacity: 0;
    pointer-events: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 11px 12px 11px 11px;
    transition: opacity var(--t-fast) var(--ease-out);
  }

  .toast-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
  .toast-msg {
    flex: 1;
    font-size: var(--fs-sm);
    line-height: 1.4;
    color: var(--text-soft);
    overflow-wrap: anywhere;
  }
  .toast-action {
    flex: none;
    padding: 4px 9px;
    border-radius: var(--radius-xs);
    border: 1px solid var(--border-strong);
    font-size: var(--fs-sm);
    color: var(--text);
    transition: background-color var(--t-fast) var(--ease-out);
  }
  .toast-action:hover {
    background: var(--sunken);
  }
  .toast-x {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-xs);
    color: var(--muted);
    transition:
      background-color var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }
  .toast-x:hover {
    background: var(--sunken);
    color: var(--text);
  }

  /* The success mark draws itself: ring first, then the tick. */
  .ring,
  .tick {
    stroke-dasharray: 1;
    stroke-dashoffset: 0;
  }
  .ring {
    animation: draw 380ms var(--ease-out) backwards;
  }
  .tick {
    animation: draw 260ms var(--ease-out) 220ms backwards;
  }

  @keyframes draw {
    from {
      stroke-dashoffset: 1;
    }
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translate3d(0, calc(100% + 16px), 0);
    }
  }
</style>
