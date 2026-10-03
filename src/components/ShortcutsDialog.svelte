<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import { sectionForContext, shortcutSections, type ShortcutContext, type ShortcutSectionId } from "../lib/shortcuts";

  let {
    onclose,
    context,
    section,
  }: {
    onclose: () => void;
    /** Screen the dialog was opened from; selects the matching section. */
    context?: ShortcutContext;
    /** Explicit section; wins over `context`. Omit both to show everything. */
    section?: ShortcutSectionId;
  } = $props();

  let dialogEl: HTMLDivElement | undefined = $state();
  let closeBtn: HTMLButtonElement | undefined = $state();
  const prevFocus = document.activeElement as HTMLElement | null;

  // Initial selection only; the user drives it afterwards.
  // svelte-ignore state_referenced_locally
  let selected = $state<ShortcutSectionId | "all">(section ?? (context ? sectionForContext(context) : "all"));
  let filter = $state("");

  const sections = $derived(shortcutSections());
  const visible = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return sections
      .filter((s) => selected === "all" || s.id === selected)
      .map((s) => ({
        ...s,
        items: q
          ? s.items.filter((i) =>
              `${i.action} ${i.contextLabel} ${i.keys.join(" ")}`.toLowerCase().includes(q),
            )
          : s.items,
      }))
      .filter((s) => s.items.length > 0);
  });

  const FOCUSABLE = 'button:not(:disabled), input:not(:disabled), [tabindex]:not([tabindex="-1"])';

  // Capture phase: the dialog owns Escape/Tab before App's shortcuts see them.
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
      return;
    }
    if (e.key !== "Tab" || !dialogEl) return;
    const items = Array.from(dialogEl.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
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
    closeBtn?.focus();
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (prevFocus?.isConnected) prevFocus.focus();
    };
  });
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div
    class="dialog card"
    role="dialog"
    aria-modal="true"
    aria-labelledby="shortcuts-title"
    aria-describedby="shortcuts-hint"
    tabindex="-1"
    bind:this={dialogEl}
  >
    <header class="head">
      <div class="titles">
        <h2 id="shortcuts-title" class="display">{t("shortcuts.title")}</h2>
        <p id="shortcuts-hint" class="hint">{t("shortcuts.hint")}</p>
      </div>
      <button class="btn btn-icon" bind:this={closeBtn} onclick={onclose} aria-label={t("common.close")} title={t("common.close")}>
        <Icon name="x" size={16} />
      </button>
    </header>

    <div class="tabs" role="group" aria-label={t("shortcuts.sections")}>
      <button class="tab" class:active={selected === "all"} aria-pressed={selected === "all"} onclick={() => (selected = "all")}>
        {t("shortcuts.section.all")}
      </button>
      {#each sections as s (s.id)}
        <button class="tab" class:active={selected === s.id} aria-pressed={selected === s.id} onclick={() => (selected = s.id)}>
          {s.title}
        </button>
      {/each}
    </div>

    <input class="filter" type="search" placeholder={t("shortcuts.filter")} aria-label={t("shortcuts.filter")} bind:value={filter} />

    <div class="body">
      {#each visible as s (s.id)}
        <section aria-labelledby={`shortcuts-sec-${s.id}`}>
          <h3 id={`shortcuts-sec-${s.id}`}>{s.title}</h3>
          <ul>
            {#each s.items as item (item.id)}
              <li>
                <div class="what">
                  <span class="action">{item.action}</span>
                  <span class="ctx">{item.contextLabel}</span>
                </div>
                <div class="keys">
                  {#each item.keys as k, i (i)}
                    {#if i > 0}<span class="or">{t("shortcuts.or")}</span>{/if}
                    <kbd class="kbd">{k}</kbd>
                  {/each}
                </div>
              </li>
            {/each}
          </ul>
        </section>
      {:else}
        <p class="empty">{t("shortcuts.empty")}</p>
      {/each}
    </div>
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
    padding: 10vh 24px 24px;
    background: var(--scrim);
    animation: sc-fade var(--t-fast) var(--ease-out);
  }

  .dialog {
    width: 640px;
    max-width: 100%;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 22px;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    outline: none;
    animation: sc-pop var(--t-med) var(--ease-out);
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    font-size: var(--fs-xl);
  }

  .hint {
    margin-top: 2px;
    font-size: var(--fs-sm);
    color: var(--muted);
  }

  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .tab {
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: transparent;
    color: var(--text-soft);
    font: inherit;
    font-size: var(--fs-xs);
    cursor: pointer;
  }

  .tab:hover {
    background: var(--sunken);
  }

  .tab.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent-strong);
  }

  .tab:focus-visible,
  .filter:focus-visible {
    outline: none;
    box-shadow: 0 0 0 3px var(--accent-ring);
  }

  .filter {
    width: 100%;
    padding: 7px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--sunken);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-right: 4px;
  }

  h3 {
    margin-bottom: 6px;
    font-size: var(--fs-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 7px 0;
    border-top: 1px solid var(--border);
  }

  li:first-child {
    border-top: none;
  }

  .what {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .action {
    font-size: var(--fs-sm);
    color: var(--text);
  }

  .ctx {
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .keys {
    flex: none;
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .or {
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .empty {
    padding: 24px 0;
    text-align: center;
    font-size: var(--fs-sm);
    color: var(--muted);
  }

  @keyframes sc-fade {
    from {
      opacity: 0;
    }
  }

  @keyframes sc-pop {
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
