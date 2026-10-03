<script lang="ts">
  import { onMount, tick, type Snippet } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import type { IconName } from "../lib/icons";

  export interface DropdownMenuItem {
    id?: string;
    label?: string;
    icon?: IconName;
    shortcut?: string;
    danger?: boolean;
    disabled?: boolean;
    checked?: boolean;
    divider?: boolean;
    header?: boolean;
    onclick?: () => void;
  }

  let {
    items = [],
    align = "left",
    ariaLabel = "Menu",
    class: className = "",
    trigger,
  }: {
    items: DropdownMenuItem[];
    align?: "left" | "right";
    ariaLabel?: string;
    class?: string;
    trigger: Snippet<[{ open: boolean; toggle: () => void }]>;
  } = $props();

  let open = $state(false);
  let dropUp = $state(false);
  let highlightedIndex = $state(-1);
  let triggerWrapEl: HTMLDivElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();

  const id = `dropdown-${Math.random().toString(36).slice(2, 9)}`;

  function checkPlacement() {
    if (!triggerWrapEl) return;
    const rect = triggerWrapEl.getBoundingClientRect();
    const spaceBelow = window.innerHeight - rect.bottom;
    const spaceAbove = rect.top;
    dropUp = spaceBelow < 220 && spaceAbove > spaceBelow;
  }

  export function toggle() {
    if (open) {
      closeMenu();
    } else {
      openMenu();
    }
  }

  export function openMenu() {
    checkPlacement();
    highlightedIndex = -1;
    open = true;
    void tick().then(() => {
      menuEl?.focus();
    });
  }

  export function closeMenu() {
    open = false;
    highlightedIndex = -1;
    // return focus to trigger button inside wrapper
    const btn = triggerWrapEl?.querySelector<HTMLButtonElement>("button");
    btn?.focus();
  }

  function handleSelect(item: DropdownMenuItem) {
    if (item.disabled || item.divider || item.header) return;
    item.onclick?.();
    closeMenu();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeMenu();
      return;
    }
    if (e.key === "Tab") {
      closeMenu();
      return;
    }

    const interactiveIndices = items
      .map((item, idx) => (!item.disabled && !item.divider && !item.header ? idx : -1))
      .filter((idx) => idx !== -1);

    if (interactiveIndices.length === 0) return;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      const currentPos = interactiveIndices.indexOf(highlightedIndex);
      if (currentPos === -1 || currentPos === interactiveIndices.length - 1) {
        highlightedIndex = interactiveIndices[0];
      } else {
        highlightedIndex = interactiveIndices[currentPos + 1];
      }
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      const currentPos = interactiveIndices.indexOf(highlightedIndex);
      if (currentPos <= 0) {
        highlightedIndex = interactiveIndices[interactiveIndices.length - 1];
      } else {
        highlightedIndex = interactiveIndices[currentPos - 1];
      }
      return;
    }

    if (e.key === "Home") {
      e.preventDefault();
      highlightedIndex = interactiveIndices[0];
      return;
    }

    if (e.key === "End") {
      e.preventDefault();
      highlightedIndex = interactiveIndices[interactiveIndices.length - 1];
      return;
    }

    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (highlightedIndex >= 0 && highlightedIndex < items.length) {
        handleSelect(items[highlightedIndex]);
      }
      return;
    }
  }

  function onPointerDownOutside(e: PointerEvent) {
    if (!open) return;
    const target = e.target as Node | null;
    if (triggerWrapEl && target && !triggerWrapEl.contains(target)) {
      closeMenu();
    }
  }

  onMount(() => {
    window.addEventListener("pointerdown", onPointerDownOutside, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDownOutside, true);
    };
  });
</script>

<div class="dropdown-root {className}" bind:this={triggerWrapEl}>
  {@render trigger({ open, toggle })}

  {#if open}
    <div
      class="dropdown-menu"
      class:align-right={align === "right"}
      class:drop-up={dropUp}
      role="menu"
      id={id}
      aria-label={ariaLabel}
      tabindex="-1"
      bind:this={menuEl}
      onkeydown={onKeyDown}
    >
      {#each items as item, idx (item.id || item.label || idx)}
        {#if item.divider}
          <div class="dropdown-divider" role="separator"></div>
        {:else if item.header}
          <div class="dropdown-header">{item.label}</div>
        {:else}
          {@const isHighlighted = idx === highlightedIndex}
          <button
            type="button"
            role="menuitem"
            class="dropdown-item"
            class:highlighted={isHighlighted}
            class:danger={item.danger}
            class:active={item.checked}
            disabled={item.disabled}
            onclick={() => handleSelect(item)}
            onmouseenter={() => (highlightedIndex = idx)}
          >
            {#if item.icon}
              <span class="dropdown-icon" aria-hidden="true">
                <Icon name={item.icon} size={14} />
              </span>
            {/if}

            <span class="dropdown-item-label">{item.label}</span>

            {#if item.shortcut}
              <span class="dropdown-shortcut">{item.shortcut}</span>
            {/if}

            {#if item.checked}
              <span class="dropdown-item-check" aria-hidden="true">
                <Icon name="check" size={13} />
              </span>
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .dropdown-root {
    position: relative;
    display: inline-flex;
    vertical-align: middle;
  }

  .dropdown-icon {
    flex: none;
    display: inline-flex;
    align-items: center;
    color: var(--muted);
  }

  .dropdown-item:hover .dropdown-icon,
  .dropdown-item.highlighted .dropdown-icon {
    color: currentColor;
  }

  .dropdown-item.danger {
    color: var(--danger);
  }

  .dropdown-item.danger:hover,
  .dropdown-item.danger.highlighted {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .dropdown-shortcut {
    font-size: 11px;
    color: var(--muted);
    margin-left: 12px;
    letter-spacing: 0.02em;
  }
</style>
