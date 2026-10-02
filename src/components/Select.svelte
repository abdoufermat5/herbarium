<script module lang="ts">
  import type { IconName } from "../lib/icons";

  export interface SelectOption<V = string | number> {
    value: V;
    label: string;
    icon?: IconName;
    hint?: string;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="T extends string | number">
  import { onMount, tick } from "svelte";
  import Icon from "../lib/Icon.svelte";
  let {
    value,
    options,
    onchange,
    placeholder = "",
    ariaLabel,
    icon,
    size = "sm",
    align = "left",
    disabled = false,
    class: className = "",
  }: {
    value: T;
    options: SelectOption<T>[];
    onchange?: (val: T) => void;
    placeholder?: string;
    ariaLabel?: string;
    icon?: IconName;
    size?: "sm" | "md";
    align?: "left" | "right";
    disabled?: boolean;
    class?: string;
  } = $props();

  let open = $state(false);
  let dropUp = $state(false);
  let highlightedIndex = $state(0);
  let triggerEl: HTMLButtonElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();
  let rootEl: HTMLDivElement | undefined = $state();

  const id = `select-${Math.random().toString(36).slice(2, 9)}`;
  const listId = `${id}-list`;

  const selectedOption = $derived(options.find((o) => o.value === value));
  const currentLabel = $derived(selectedOption ? selectedOption.label : placeholder);

  function updateHighlighted() {
    const idx = options.findIndex((o) => o.value === value);
    highlightedIndex = idx >= 0 ? idx : 0;
  }

  function checkPlacement() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const spaceBelow = window.innerHeight - rect.bottom;
    const spaceAbove = rect.top;
    dropUp = spaceBelow < 220 && spaceAbove > spaceBelow;
  }

  function toggleOpen() {
    if (disabled) return;
    if (open) {
      closeMenu();
    } else {
      openMenu();
    }
  }

  function openMenu() {
    checkPlacement();
    updateHighlighted();
    open = true;
    void tick().then(() => {
      scrollToHighlighted();
      menuEl?.focus();
    });
  }

  function closeMenu(refocus = true) {
    open = false;
    if (refocus && triggerEl && document.activeElement === menuEl) {
      triggerEl.focus();
    }
  }

  function scrollToHighlighted() {
    if (!menuEl) return;
    const items = menuEl.querySelectorAll<HTMLElement>(".select-opt");
    const active = items[highlightedIndex];
    if (active) {
      active.scrollIntoView({ block: "nearest" });
    }
  }

  function selectOption(opt: SelectOption<T>) {
    if (opt.disabled) return;
    if (opt.value !== value) {
      onchange?.(opt.value);
    }
    closeMenu();
  }

  function onTriggerKeyDown(e: KeyboardEvent) {
    if (disabled) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      openMenu();
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      toggleOpen();
    }
  }

  let typeTimer: ReturnType<typeof setTimeout> | undefined;
  let typeBuffer = "";

  function handleTypeAhead(char: string) {
    clearTimeout(typeTimer);
    typeBuffer += char.toLowerCase();
    typeTimer = setTimeout(() => {
      typeBuffer = "";
    }, 500);

    const matchIdx = options.findIndex(
      (o, i) => !o.disabled && i >= highlightedIndex && o.label.toLowerCase().startsWith(typeBuffer),
    );
    const wrapIdx =
      matchIdx !== -1
        ? matchIdx
        : options.findIndex(
            (o) => !o.disabled && o.label.toLowerCase().startsWith(typeBuffer),
          );

    if (wrapIdx !== -1) {
      highlightedIndex = wrapIdx;
      scrollToHighlighted();
    }
  }

  function onMenuKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeMenu();
      return;
    }

    if (e.key === "Tab") {
      closeMenu(false);
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      let next = highlightedIndex + 1;
      while (next < options.length && options[next]?.disabled) next++;
      if (next < options.length) {
        highlightedIndex = next;
        scrollToHighlighted();
      } else {
        // wrap to top
        let first = 0;
        while (first < options.length && options[first]?.disabled) first++;
        if (first < options.length) {
          highlightedIndex = first;
          scrollToHighlighted();
        }
      }
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      let prev = highlightedIndex - 1;
      while (prev >= 0 && options[prev]?.disabled) prev--;
      if (prev >= 0) {
        highlightedIndex = prev;
        scrollToHighlighted();
      } else {
        // wrap to bottom
        let last = options.length - 1;
        while (last >= 0 && options[last]?.disabled) last--;
        if (last >= 0) {
          highlightedIndex = last;
          scrollToHighlighted();
        }
      }
      return;
    }

    if (e.key === "Home") {
      e.preventDefault();
      let first = 0;
      while (first < options.length && options[first]?.disabled) first++;
      if (first < options.length) {
        highlightedIndex = first;
        scrollToHighlighted();
      }
      return;
    }

    if (e.key === "End") {
      e.preventDefault();
      let last = options.length - 1;
      while (last >= 0 && options[last]?.disabled) last--;
      if (last >= 0) {
        highlightedIndex = last;
        scrollToHighlighted();
      }
      return;
    }

    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      const current = options[highlightedIndex];
      if (current && !current.disabled) {
        selectOption(current);
      }
      return;
    }

    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      handleTypeAhead(e.key);
    }
  }

  function onPointerDownOutside(e: PointerEvent) {
    if (!open) return;
    const target = e.target as Node | null;
    if (rootEl && target && !rootEl.contains(target)) {
      closeMenu(false);
    }
  }

  onMount(() => {
    window.addEventListener("pointerdown", onPointerDownOutside, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDownOutside, true);
    };
  });
</script>

<div
  class="select-root {className}"
  class:open
  class:disabled
  bind:this={rootEl}
