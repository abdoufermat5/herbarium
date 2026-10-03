<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import type { DropdownMenuItem } from "./DropdownMenu.svelte";

  /** A menu opened at the pointer (right click) or the focused element (keyboard). */
  let {
    x,
    y,
    items,
    ariaLabel,
    onclose,
  }: {
    x: number;
    y: number;
    items: DropdownMenuItem[];
    ariaLabel: string;
    onclose: () => void;
  } = $props();

  const EDGE = 8;

  let menuEl: HTMLDivElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);
  let highlightedIndex = $state(-1);
  const returnFocus = document.activeElement as HTMLElement | null;

  const interactive = $derived(
    items.flatMap((item, idx) => (!item.disabled && !item.divider && !item.header ? [idx] : [])),
  );

  // Keep the whole menu on screen: flip left/up when it would overflow.
  $effect(() => {
    const el = menuEl;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    left = x + width + EDGE > window.innerWidth ? Math.max(EDGE, x - width) : x;
    top = y + height + EDGE > window.innerHeight ? Math.max(EDGE, y - height) : y;
  });

  function close(restoreFocus = true) {
    onclose();
    if (restoreFocus && returnFocus?.isConnected) returnFocus.focus();
  }

  function select(item: DropdownMenuItem) {
    if (item.disabled || item.divider || item.header) return;
    close();
    item.onclick?.();
  }

  function step(delta: number) {
    if (interactive.length === 0) return;
    const pos = interactive.indexOf(highlightedIndex);
    const next = pos === -1 ? (delta > 0 ? 0 : interactive.length - 1) : (pos + delta + interactive.length) % interactive.length;
    highlightedIndex = interactive[next];
  }

  function onKeyDown(e: KeyboardEvent) {
    // The menu owns these keys; app-level shortcuts (Escape clears filters) must not see them.
    e.stopPropagation();
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        close();
        return;
      case "Tab":
        e.preventDefault();
        close();
        return;
      case "ArrowDown":
        e.preventDefault();
        step(1);
        return;
      case "ArrowUp":
        e.preventDefault();
        step(-1);
        return;
      case "Home":
        e.preventDefault();
        highlightedIndex = interactive[0] ?? -1;
        return;
      case "End":
        e.preventDefault();
        highlightedIndex = interactive.at(-1) ?? -1;
        return;
      case "Enter":
      case " ":
        e.preventDefault();
        if (highlightedIndex >= 0) select(items[highlightedIndex]);
        return;
    }
  }

  function onPointerDown(e: PointerEvent) {
    if (menuEl && !menuEl.contains(e.target as Node)) close(false);
  }

  function dismiss() {
    close(false);
  }

  onMount(() => {
    void tick().then(() => menuEl?.focus());
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("blur", dismiss);
    window.addEventListener("resize", dismiss);
    window.addEventListener("scroll", dismiss, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("blur", dismiss);
      window.removeEventListener("resize", dismiss);
      window.removeEventListener("scroll", dismiss, true);
    };
  });
</script>

<div
  class="dropdown-menu context-menu"
  role="menu"
  aria-label={ariaLabel}
  aria-orientation="vertical"
  tabindex="-1"
  style:left="{left}px"
  style:top="{top}px"
  bind:this={menuEl}
  onkeydown={onKeyDown}
  oncontextmenu={(e) => e.preventDefault()}
>
  {#each items as item, idx (item.id || item.label || idx)}
    {#if item.divider}
      <div class="dropdown-divider" role="separator"></div>
    {:else if item.header}
      <div class="dropdown-header">{item.label}</div>
    {:else}
      <button
        type="button"
        role="menuitem"
        tabindex="-1"
        class="dropdown-item"
        class:highlighted={idx === highlightedIndex}
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={() => select(item)}
        onmouseenter={() => (highlightedIndex = idx)}
      >
        {#if item.icon}
          <span class="menu-icon" aria-hidden="true"><Icon name={item.icon} size={14} /></span>
        {/if}
        <span class="dropdown-item-label">{item.label}</span>
        {#if item.shortcut}
          <span class="menu-shortcut">{item.shortcut}</span>
        {/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .context-menu {
    position: fixed;
    min-width: 184px;
    max-height: none;
    outline: none;
  }

  .menu-icon {
    flex: none;
    display: inline-flex;
    align-items: center;
    color: var(--muted);
  }

  .dropdown-item:hover .menu-icon,
  .dropdown-item.highlighted .menu-icon {
    color: currentColor;
  }

  .menu-shortcut {
    margin-left: auto;
    padding-left: 14px;
    font-family: var(--mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }


  .dropdown-item:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .dropdown-item.danger,
  .dropdown-item.danger .menu-icon {
    color: var(--danger);
  }

  .dropdown-item.danger:hover,
  .dropdown-item.danger.highlighted {
    background: var(--danger-soft);
    color: var(--danger);
  }
</style>
