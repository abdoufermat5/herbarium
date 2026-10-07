<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    app,
    visiblePages,
    reloadPages,
    toast,
    clearFilters,
    setLayout,
    setSort,
    openPage,
    showFolder,
    showTag,
    movePages,
    deletePages,
    duplicatePage,
    errorMessage,
    saveSearch,
    type SortKey,
  } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import type { BulkError, PageMeta } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { reveal } from "../lib/reveal";
  import { folderLook, tagColor } from "../lib/appearance";
  import Miniature from "./Miniature.svelte";
  import Select from "./Select.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import type { DropdownMenuItem } from "./DropdownMenu.svelte";
  import { dueInfo, fmtDuration, plural } from "../lib/format";
  import { t } from "../lib/i18n.svelte";

  /** Search hits carry an optional `[match]`-marked snippet on top of the page meta. */
  type Row = PageMeta & { snippet?: string };

  let searchInput = $state(app.search);
  let timer: ReturnType<typeof setTimeout> | undefined;
  /** Last value the input itself pushed into `app.search`; external changes resync the field. */
  let lastSearch = app.search;

  let selected = $state<Set<string>>(new Set());
  let anchorId = $state<string | null>(null);
  let selectAllEl: HTMLInputElement | undefined = $state();
  let tagDraft = $state("");
  let scheduleMinutes = $state(1440);
  let menu = $state<{ x: number; y: number; items: DropdownMenuItem[] } | null>(null);

  const pages = $derived(visiblePages(app.pages) as Row[]);
  const selectedIds = $derived([...selected]);
  const searching = $derived(!!app.search.trim());
  const filtersActive = $derived(!!(app.folderFilter || app.tagFilter));
  const allSelected = $derived(pages.length > 0 && pages.every((p) => selected.has(p.id)));
  const someSelected = $derived(selected.size > 0 && !allSelected);

  const eyebrow = $derived(
    app.folderFilter ? t("sidebar.folders") : app.tagFilter ? t("sidebar.tags") : t("sidebar.library"),
  );
  const heading = $derived(
    app.folderFilter ?? (app.tagFilter ? `#${app.tagFilter}` : t("sidebar.all")),
  );
  // Relevance only means something against the backend's ranked search hits.
  const sortOptions = $derived<Array<{ value: SortKey; label: string }>>([
    ...(searching ? [{ value: "relevance" as SortKey, label: t("list.sortRelevance") }] : []),
    { value: "recent", label: t("list.sortRecent") },
    { value: "title", label: t("list.sortTitle") },
    { value: "review", label: t("list.sortReview") },
  ]);
  const sortValue = $derived<SortKey>(app.sort === "relevance" && !searching ? "recent" : app.sort);
  const scheduleOptions = $derived(
    app.review.presets.map((m) => ({ value: m, label: fmtDuration(m) })),
  );

  // Keep the field in step when a route change (goAll, sidebar, palette) resets app.search.
  $effect(() => {
    const s = app.search;
    if (s === lastSearch) return;
    lastSearch = s;
    clearTimeout(timer);
    if (searchInput !== s) searchInput = s;
  });

  // Keep the bulk selection to what is actually on screen: switching vault, searching or
  // changing a filter must never leave hidden pages selected.
  $effect(() => {
    const visible = new Set(pages.map((p) => p.id));
    if (selectedIds.some((id) => !visible.has(id))) {
      selected = new Set(selectedIds.filter((id) => visible.has(id)));
      if (anchorId && !visible.has(anchorId)) anchorId = null;
    }
  });

  // The header checkbox is "mixed" while some, but not all, visible pages are picked.
  $effect(() => {
    if (selectAllEl) selectAllEl.indeterminate = someSelected;
  });

  $effect(() => {
    const presets = app.review.presets;
    if (presets.length > 0 && !presets.includes(scheduleMinutes)) scheduleMinutes = presets[0];
  });

  onDestroy(() => {
    clearTimeout(timer);
  });

  function onSearch() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      app.search = searchInput.trim();
      lastSearch = app.search;
      void reloadPages();
    }, 250);
  }

  /** Name being typed for the current search, or null when not saving. */
  let saveName = $state<string | null>(null);
  const searchSaved = $derived(app.savedSearches.some((s) => s.query === app.search.trim()));

  async function commitSaveSearch() {
    const name = saveName?.trim();
    const query = app.search.trim();
    if (!name || !query) return;
    if (await saveSearch(name, query)) saveName = null;
  }

  function onSaveNameKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void commitSaveSearch();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      saveName = null;
    }
  }

  function clearSearch() {
    clearTimeout(timer);
    searchInput = "";
    app.search = "";
    lastSearch = "";
    void reloadPages();
  }

  /* ------------------------------------------------------------- selection */

  function toggleSelect(id: string, shift: boolean) {
    const ids = pages.map((p) => p.id);
    const from = anchorId ? ids.indexOf(anchorId) : -1;
    const to = ids.indexOf(id);
    if (shift && from >= 0 && to >= 0) {
      const next = new Set(selected);
      for (let i = Math.min(from, to); i <= Math.max(from, to); i++) next.add(ids[i]);
      selected = next;
      return;
    }
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
    anchorId = id;
  }

  function toggleAll() {
    if (allSelected) {
      selected = new Set();
      anchorId = null;
    } else {
      selected = new Set(pages.map((p) => p.id));
    }
  }

  function clearSelection() {
    selected = new Set();
    anchorId = null;
  }

  /* ----------------------------------------------------------- page actions */

  function titleOf(id: string): string {
    const meta = app.library.find((p) => p.id === id) ?? app.pages.find((p) => p.id === id);
    return meta?.title || t("common.untitled");
  }

  /** First failure as "Title: reason", with a count of the rest. */
  function failureDetail(errors: BulkError[]): string {
    const first = errors[0];
    if (!first) return "";
    const rest = errors.length > 1 ? ` (+${errors.length - 1})` : "";
    return `${titleOf(first.id)}: ${first.error}${rest}`;
  }

  async function revealPage(p: PageMeta) {
    try {
      await api.revealPage(p.id);
    } catch (e) {
      console.error(e);
      toast(t("list.revealFailed", { detail: errorMessage(e) }), "error");
    }
  }

  function onOpenPage(e: MouseEvent, id: string) {
    if (e.ctrlKey || e.metaKey || e.shiftKey) {
      e.preventDefault();
      toggleSelect(id, e.shiftKey);
      return;
    }
    void openPage(id);
  }

  function parseTags(): string[] {
    return [...new Set(tagDraft.split(/[,\n]/).map((s) => s.trim()).filter(Boolean))];
  }

  async function bulkTags(mode: "add" | "remove") {
    const tags = parseTags();
    if (tags.length === 0) {
      toast(t("list.tagsNone"), "error");
      return;
    }
    const ids = selectedIds;
    try {
      const result = await api.bulkUpdate(ids, mode === "add" ? { addTags: tags } : { removeTags: tags });
      if (result.errors.length > 0) {
        toast(
          t("list.tagsPartial", {
            done: result.updated.length,
            failed: result.errors.length,
            detail: failureDetail(result.errors),
          }),
          "error",
        );
      } else {
        toast(t("list.tagsDone", { count: result.updated.length }), "success");
      }
      tagDraft = "";
      await reloadPages(true);
    } catch (e) {
      console.error(e);
      toast(t("list.tagsFailed", { detail: errorMessage(e) }), "error");
    }
  }

  async function bulkSchedule() {
    const ids = selectedIds;
    if (ids.length === 0) return;
    let done = 0;
    const errors: BulkError[] = [];
    for (const id of ids) {
      try {
        await api.scheduleReview(id, scheduleMinutes);
        done++;
      } catch (e) {
        console.error(e);
        errors.push({ id, error: errorMessage(e) });
      }
    }
    if (errors.length > 0) {
      toast(
        t("list.schedulePartial", { done, failed: errors.length, detail: failureDetail(errors) }),
        "error",
      );
    } else {
      toast(t("list.scheduled", { count: done }), "success");
    }
    await reloadPages(true);
  }

  /* ---------------------------------------------------------------- context */

  /** Right click anchors at the pointer; the context-menu key (clientX/Y = 0) at the row. */
  function anchor(e: MouseEvent): { x: number; y: number } {
    e.preventDefault();
    e.stopPropagation();
    if (e.clientX === 0 && e.clientY === 0 && e.currentTarget instanceof HTMLElement) {
      const rect = e.currentTarget.getBoundingClientRect();
      return { x: rect.left + 24, y: rect.bottom };
    }
    return { x: e.clientX, y: e.clientY };
  }

  function openPageMenu(e: MouseEvent, p: Row) {
    const at = anchor(e);
    menu = {
      ...at,
      items: [
        { id: "open", label: t("common.open"), icon: "file-text", onclick: () => void openPage(p.id) },
        { id: "move", label: t("list.move"), icon: "folder", onclick: () => void movePages([p.id]) },
        { id: "duplicate", label: t("list.duplicate"), icon: "file-plus", onclick: () => void duplicatePage(p.id) },
        { id: "reveal", label: t("list.reveal"), icon: "folder-open", onclick: () => void revealPage(p) },
        { divider: true },
        { id: "trash", label: t("list.trash"), icon: "trash-2", danger: true, onclick: () => void deletePages([p.id]) },
      ],
    };
  }

  /* ------------------------------------------------------------------- drag */

  function onDragStart(e: DragEvent, p: Row) {
    if (!e.dataTransfer) return;
    const ids = selected.has(p.id) ? selectedIds : [p.id];
    e.dataTransfer.setData("application/x-herbarium-page-ids", JSON.stringify(ids));
    e.dataTransfer.setData("text/plain", ids.map((id) => titleOf(id)).join(", "));
    e.dataTransfer.effectAllowed = "move";
  }

  /** Split a `[match]`-marked snippet into plain and highlighted runs. Text only — never HTML. */
  function snippetParts(snippet: string): Array<{ text: string; mark: boolean }> {
    const parts: Array<{ text: string; mark: boolean }> = [];
    let buffer = "";
    let mark = false;
    const flush = () => {
      if (buffer) parts.push({ text: buffer, mark });
      buffer = "";
    };
    for (const ch of snippet) {
      if (ch === "[") {
        flush();
        mark = true;
      } else if (ch === "]") {
        flush();
        mark = false;
      } else {
        buffer += ch;
      }
    }
    flush();
    return parts;
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
          <button type="button" class="btn btn-ghost btn-sm" onclick={clearFilters}>
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
          title={t("search.filtersHelp")}
          spellcheck="false"
          autocomplete="off"
        />
        {#if searchInput}
          <button type="button" class="clear" aria-label={t("list.clearSearch")} onclick={clearSearch}>
            <Icon name="x" size={11} />
          </button>
        {:else}
          <kbd class="kbd slash">/</kbd>
        {/if}
      </div>

      {#if saveName !== null}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="save-name"
          type="text"
          bind:value={saveName}
          onkeydown={onSaveNameKey}
          onblur={() => (saveName = null)}
          placeholder={t("search.namePlaceholder")}
          aria-label={t("search.nameLabel")}
          maxlength="80"
          autofocus
        />
      {:else if searching && !searchSaved}
        <button
          type="button"
          class="btn btn-ghost btn-sm"
          onclick={() => (saveName = app.search.trim())}
          title={t("search.saveHint")}
        >
          <Icon name="plus" size={12} />
          {t("search.save")}
        </button>
      {/if}

      <div class="tools">
        <Select
          value={sortValue}
          options={sortOptions}
          icon="arrow-up-down"
          ariaLabel={t("list.sort")}
          onchange={(val) => setSort(val)}
          size="sm"
        />

        <div class="seg" role="group" aria-label={t("list.layout")}>
          <button
            type="button"
            class="seg-btn"
            class:active={app.layout === "grid"}
            aria-pressed={app.layout === "grid"}
            title={t("list.grid")}
            onclick={() => setLayout("grid")}
          >
            <Icon name="layout-grid" size={14} />
          </button>
          <button
            type="button"
            class="seg-btn"
            class:active={app.layout === "list"}
            aria-pressed={app.layout === "list"}
            title={t("list.list")}
            onclick={() => setLayout("list")}
          >
            <Icon name="rows-3" size={14} />
          </button>
        </div>

        <button type="button" class="btn btn-primary" onclick={() => (app.importOpen = true)}>
          <Icon name="plus" size={13} />
          <span>{t("list.import")}</span>
        </button>
      </div>
    </div>

    {#if app.loadError && pages.length > 0}
      <div class="error-banner" role="status">
        <Icon name="info" size={14} />
        <span class="error-text">{t("list.loadFailed")} <em>{app.loadError}</em></span>
        <button type="button" class="btn btn-ghost btn-sm" onclick={() => void reloadPages()}>
          {t("list.retry")}
        </button>
      </div>
    {/if}

    {#if selectedIds.length > 0}
      <div class="bulk" role="group" aria-label={t("list.bulk")}>
        <label class="bulk-all">
          <input
            type="checkbox"
            bind:this={selectAllEl}
            checked={allSelected}
            onchange={toggleAll}
            aria-label={t("list.selectAll")}
          />
        </label>
        <span class="bulk-count">{t("list.selected", { count: selectedIds.length })}</span>
        <div class="bulk-actions">
          <button type="button" class="btn btn-sm" onclick={() => void movePages(selectedIds)}>
            <Icon name="folder" size={13} />
            {t("list.move")}
          </button>
          <div class="tag-input">
            <input
              type="text"
              bind:value={tagDraft}
              placeholder={t("list.tagsPlaceholder")}
              aria-label={t("list.tagsLabel")}
              spellcheck="false"
              autocomplete="off"
            />
          </div>
          <button type="button" class="btn btn-sm" onclick={() => void bulkTags("add")}>
            <Icon name="plus" size={12} />
            {t("list.addTags")}
          </button>
          <button type="button" class="btn btn-sm" onclick={() => void bulkTags("remove")}>
            <Icon name="x" size={12} />
            {t("list.removeTags")}
          </button>
          <div class="schedule">
            <Select
              value={scheduleMinutes}
              options={scheduleOptions}
              ariaLabel={t("list.scheduleLabel")}
              onchange={(val) => (scheduleMinutes = val)}
              size="sm"
              align="right"
              disabled={scheduleOptions.length === 0}
            />
            <button
              type="button"
              class="btn btn-sm"
              disabled={scheduleOptions.length === 0}
              onclick={() => void bulkSchedule()}
            >
              <Icon name="calendar-clock" size={13} />
              {t("list.schedule")}
            </button>
          </div>
          <button type="button" class="btn btn-sm btn-danger-soft" onclick={() => void deletePages(selectedIds)}>
            <Icon name="trash-2" size={13} />
            {t("list.trash")}
          </button>
          <button type="button" class="btn btn-ghost btn-sm" onclick={clearSelection}>
            {t("list.clearSelection")}
          </button>
        </div>
      </div>
    {/if}

    {#if app.busy && app.pages.length === 0}
      <div class="grid" class:list={app.layout === "list"} aria-hidden="true">
        {#each Array(6) as _, i (i)}
          <div class="skel"></div>
        {/each}
      </div>
    {:else if app.loadError && pages.length === 0}
      <div class="empty">
        <span class="empty-icon danger"><Icon name="info" size={20} /></span>
        <strong>{t("list.loadFailed")}</strong>
        <span class="detail">{app.loadError}</span>
        <button type="button" class="btn btn-sm" onclick={() => void reloadPages()}>
          {t("list.retry")}
        </button>
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
          <button type="button" class="btn btn-sm" onclick={clearSearch}>{t("list.clearSearch")}</button>
        {:else if filtersActive}
          <button type="button" class="btn btn-sm" onclick={clearFilters}>{t("list.clearFilters")}</button>
        {:else}
          <button type="button" class="btn btn-primary btn-sm" onclick={() => (app.importOpen = true)}>
            <Icon name="plus" size={12} />
            {t("list.importPage")}
          </button>
        {/if}
      </div>
    {:else}
      <div class="grid" class:list={app.layout === "list"}>
        {#each pages as p (p.id)}
          {@const due = dueInfo(p.nextReview)}
          {@const selectedRow = selected.has(p.id)}
          <article
            class="page"
            class:selected={selectedRow}
            use:reveal
            draggable="true"
            ondragstart={(e) => onDragStart(e, p)}
            oncontextmenu={(e) => openPageMenu(e, p)}
          >
            <button
              type="button"
              class="open-overlay"
              aria-label={p.title || t("common.untitled")}
              onclick={(e) => onOpenPage(e, p.id)}
            ></button>
            {#if app.layout === "grid" && app.previews[p.id]}
              <div class="preview" aria-hidden="true">
                <Miniature digest={app.previews[p.id]} />
              </div>
            {/if}
            <div class="page-open">
              <span class="thumb" data-color={folderLook(p.folder).color}>
                {#if p.ext?.look?.icon}<span class="thumb-emoji">{p.ext.look.icon}</span>{:else}<Icon name="leaf" size={15} />{/if}
              </span>
              <span class="page-body">
                <span class="page-title">{p.title || t("common.untitled")}</span>
                {#if searching && p.snippet}
                  <span class="note snippet">
                    {#each snippetParts(p.snippet) as part, i (i)}
                      {#if part.mark}<mark>{part.text}</mark>{:else}{part.text}{/if}
                    {/each}
                  </span>
                {:else if p.note}
                  <span class="note">{p.note}</span>
                {/if}
                {#if p.folder || p.tags.length > 0}
                  <span class="meta">
                    {#if p.folder}
                      <button
                        type="button"
                        class="loc chip-link"
                        style:color={folderLook(p.folder).color ? `var(--c-${folderLook(p.folder).color})` : undefined}
                        title={t("list.showFolder", { folder: p.folder })}
                        onclick={() => void showFolder(p.folder!)}
                      >
                        <Icon name="folder" size={11} />{p.folder}
                      </button>
                    {/if}
                    {#each p.tags.slice(0, 3) as tag (tag)}
                      <button
                        type="button"
                        class="chip chip-muted chip-link"
                        data-color={tagColor(tag)}
                        title={t("list.showTag", { tag })}
                        onclick={() => void showTag(tag)}
                        oncontextmenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          app.lookEdit = { kind: "tag", key: tag };
                        }}
                      >
                        {tag}
                      </button>
                    {/each}
                    {#if p.tags.length > 3}
                      <span class="chip chip-muted">+{p.tags.length - 3}</span>
                    {/if}
                  </span>
                {/if}
              </span>
            </div>
            <span class="page-side">
              <label class="check">
                <input
                  type="checkbox"
                  checked={selectedRow}
                  onclick={(e) => toggleSelect(p.id, e.shiftKey)}
                  aria-label={t("list.select", { title: p.title || t("common.untitled") })}
                />
              </label>
              {#if due}
                <span
                  class="chip due"
                  class:chip-warn={due.hot && !due.overdue}
                  class:chip-danger={due.overdue}
                  class:chip-muted={!due.hot && !due.overdue}
                >
                  {due.label}
                </span>
              {/if}
              <button
                type="button"
                class="btn btn-ghost btn-sm btn-icon del"
                aria-label={t("list.delete", { title: p.title || t("common.untitled") })}
                onclick={() => void deletePages([p.id])}
              >
                <Icon name="trash-2" size={14} />
              </button>
            </span>
          </article>
        {/each}
      </div>
    {/if}
  </div>
</section>

{#if menu}
  <ContextMenu
    x={menu.x}
    y={menu.y}
    items={menu.items}
    ariaLabel={t("list.menu")}
    onclose={() => (menu = null)}
  />
{/if}

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
  .save-name {
    width: 200px;
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

  /* ---- Load failure, distinct from an empty library ---- */
  .error-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    margin-bottom: -8px;
    border: 1px solid var(--danger);
    border-radius: var(--radius-sm);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--fs-sm);
  }
  .error-banner .error-text {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 6px;
    align-items: baseline;
    flex-wrap: wrap;
  }
  .error-banner em {
    font-style: normal;
    opacity: 0.85;
    overflow-wrap: anywhere;
  }
  .empty .detail {
    color: var(--text);
    opacity: 0.8;
    overflow-wrap: anywhere;
    max-width: 44ch;
  }
  .empty .danger {
    background: var(--danger-soft);
    color: var(--danger);
  }

  /* ---- Bulk selection toolbar ---- */
  .bulk {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }
  .bulk-all,
  .check {
    display: inline-flex;
    align-items: center;
  }
  .bulk-all input,
  .check input {
    width: 15px;
    height: 15px;
    accent-color: var(--leaf);
    cursor: pointer;
  }
  .bulk-count {
    font-size: var(--fs-sm);
    color: var(--muted);
    white-space: nowrap;
  }
  .bulk-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-left: auto;
  }
  .tag-input input {
    width: 160px;
    padding: 4px 8px;
    font-size: var(--fs-sm);
    background: var(--surface);
  }
  .schedule {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .btn-danger-soft {
    color: var(--danger);
  }
  .btn-danger-soft:hover {
    background: var(--danger-soft);
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
  .page.selected {
    border-color: var(--leaf);
    box-shadow: 0 0 0 1px var(--leaf);
  }
  .page.selected .page-open {
    background: var(--leaf-soft);
  }
  .open-overlay {
    position: absolute;
    inset: 0;
    z-index: 0;
    border: none;
    background: transparent;
    border-radius: inherit;
    cursor: pointer;
  }
  .open-overlay:focus-visible {
    outline: 2px solid var(--leaf);
    outline-offset: -2px;
  }
  .page-open {
    position: relative;
    z-index: 1;
    pointer-events: none;
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
    overflow-wrap: anywhere;
  }
  .snippet mark {
    background: var(--leaf-soft);
    color: var(--text);
    border-radius: 2px;
    padding: 0 1px;
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: auto;
    padding-top: 6px;
  }
  .page-open .meta {
    pointer-events: none;
  }
  .loc {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-right: 2px;
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .chip-link {
    pointer-events: auto;
    cursor: pointer;
    border: none;
    background: none;
    font: inherit;
  }
  .chip-link:hover {
    color: var(--leaf);
  }
  button.chip {
    padding: 1px 6px;
  }
  .page-side {
    position: absolute;
    z-index: 2;
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
  .check {
    margin-right: 2px;
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
  .page.selected .del {
    opacity: 1;
  }
  .del:hover {
    color: var(--danger);
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
  .list .page.selected {
    box-shadow: none;
    background: var(--leaf-soft);
  }
  .list .page.selected .page-open {
    background: transparent;
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
  .thumb[data-color] {
    background: var(--chip-wash);
    color: var(--chip-ink);
  }
  .thumb-emoji {
    font-size: 16px;
    line-height: 1;
  }
  .preview {
    height: 128px;
    margin: -1px -1px 0;
    border-bottom: 1px solid var(--border);
    border-radius: var(--radius) var(--radius) 0 0;
    overflow: hidden;
    color: var(--text);
    pointer-events: none;
  }
  .list .preview {
    display: none;
  }
</style>
