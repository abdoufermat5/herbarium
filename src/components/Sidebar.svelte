<script lang="ts">
  import { app, dueLabel, clearFilters, toggleTheme } from "../lib/state.svelte";
  import Icon from "../lib/Icon.svelte";
  import { modKey } from "../lib/format";
  import { t, i18n, otherLocale, toggleLocale, LOCALES } from "../lib/i18n.svelte";
  import { slide } from "svelte/transition";

  let foldersOpen = $state(true);
  let tagsOpen = $state(true);

  const vaultName = $derived(
    app.config?.vaultPath?.split(/[\\/]/).filter(Boolean).pop() ?? "Herbarium",
  );

  function goAll() {
    app.view = "list";
    clearFilters();
  }

  function goReview() {
    app.view = "review";
  }

  function filterFolder(folder: string) {
    app.folderFilter = app.folderFilter === folder ? null : folder;
    app.tagFilter = null;
    app.view = "list";
  }

  function toggleTag(tag: string) {
    app.tagFilter = app.tagFilter === tag ? null : tag;
    app.folderFilter = null;
    app.view = "list";
  }
</script>

<aside class="sidebar">
  <div class="brand">
    <div class="brand-mark"><Icon name="leaf" size={19} /></div>
    <div class="brand-text">
      <strong>Herbarium</strong>
      <span class="brand-vault ellipsis" title={app.config?.vaultPath ?? ""}>{vaultName}</span>
    </div>
  </div>

  <div class="body">
    <nav class="nav" aria-label={t("sidebar.library")}>
      <button
        class="nav-item"
        class:active={app.view === "list" && !app.folderFilter && !app.tagFilter}
        onclick={goAll}
      >
        <span class="nav-icon"><Icon name="files" size={15} /></span>
        <span class="nav-label ellipsis">{t("sidebar.all")}</span>
      </button>
      <button
        class="nav-item"
        class:active={app.view === "review"}
        title={dueLabel()}
        onclick={goReview}
      >
        <span class="nav-icon"><Icon name="refresh-cw" size={15} /></span>
        <span class="nav-label ellipsis">{t("sidebar.review")}</span>
        {#if app.dueCount > 0}
          <span class="badge">{app.dueCount}</span>
        {/if}
      </button>
    </nav>

    {#if app.folders.length > 0}
      <div class="group">
        <button
          class="group-title"
          aria-expanded={foldersOpen}
          onclick={() => (foldersOpen = !foldersOpen)}
        >
          <span class="chev" class:open={foldersOpen}><Icon name="chevron-right" size={13} /></span>
          {t("sidebar.folders")}
        </button>
        {#if foldersOpen}
          <div class="list" transition:slide={{ duration: 140 }}>
            {#each app.folders as folder (folder)}
              <button
                class="nav-item nav-sub"
                class:active={app.view === "list" && app.folderFilter === folder}
                title={folder}
                onclick={() => filterFolder(folder)}
              >
                <span class="nav-icon"><Icon name="folder" size={14} /></span>
                <span class="nav-label ellipsis">{folder}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    {#if app.tags.length > 0}
      <div class="group">
        <button
          class="group-title"
          aria-expanded={tagsOpen}
          onclick={() => (tagsOpen = !tagsOpen)}
        >
          <span class="chev" class:open={tagsOpen}><Icon name="chevron-right" size={13} /></span>
          {t("sidebar.tags")}
        </button>
        {#if tagsOpen}
          <div class="list" transition:slide={{ duration: 140 }}>
            {#each app.tags as tg (tg.tag)}
              <button
                class="nav-item nav-sub"
                class:active={app.view === "list" && app.tagFilter === tg.tag}
                title={tg.tag}
                onclick={() => toggleTag(tg.tag)}
              >
                <span class="nav-icon"><Icon name="hash" size={14} /></span>
                <span class="nav-label ellipsis">{tg.tag}</span>
                <span class="count">{tg.count}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    {#if app.folders.length === 0 && app.tags.length === 0}
      <div class="hint">
        <Icon name="info" size={14} />
        <span>{t("sidebar.hint")}</span>
      </div>
    {/if}
  </div>

  <div class="foot">
    <div class="foot-row">
      <button class="search-btn" onclick={() => (app.paletteOpen = true)}>
        <Icon name="search" size={14} />
        <span>{t("sidebar.search")}</span>
        <kbd class="kbd">{modKey("K")}</kbd>
      </button>
      <button
        class="btn btn-icon theme-btn"
        onclick={toggleTheme}
        aria-label={app.theme === "dark" ? t("prefs.themeLight") : t("prefs.themeDark")}
        title={app.theme === "dark" ? t("prefs.themeLight") : t("prefs.themeDark")}
      >
        <Icon name={app.theme === "dark" ? "sun" : "moon"} size={15} />
      </button>
      <button
        class="btn btn-icon theme-btn lang-btn"
        onclick={toggleLocale}
        aria-label={t("prefs.switchLang", { language: LOCALES[otherLocale()].name })}
        title={t("prefs.switchLang", { language: LOCALES[otherLocale()].name })}
      >
        {LOCALES[i18n.locale].short}
      </button>
    </div>
    <p class="shortcut-hint">
      <kbd class="kbd">/</kbd> {t("sidebar.toSearch")} · <kbd class="kbd">i</kbd> {t("sidebar.toImport")}
    </p>
  </div>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--raised);
    border-right: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 14px 14px;
  }
  .brand-mark {
    flex: none;
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
  .brand-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.25;
  }
  .brand-text strong {
    font-family: var(--font-display);
    font-size: 16.5px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--text);
  }
  .brand-vault {
    font-size: 11.5px;
    color: var(--muted);
  }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: 0 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 10px;
    border-radius: var(--radius-sm);
    font-size: 13.5px;
    font-weight: 500;
    color: var(--text-soft);
    text-align: left;
    transition: background var(--t-fast) var(--ease-out), color var(--t-fast) var(--ease-out);
  }
  .nav-item:hover {
    background: var(--sunken);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
  .nav-icon {
    flex: none;
    display: grid;
    place-items: center;
    opacity: 0.85;
  }
  .nav-label {
    flex: 1;
    min-width: 0;
  }
  .nav-item .badge {
    margin-left: auto;
  }
  .nav-sub {
    font-weight: 400;
    color: var(--muted);
  }
  .nav-sub .count {
    margin-left: auto;
    font-size: 11.5px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .group-title {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px 8px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .group-title:hover {
    color: var(--text-soft);
  }
  .chev {
    display: grid;
    place-items: center;
    transition: transform var(--t-fast) var(--ease-out);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .hint {
    display: flex;
    gap: 8px;
    padding: 10px 10px;
    font-size: 12px;
    line-height: 1.45;
    color: var(--muted);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-sm);
  }
  .hint :global(svg) {
    margin-top: 1px;
  }

  .foot {
    flex: none;
    padding: 10px 10px 12px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .foot-row {
    display: flex;
    gap: 6px;
  }
  .theme-btn {
    flex: none;
    height: auto;
    width: 34px;
    color: var(--muted);
  }
  .lang-btn {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.03em;
  }
  .search-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
    font-size: 13px;
    transition: border-color var(--t-fast) var(--ease-out), color var(--t-fast) var(--ease-out);
  }
  .search-btn:hover {
    border-color: var(--border-hover);
    color: var(--text-soft);
  }
  .search-btn span {
    flex: 1;
    text-align: left;
  }
  .shortcut-hint {
    font-size: 11px;
    color: var(--muted);
    text-align: center;
  }
</style>
