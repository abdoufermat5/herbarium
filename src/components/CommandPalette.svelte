<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { app, setLayout, toggleTheme, clearFilters, dueLabel } from "../lib/state.svelte";
  import { t, otherLocale, toggleLocale, LOCALES } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import type { IconName } from "../lib/icons";

  interface Item {
    key: string;
    group: string;
    label: string;
    hint?: string;
    icon: IconName;
    run: () => void;
  }

  let q = $state("");
  let active = $state(0);
  let inputEl: HTMLInputElement | undefined;
  let prevFocus: HTMLElement | null = null;

  const actions = $derived.by<Item[]>(() => {
    const list: Item[] = [
      {
        key: "import",
        group: t("palette.actions"),
        label: t("palette.import"),
        hint: t("palette.importHint"),
        icon: "file-plus",
        run: () => (app.importOpen = true),
      },
      {
        key: "review",
        group: t("palette.actions"),
        label: t("palette.goReview"),
        hint: dueLabel(),
        icon: "refresh-cw",
        run: () => {
          app.readId = null;
          app.view = "review";
        },
      },
      {
        key: "all",
        group: t("palette.actions"),
        label: t("palette.goAll"),
        icon: "files",
        run: () => {
          app.readId = null;
          app.view = "list";
        },
      },
      {
        key: "settings",
        group: t("palette.actions"),
        label: t("palette.goSettings"),
        icon: "settings",
        run: () => {
          app.readId = null;
          app.view = "settings";
        },
      },
      {
        key: "lang",
        group: t("palette.actions"),
        label: t("prefs.switchLang", { language: LOCALES[otherLocale()].name }),
        icon: "languages",
        run: toggleLocale,
      },
      {
        key: "theme",
        group: t("palette.actions"),
        label: app.theme === "dark" ? t("prefs.themeLight") : t("prefs.themeDark"),
        icon: app.theme === "dark" ? "sun" : "moon",
        run: toggleTheme,
      },
      {
        key: "layout",
        group: t("palette.actions"),
        label: app.layout === "grid" ? t("palette.toList") : t("palette.toGrid"),
        icon: app.layout === "grid" ? "rows-3" : "layout-grid",
        run: () => setLayout(app.layout === "grid" ? "list" : "grid"),
      },
    ];
    if (app.folderFilter || app.tagFilter) {
      list.push({
        key: "clear-filters",
        group: t("palette.actions"),
        label: t("palette.clearFilters"),
        icon: "x",
        run: () => {
          clearFilters();
          app.readId = null;
          app.view = "list";
        },
      });
    }
    return list;
  });

  const pageItems = $derived.by<Item[]>(() => {
    const needle = q.trim().toLowerCase();
    const base = needle
      ? app.pages.filter(
          (p) =>
            (p.title || "").toLowerCase().includes(needle) ||
            p.tags.some((tag) => tag.toLowerCase().includes(needle)) ||
            (p.folder ?? "").toLowerCase().includes(needle),
        )
      : [...app.pages];
    base.sort((a, b) => (b.updatedAt || b.createdAt) - (a.updatedAt || a.createdAt));
    return base.slice(0, 8).map((p) => ({
      key: `page-${p.id}`,
      group: t("palette.pages"),
      label: p.title || t("common.untitled"),
      hint: [
        p.folder ? `▸ ${p.folder}` : null,
        p.tags.slice(0, 3).map((tag) => `#${tag}`).join(" ") || null,
      ]
        .filter(Boolean)
        .join("   "),
      icon: "file-text" as IconName,
      run: () => (app.readId = p.id),
    }));
  });

  const items = $derived.by<Item[]>(() => {
    const needle = q.trim().toLowerCase();
    const acts = needle ? actions.filter((a) => a.label.toLowerCase().includes(needle)) : actions;
    return [...acts, ...pageItems];
  });

  $effect(() => {
    const len = items.length;
    if (active >= len) active = Math.max(0, len - 1);
  });

  $effect(() => {
    const id = `palette-item-${active}`;
    document.getElementById(id)?.scrollIntoView({ block: "nearest" });
  });

  onMount(() => {
    prevFocus = document.activeElement as HTMLElement;
    queueMicrotask(() => inputEl?.focus());
  });

  function restoreFocus() {
    if (!prevFocus || !prevFocus.isConnected) return;
    prevFocus.focus();
  }

  onDestroy(() => {
    restoreFocus();
    // The layer beneath may still be inert until this flush completes.
    if (prevFocus?.isConnected && document.activeElement !== prevFocus) queueMicrotask(restoreFocus);
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      active = Math.min(active + 1, Math.max(0, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      active = Math.max(active - 1, 0);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = items[active];
      if (item) void run(item);
    } else if (e.key === "Tab" || e.code === "Tab") {
      e.preventDefault();
    }
  }

  let running = false;

  // Unmount the palette (restoring focus beneath) before activating the destination.
  async function run(item: Item) {
    if (running) return;
    running = true;
    app.paletteOpen = false;
    await tick();
    item.run();
  }
</script>

<div
  class="overlay"
  role="presentation"
  onmousedown={(e) => e.target === e.currentTarget && (app.paletteOpen = false)}
>
  <div class="palette" role="dialog" aria-modal="true" aria-label={t("palette.label")}>
    <div class="p-search">
      <Icon name="search" size={16} />
      <input
        bind:this={inputEl}
        bind:value={q}
        oninput={() => (active = 0)}
        onkeydown={onKeydown}
        placeholder={t("palette.placeholder")}
        aria-label={t("palette.placeholder")}
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={items.length ? `palette-item-${active}` : undefined}
        aria-autocomplete="list"
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    <div class="p-list" id="palette-list" role="listbox" aria-label={t("palette.results")}>
      {#each items as item, i (item.key)}
        {#if i === 0 || items[i - 1].group !== item.group}
          <div class="p-group">{item.group}</div>
        {/if}
        <div
          id={`palette-item-${i}`}
          class="p-item"
          class:active={i === active}
          role="option"
          tabindex={-1}
          aria-selected={i === active}
          onmousemove={() => (active = i)}
          onmousedown={(e) => {
            e.preventDefault();
            run(item);
          }}
        >
          <span class="p-icon"><Icon name={item.icon} size={15} /></span>
          <span class="p-label ellipsis">{item.label}</span>
          {#if item.hint}
            <span class="p-hint ellipsis">{item.hint}</span>
          {/if}
        </div>
      {/each}
      {#if items.length === 0}
        <div class="p-empty">{t("palette.noMatches", { query: q })}</div>
      {/if}
    </div>

    <div class="p-foot">
      <span><kbd class="kbd">↑</kbd><kbd class="kbd">↓</kbd> {t("palette.navigate")}</span>
      <span><kbd class="kbd">↵</kbd> {t("palette.openKey")}</span>
      <span><kbd class="kbd">esc</kbd> {t("palette.closeKey")}</span>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 12vh 24px 24px;
    background: var(--scrim);
    animation: fade-in var(--t-fast) var(--ease-out);
  }
  .palette {
    width: 580px;
    max-width: 100%;
    max-height: 460px;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    animation: pop-in var(--t-med) var(--ease-out);
  }
  .p-search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
  }
  .p-search input {
    flex: 1;
    border: none;
    background: transparent;
    padding: 2px 0;
    font-size: 14.5px;
    color: var(--text);
    outline: none;
    box-shadow: none;
  }

  .p-list {
    flex: 1;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 6px;
  }
  .p-group {
    padding: 10px 10px 4px;
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  .p-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-soft);
  }
  .p-item.active {
    background: var(--sunken);
    color: var(--accent-strong);
  }
  .p-icon {
    flex: none;
    display: grid;
    place-items: center;
  }
  .p-label {
    flex: 1;
    min-width: 0;
    font-size: 13.5px;
    font-weight: 500;
  }
  .p-hint {
    flex: none;
    max-width: 45%;
    font-size: 12px;
    color: var(--muted);
  }
  .p-item.active .p-hint {
    color: var(--text-soft);
  }
  .p-empty {
    padding: 22px 12px;
    text-align: center;
    font-size: 13px;
    color: var(--muted);
  }

  .p-foot {
    display: flex;
    gap: 14px;
    padding: 9px 14px;
    border-top: 1px solid var(--border);
    background: var(--raised);
    font-size: 11.5px;
    color: var(--muted);
  }
  .p-foot span {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
  @keyframes pop-in {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }
</style>
