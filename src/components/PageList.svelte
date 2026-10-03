<script lang="ts">
  import { onDestroy } from "svelte";
  import { app, visiblePages, reloadPages, toast, clearFilters, setLayout, setSort, type SortKey } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import type { PageMeta } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { reveal } from "../lib/reveal";
  import Select from "./Select.svelte";
  import { dueInfo, plural } from "../lib/format";
  import { t } from "../lib/i18n.svelte";

  let searchInput = $state(app.search);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let confirmDelId = $state<string | null>(null);

  const pages = $derived(visiblePages(app.pages));
  const filtersActive = $derived(!!(app.folderFilter || app.tagFilter));
  const eyebrow = $derived(
    app.folderFilter ? t("sidebar.folders") : app.tagFilter ? t("sidebar.tags") : t("sidebar.library"),
  );
  const heading = $derived(
    app.folderFilter ?? (app.tagFilter ? `#${app.tagFilter}` : t("sidebar.all")),
  );
  const sortOptions = $derived<Array<{ value: SortKey; label: string }>>([
    { value: "recent", label: t("list.sortRecent") },
    { value: "title", label: t("list.sortTitle") },
    { value: "review", label: t("list.sortReview") },
  ]);
  onDestroy(() => {
    clearTimeout(timer);
  });

  function onSearch() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      app.search = searchInput.trim();
      reloadPages();
    }, 250);
  }

  function clearSearch() {
    clearTimeout(timer);
    searchInput = "";
    app.search = "";
    reloadPages();
  }

  function requestDelete(p: PageMeta) {
    if (confirmDelId !== p.id) {
      confirmDelId = p.id;
      return;
    }
    confirmDelId = null;
    void removePage(p);
  }

  async function removePage(p: PageMeta) {
    try {
      await api.deletePage(p.id);
      toast(t("toast.deleted", { title: p.title || t("common.untitled") }));
      await reloadPages();
    } catch (e) {
      console.error(e);
      toast(t("toast.deleteFailed"), "error");
    }
  }
</script>