>
  <button
    type="button"
    class="select-btn select-{size}"
    class:active={open}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={open ? listId : undefined}
    aria-label={ariaLabel}
    {disabled}
    bind:this={triggerEl}
    onclick={toggleOpen}
    onkeydown={onTriggerKeyDown}
  >
    {#if icon}
      <span class="leading-icon" aria-hidden="true">
        <Icon name={icon} size={size === "sm" ? 13 : 15} />
      </span>
    {:else if selectedOption?.icon}
      <span class="leading-icon" aria-hidden="true">
        <Icon name={selectedOption.icon} size={size === "sm" ? 13 : 15} />
      </span>
    {/if}

    <span class="select-label ellipsis">{currentLabel}</span>

    <span class="chevron" class:flipped={open} aria-hidden="true">
      <Icon name="chevron-down" size={size === "sm" ? 13 : 14} />
    </span>
  </button>

  {#if open}
    <div
      class="select-menu"
      class:align-right={align === "right"}
      class:drop-up={dropUp}
      role="listbox"
      id={listId}
      aria-label={ariaLabel || currentLabel}
      tabindex="-1"
      bind:this={menuEl}
      onkeydown={onMenuKeyDown}
    >
      {#each options as opt, idx (opt.value)}
        {@const isSelected = opt.value === value}
        {@const isHighlighted = idx === highlightedIndex}
        <div
          role="option"
          id="{id}-opt-{idx}"
          class="select-opt"
          class:selected={isSelected}
          class:highlighted={isHighlighted}
          class:disabled={opt.disabled}
          aria-selected={isSelected}
          aria-disabled={opt.disabled}
          tabindex="-1"
          onmouseenter={() => {
            if (!opt.disabled) highlightedIndex = idx;
          }}
          onclick={() => selectOption(opt)}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              selectOption(opt);
            }
          }}
        >
          {#if opt.icon}
            <span class="opt-icon" aria-hidden="true">
              <Icon name={opt.icon} size={14} />
            </span>
          {/if}

          <div class="opt-content">
            <span class="opt-label">{opt.label}</span>
            {#if opt.hint}
              <span class="opt-hint">{opt.hint}</span>
            {/if}
          </div>

          {#if isSelected}
            <span class="opt-check" aria-hidden="true">
              <Icon name="check" size={13} stroke={2.2} />
            </span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .select-root {
    position: relative;
    display: inline-flex;
    vertical-align: middle;
  }

  .select-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-family: inherit;
    font-weight: 500;
    line-height: 1.3;
    white-space: nowrap;
    user-select: none;
    box-shadow: var(--shadow-sm);
    transition:
      border-color var(--t-fast) var(--ease-out),
      background-color var(--t-fast) var(--ease-out),
      box-shadow var(--t-fast) var(--ease-out);
  }

  .select-btn:hover:not(:disabled) {
    border-color: var(--border-hover);
    background: var(--raised);
  }

  .select-btn:focus-visible,
  .select-btn.active {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-ring), var(--shadow-sm);
    background: var(--surface);
  }

  .select-btn:disabled {
    cursor: not-allowed;
    opacity: 0.55;
    background: var(--sunken);
  }

  .select-sm {
    padding: 5px 9px 5px 9px;
    font-size: 13px;
    height: 32px;
  }

  .select-md {
    padding: 7px 12px 7px 11px;
    font-size: 13.5px;
    height: 36px;
  }

  .leading-icon {
    flex: none;
    display: inline-flex;
    align-items: center;
    color: var(--muted);
    transition: color var(--t-fast) var(--ease-out);
  }

  .select-btn:hover:not(:disabled) .leading-icon,
  .select-btn.active .leading-icon {
    color: var(--accent);
  }

  .select-label {
    min-width: 0;
    font-weight: 500;
    letter-spacing: -0.01em;
  }

  .chevron {
    flex: none;
    display: inline-flex;
    align-items: center;
    color: var(--muted);
    margin-left: 1px;
    transition:
      transform var(--t-med) var(--ease-spring),
      color var(--t-fast) var(--ease-out);
  }

  .select-btn:hover:not(:disabled) .chevron,
  .select-btn.active .chevron {
    color: var(--text-soft);
  }

  .chevron.flipped {
    transform: rotate(180deg);
    color: var(--accent);
  }

  /* Popover Menu */
  .select-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: max(100%, 150px);
    max-height: 250px;
    overflow-y: auto;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-lg);
    padding: 4px;
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 2px;
    outline: none;
    transform-origin: top left;
    animation: select-pop var(--t-fast) var(--ease-out);
  }

  .select-menu.align-right {
    left: auto;
    right: 0;
    transform-origin: top right;
  }

  .select-menu.drop-up {
    top: auto;
    bottom: calc(100% + 4px);
    transform-origin: bottom left;
  }

  .select-menu.drop-up.align-right {
    transform-origin: bottom right;
  }

  @keyframes select-pop {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .select-opt {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 9px;
    border-radius: var(--radius-xs);
    cursor: pointer;
    color: var(--text-soft);
    font-size: 13px;
    font-weight: 500;
    user-select: none;
    outline: none;
    transition:
      background-color var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }

  .select-opt:hover,
  .select-opt.highlighted {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }

  .select-opt.selected {
    color: var(--accent-strong);
    font-weight: 600;
  }

  .select-opt.disabled {
    opacity: 0.4;
    cursor: not-allowed;
    pointer-events: none;
  }

  .opt-icon {
    flex: none;
    display: inline-flex;
    align-items: center;
    color: var(--muted);
  }

  .select-opt.selected .opt-icon,
  .select-opt.highlighted .opt-icon {
    color: var(--accent);
  }

  .opt-content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .opt-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .opt-hint {
    font-size: 11px;
    color: var(--muted);
  }

  .opt-check {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
  }
</style>
