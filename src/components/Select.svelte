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
    fill = false,
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
    /** Stretch to the width of the container instead of hugging the label. */
    fill?: boolean;
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
  class:fill
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
      <Icon name="chevron-down" size={size === "sm" ? 11 : 12} />
    </span>
  </button>

  {#if open}
    <div
      class="dropdown-menu select-menu"
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
          class="dropdown-item select-opt"
          class:active={isSelected}
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
            <span class="dropdown-item-check" aria-hidden="true">
              <Icon name="check" size={13} />
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

  .select-root.fill {
    display: flex;
    width: 100%;
    min-width: 0;
  }

  .fill .select-btn {
    width: 100%;
    min-width: 0;
  }

  .fill .select-label {
    flex: 1;
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
  }

  .select-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border-input);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-family: inherit;
    font-weight: 500;
    line-height: 1.3;
    white-space: nowrap;
    user-select: none;
    transition:
      border-color var(--t-fast) var(--ease-out),
      background-color var(--t-fast) var(--ease-out),
      box-shadow var(--t-fast) var(--ease-out);
  }

  .select-btn:hover:not(:disabled) {
    border-color: var(--border-input-hover);
    background: var(--raised);
  }

  .select-btn:focus-visible,
  .select-btn.active {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-ring);
    background: var(--surface);
  }

  .select-btn:disabled {
    cursor: not-allowed;
    opacity: 0.55;
    background: var(--sunken);
  }

  .select-sm {
    padding: 5px 9px;
    font-size: var(--fs-sm);
    height: 32px;
  }

  .select-md {
    padding: 7px 11px;
    font-size: var(--fs-base);
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
    color: var(--text);
  }

  .select-label {
    min-width: 0;
    font-weight: 500;
  }

  .chevron {
    flex: none;
    display: inline-flex;
    align-items: center;
    color: var(--muted);
    margin-left: 2px;
    transition:
      transform var(--t-med) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }

  .select-btn:hover:not(:disabled) .chevron,
  .select-btn.active .chevron {
    color: var(--text);
  }

  .chevron.flipped {
    transform: rotate(180deg);
  }

  /* Popover menu/item visuals come from the global .dropdown-menu/.dropdown-item primitive */
  .select-menu {
    min-width: max(100%, 150px);
    outline: none;
  }

  .opt-icon {
    flex: none;
    display: inline-flex;
    align-items: center;
    color: var(--muted);
  }

  .select-opt.active .opt-icon,
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
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
</style>