<section class="pane" aria-label={t("list.label")}>
  <div class="inner">
    <header class="head">
      <div class="head-text">
        <span class="eyebrow">{eyebrow}</span>
        <h1 class="display ellipsis" title={heading}>{heading}</h1>
      </div>
      <div class="head-meta">
        {#if app.busy}
          <span class="spinner" aria-hidden="true"></span>
        {/if}
        {#if !(app.busy && app.pages.length === 0)}
          <span class="count">{plural(pages.length, "page")}</span>
        {/if}
        {#if filtersActive}
          <button class="btn btn-ghost btn-sm" onclick={clearFilters}>
            <Icon name="x" size={11} />
            {t("list.clearFilters")}
          </button>
        {/if}
      </div>
    </header>

    <div class="toolbar">
      <div class="search">
        <Icon name="search" size={14} class="search-icon" />
        <input
          id="page-search"
          type="text"
          bind:value={searchInput}
          oninput={onSearch}
          placeholder={t("list.searchPlaceholder")}
          aria-label={t("list.searchLabel")}
          spellcheck="false"
          autocomplete="off"
        />
        {#if searchInput}
          <button class="clear" aria-label={t("list.clearSearch")} onclick={clearSearch}>
            <Icon name="x" size={11} />
          </button>
        {:else}
          <kbd class="kbd slash">/</kbd>
        {/if}
      </div>

      <div class="tools">
        <Select
          value={app.sort}
          options={sortOptions}
          icon="arrow-up-down"
          ariaLabel={t("list.sort")}
          onchange={(val) => setSort(val)}
          size="sm"
        />

        <div class="seg" role="group" aria-label={t("list.layout")}>
          <button
            class="seg-btn"
            class:active={app.layout === "grid"}
            aria-pressed={app.layout === "grid"}
            title={t("list.grid")}
            onclick={() => setLayout("grid")}
          >
            <Icon name="layout-grid" size={14} />
          </button>
          <button
            class="seg-btn"
            class:active={app.layout === "list"}
            aria-pressed={app.layout === "list"}
            title={t("list.list")}
            onclick={() => setLayout("list")}
          >
            <Icon name="rows-3" size={14} />
          </button>
        </div>

        <button class="btn btn-primary" onclick={() => (app.importOpen = true)}>
          <Icon name="plus" size={13} />
          <span>{t("list.import")}</span>
        </button>
      </div>
    </div>

    {#if app.busy && app.pages.length === 0}
      <div class="grid" class:list={app.layout === "list"} aria-hidden="true">
        {#each Array(6) as _, i (i)}
          <div class="skel"></div>
        {/each}
      </div>
    {:else if pages.length === 0}
      <div class="empty">
        <span class="empty-icon">
          <Icon name={app.search ? "search" : "files"} size={20} />
        </span>
        <strong>
          {app.search
            ? t("list.noMatch", { query: app.search })
            : filtersActive
              ? t("list.emptyFilter")
              : t("list.empty")}
        </strong>
        <span>
          {app.search
            ? t("list.noMatchHint")
            : filtersActive
              ? t("list.emptyFilterHint")
              : t("list.emptyHint")}
        </span>
        {#if app.search}
          <button class="btn btn-sm" onclick={clearSearch}>{t("list.clearSearch")}</button>
        {:else if filtersActive}
          <button class="btn btn-sm" onclick={clearFilters}>{t("list.clearFilters")}</button>
        {:else}
          <button class="btn btn-primary btn-sm" onclick={() => (app.importOpen = true)}>
            <Icon name="plus" size={12} />
            {t("list.importPage")}
          </button>
        {/if}
      </div>
    {:else}
      <div class="grid" class:list={app.layout === "list"}>
        {#each pages as p (p.id)}
          {@const due = dueInfo(p.nextReview)}
          <article class="page" use:reveal>
            <button class="page-open" onclick={() => (app.readId = p.id)}>
              <span class="thumb"><Icon name="leaf" size={15} /></span>
              <span class="page-body">
                <span class="page-title">{p.title || t("common.untitled")}</span>
                {#if p.note}
                  <span class="note">{p.note}</span>
                {/if}
                {#if p.folder || p.tags.length > 0}
                  <span class="meta">
                    {#if p.folder}
                      <span class="loc"><Icon name="folder" size={11} />{p.folder}</span>
                    {/if}
                    {#each p.tags.slice(0, 3) as tag (tag)}
                      <span class="chip chip-muted">{tag}</span>
                    {/each}
                    {#if p.tags.length > 3}
                      <span class="chip chip-muted">+{p.tags.length - 3}</span>
                    {/if}
                  </span>
                {/if}
              </span>
            </button>
            <span class="page-side">
              {#if due && confirmDelId !== p.id}
                <span
                  class="chip due"
                  class:chip-warn={due.hot && !due.overdue}
                  class:chip-danger={due.overdue}
                  class:chip-muted={!due.hot && !due.overdue}
                >
                  {due.label}
                </span>
              {/if}
              {#if confirmDelId === p.id}
                <button
                  class="btn btn-ghost btn-sm"
                  aria-label={t("list.cancelDelete", { title: p.title || t("common.untitled") })}
                  onclick={() => (confirmDelId = null)}
                >
                  {t("list.cancel")}
                </button>
              {/if}
              <button
                class="btn btn-ghost btn-sm del"
                class:btn-icon={confirmDelId !== p.id}
                class:confirm={confirmDelId === p.id}
                aria-label={confirmDelId === p.id
                  ? t("list.confirmDelete", { title: p.title || t("common.untitled") })
                  : t("list.delete", { title: p.title || t("common.untitled") })}
                onclick={() => requestDelete(p)}
              >
                {#if confirmDelId === p.id}
                  {t("list.confirm")}
                {:else}
                  <Icon name="trash-2" size={14} />
                {/if}
              </button>
            </span>
          </article>
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  .pane {
    height: 100%;
    overflow-y: auto;
  }
  .inner {
    max-width: var(--content-w);
    margin: 0 auto;
    padding: 48px 40px 72px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 24px;
  }
  .head-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h1 {
    font-size: var(--fs-4xl);
    padding-bottom: 2px;
  }
  .head-meta {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding-bottom: 6px;
    color: var(--muted);
  }
  .count {
    font-family: var(--mono);
    font-size: var(--fs-xs);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding-bottom: 20px;
    border-bottom: 1px solid var(--border);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 220px;
    max-width: 440px;
  }
  .search :global(.search-icon) {
    position: absolute;
    left: 11px;
    color: var(--muted);
    pointer-events: none;
  }
  .search input {
    width: 100%;
    padding-left: 32px;
    padding-right: 40px;
    background: var(--surface);
  }
  .slash {
    position: absolute;
    right: 8px;
    pointer-events: none;
  }
  .clear {
    position: absolute;
    right: 6px;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-xs);
    color: var(--muted);
  }
  .clear:hover {
    background: var(--sunken);
    color: var(--text);
  }

  /* ---- Grid: bento cards ---- */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 16px;
  }

  .page {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    transition:
      border-color var(--t-med) var(--ease-out),
      box-shadow var(--t-med) var(--ease-out),
      background var(--t-med) var(--ease-out);
  }
  .page:hover {
    border-color: var(--border-hover);
    box-shadow: var(--shadow);
  }
  .page-open {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 18px;
    padding: 24px;
    text-align: left;
    border-radius: inherit;
  }
  .thumb {
    flex: none;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--leaf-soft);
    color: var(--leaf);
  }
  .page-body {
    flex: 1;
    width: 100%;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .page-title {
    font-family: var(--font-display);
    font-size: var(--fs-lg);
    font-weight: 500;
    line-height: 1.25;
    letter-spacing: -0.015em;
    color: var(--text);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .note {
    font-size: var(--fs-sm);
    line-height: 1.55;
    color: var(--muted);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: auto;
    padding-top: 6px;
  }
  .loc {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-right: 2px;
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .page-side {
    position: absolute;
    top: 20px;
    right: 18px;
    left: 66px;
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 4px;
    pointer-events: none;
  }
  .page-side > :global(*) {
    pointer-events: auto;
  }

  .del {
    opacity: 0;
    transition:
      opacity var(--t-fast) var(--ease-out),
      background var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }
  .page:hover .del,
  .page:focus-within .del,
  .del.confirm {
    opacity: 1;
  }
  .del:hover:not(.confirm) {
    color: var(--danger);
  }
  .del.confirm {
    background: var(--danger-fill);
    border-color: var(--danger-fill);
    color: var(--on-danger);
  }
  .del.confirm:hover {
    background: var(--danger-fill);
    color: var(--on-danger);
  }

  /* ---- List: document rows separated by hairlines ---- */
  .grid.list {
    grid-template-columns: 1fr;
    gap: 0;
    margin-top: -24px;
  }
  .list .page {
    flex-direction: row;
    align-items: center;
    border: none;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    background: transparent;
  }
  .list .page:hover {
    box-shadow: none;
    background: var(--surface);
  }
  .list .page-open {
    flex-direction: row;
    align-items: center;
    gap: 14px;
    padding: 14px 8px;
  }
  .list .page-body {
    gap: 2px;
  }
  .list .page-title {
    font-family: var(--font);
    font-size: var(--fs-base);
    letter-spacing: 0;
    -webkit-line-clamp: 1;
    line-clamp: 1;
  }
  .list .note {
    -webkit-line-clamp: 1;
    line-clamp: 1;
    font-size: var(--fs-sm);
  }
  .list .meta {
    margin-top: 2px;
    padding-top: 0;
  }
  .list .page-side {
    position: static;
    flex: none;
    padding-right: 8px;
  }

  .skel {
    height: 168px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface);
    animation: pulse 1.6s ease-in-out infinite;
  }
  .list .skel {
    height: 64px;
    border: none;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    background: transparent;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
</style>
