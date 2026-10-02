<script lang="ts">
  import { onDestroy } from "svelte";
  import { app, visiblePages, reloadPages, toast, clearFilters, setLayout, setSort, type SortKey } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import type { PageMeta } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { dueInfo, plural } from "../lib/format";

  let searchInput = $state(app.search);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let confirmDelId = $state<string | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  const pages = $derived(visiblePages(app.pages));
  const filtersActive = $derived(!!(app.folderFilter || app.tagFilter));

  onDestroy(() => {
    clearTimeout(timer);
    clearTimeout(confirmTimer);
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
      clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => (confirmDelId = null), 2500);
      return;
    }
    confirmDelId = null;
    clearTimeout(confirmTimer);
    void removePage(p);
  }

  async function removePage(p: PageMeta) {
    try {
      await api.deletePage(p.id);
      toast(`Deleted “${p.title || "Untitled page"}”.`);
      await reloadPages();
    } catch (e) {
      console.error(e);
      toast("Couldn't delete the page.", "error");
    }
  }
</script>

<section class="pane" aria-label="Pages">
  <div class="toolbar">
    <div class="search">
      <Icon name="search" size={15} class="search-icon" />
      <input
        id="page-search"
        bind:value={searchInput}
        oninput={onSearch}
        placeholder="Search titles, tags, content…"
        aria-label="Search pages"
        spellcheck="false"
        autocomplete="off"
      />
      {#if searchInput}
        <button class="clear" aria-label="Clear search" onclick={clearSearch}>
          <Icon name="x" size={12} />
        </button>
      {:else}
        <kbd class="kbd slash">/</kbd>
      {/if}
    </div>

    <label class="sort">
      <span class="sr-only">Sort pages</span>
      <Icon name="arrow-up-down" size={14} />
      <select
        value={app.sort}
        onchange={(e) => setSort(e.currentTarget.value as SortKey)}
      >
        <option value="recent">Recent</option>
        <option value="title">Title</option>
        <option value="review">Review date</option>
      </select>
    </label>

    <div class="seg" role="group" aria-label="Layout">
      <button
        class="seg-btn"
        class:active={app.layout === "grid"}
        aria-pressed={app.layout === "grid"}
        title="Grid view"
        onclick={() => setLayout("grid")}
      >
        <Icon name="layout-grid" size={14} />
      </button>
      <button
        class="seg-btn"
        class:active={app.layout === "list"}
        aria-pressed={app.layout === "list"}
        title="List view"
        onclick={() => setLayout("list")}
      >
        <Icon name="rows-3" size={14} />
      </button>
    </div>

    <button class="btn btn-primary" onclick={() => (app.importOpen = true)}>
      <Icon name="plus" size={14} />
      <span>Import</span>
    </button>
  </div>

  {#if filtersActive}
    <div class="filters">
      <span class="muted">Filtering:</span>
      {#if app.folderFilter}
        <button class="chip filter-chip" onclick={() => (app.folderFilter = null)}>
          <Icon name="folder" size={12} />
          {app.folderFilter}
          <Icon name="x" size={11} />
        </button>
      {/if}
      {#if app.tagFilter}
        <button class="chip filter-chip" onclick={() => (app.tagFilter = null)}>
          <Icon name="hash" size={12} />
          {app.tagFilter}
          <Icon name="x" size={11} />
        </button>
      {/if}
      <button class="btn btn-ghost btn-xs" onclick={clearFilters}>Clear filters</button>
    </div>
  {/if}

  {#if app.busy && app.pages.length === 0}
    <div class="grid" aria-hidden="true">
      {#each Array(6) as _, i (i)}
        <div class="skel"></div>
      {/each}
    </div>
  {:else if pages.length === 0}
    <div class="empty">
      <span class="empty-icon">
        <Icon name={app.search ? "search" : "files"} size={22} />
      </span>
      <strong>
        {app.search
          ? `Nothing matches “${app.search}”`
          : filtersActive
            ? "No pages in this view"
            : "No pages yet"}
      </strong>
      <span>
        {app.search
          ? "Try a different word, or clear the search."
          : filtersActive
            ? "Clear the filters to see the rest of your library."
            : "Drag an HTML file onto this window, or import one to get started."}
      </span>
      {#if app.search}
        <button class="btn btn-sm" onclick={clearSearch}>Clear search</button>
      {:else if filtersActive}
        <button class="btn btn-sm" onclick={clearFilters}>Clear filters</button>
      {:else}
        <button class="btn btn-primary btn-sm" onclick={() => (app.importOpen = true)}>
          <Icon name="plus" size={13} />
          Import a page
        </button>
      {/if}
    </div>
  {:else}
    <div class="count-bar">
      <span class="muted">{plural(pages.length, "page")}</span>
      {#if app.busy}
        <span class="spinner"></span>
      {/if}
    </div>
    <div class="grid" class:list={app.layout === "list"}>
      {#each pages as p (p.id)}
        {@const due = dueInfo(p.nextReview)}
        <article class="card page">
          <button class="page-open" onclick={() => (app.readId = p.id)}>
            <span class="thumb"><Icon name="leaf" size={17} /></span>
            <span class="page-body">
              <span class="page-title ellipsis">{p.title || "Untitled page"}</span>
              <span class="meta">
                {#if p.folder}
                  <span class="loc"><Icon name="folder" size={12} />{p.folder}</span>
                {/if}
                {#each p.tags.slice(0, 3) as tag (tag)}
                  <span class="chip chip-muted">#{tag}</span>
                {/each}
                {#if p.tags.length > 3}
                  <span class="chip chip-muted">+{p.tags.length - 3}</span>
                {/if}
              </span>
              {#if app.layout === "list" && p.note}
                <span class="note ellipsis">{p.note}</span>
              {/if}
            </span>
          </button>
          <span class="page-side">
            {#if due}
              <span class="due" class:hot={due.hot && !due.overdue} class:overdue={due.overdue}>
                {due.label}
              </span>
            {/if}
            <button
              class="btn btn-ghost btn-sm del"
              class:confirm={confirmDelId === p.id}
              aria-label={confirmDelId === p.id
                ? `Confirm delete “${p.title || "Untitled page"}”`
                : `Delete “${p.title || "Untitled page"}”`}
              onclick={() => requestDelete(p)}
            >
              {#if confirmDelId === p.id}
                <span class="del-text">Confirm</span>
              {:else}
                <Icon name="trash-2" size={14} />
              {/if}
            </button>
          </span>
        </article>
      {/each}
    </div>
  {/if}
</section>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 20px 24px;
    overflow-y: auto;
    height: 100%;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 200px;
    max-width: 520px;
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
    padding-right: 52px;
  }
  .slash {
    position: absolute;
    right: 8px;
    pointer-events: none;
  }
  .clear {
    position: absolute;
    right: 6px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 999px;
    color: var(--muted);
  }
  .clear:hover {
    background: var(--sunken);
    color: var(--text);
  }

  .sort {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }
  .sort select {
    padding: 7px 26px 7px 9px;
    font-size: 13px;
  }

  .filters {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 12.5px;
  }
  .filter-chip {
    cursor: pointer;
    border: none;
  }
  .filter-chip:hover {
    background: var(--accent);
    color: #fff;
  }

  .count-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
    align-items: start;
  }
  .grid.list {
    grid-template-columns: 1fr;
  }

  .page {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 12px 12px 14px;
    transition: border-color var(--t-fast) var(--ease-out),
      box-shadow var(--t-fast) var(--ease-out);
  }
  .page:hover {
    border-color: var(--border-hover);
    box-shadow: var(--shadow);
  }
  .page-open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    text-align: left;
    border-radius: var(--radius-sm);
  }
  .page-open:active {
    transform: translateY(1px);
  }
  .thumb {
    flex: none;
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
  .page-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .page-title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text);
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .loc {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    color: var(--muted);
  }
  .note {
    font-size: 12px;
    color: var(--muted);
  }
  .page-side {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .due {
    font-size: 11px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 999px;
    background: var(--sunken);
    color: var(--text-soft);
    white-space: nowrap;
  }
  .due.hot {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .due.overdue {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .del {
    opacity: 0;
    transition: opacity var(--t-fast) var(--ease-out);
  }
  .page:hover .del,
  .page:focus-within .del,
  .del.confirm {
    opacity: 1;
  }
  .del.confirm {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }
  .del-text {
    font-size: 12px;
  }

  .skel {
    height: 66px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: linear-gradient(90deg, var(--raised), var(--sunken), var(--raised));
    background-size: 200% 100%;
    animation: shimmer 1.4s linear infinite;
  }
  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }
</style>
