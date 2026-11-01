<script lang="ts" module>
  /** The cascading entrance plays once per session; coming back to the list just shows it. */
  let entered = false;
</script>

<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { flip } from "svelte/animate";
  import { fade, fly } from "svelte/transition";
  import { backOut, cubicIn, cubicOut } from "svelte/easing";
  import { Tween } from "svelte/motion";
  import {
    app,
    visiblePages,
    reloadPages,
    searchLibrary,
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
  import Illustration from "../lib/Illustration.svelte";
  import { reducedMotion } from "../lib/motion";
  import VitalityLeaf from "./VitalityLeaf.svelte";
  import { folderLook, tagColor } from "../lib/appearance";
  import Miniature from "./Miniature.svelte";
  import Select from "./Select.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import type { DropdownMenuItem } from "./DropdownMenu.svelte";
  import { dueInfo, fmtDuration, plural, startOfToday } from "../lib/format";
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

  /* Windowed rendering: a big library draws the first cards at once and
     the rest as they come near the viewport, so searching, filtering and
     switching views stay instant whatever the vault's size. */
  const WINDOW = 24;
  let limit = $state(WINDOW);
  const shown = $derived(pages.length > limit ? pages.slice(0, limit) : pages);
  let sentinel: HTMLDivElement | undefined = $state();

  // A new search, filter, sort or layout starts from the top.
  $effect(() => {
    void app.search;
    void app.folderFilter;
    void app.tagFilter;
    void app.sort;
    void app.layout;
    limit = WINDOW;
  });

  $effect(() => {
    const el = sentinel;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) limit += WINDOW * 2;
      },
      { rootMargin: "1200px 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  });
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

  /* ------------------------------------------------------------ choreography */

  /* Cards enter in a short cascade the first time the list shows and whenever a
     folder or tag filter changes. Anything else that adds a card (typing in the
     search, loading more) only fades it in, and a layout switch crossfades the
     whole grid instead. The mode is read once, when each card mounts. */
  let entryMode: "stagger" | "fresh" = entered ? "fresh" : "stagger";
  let quiet = false;
  let entryTimer: ReturnType<typeof setTimeout> | undefined;
  let quietTimer: ReturnType<typeof setTimeout> | undefined;

  function armCascade() {
    entryMode = "stagger";
    clearTimeout(entryTimer);
    entryTimer = setTimeout(() => {
      entryMode = "fresh";
      entered = true;
    }, 900);
  }

  // First cards of the session: cascade them, then settle.
  $effect(() => {
    if (!entered && shown.length > 0) armCascade();
  });

  let filterKey: string | null = null;
  $effect.pre(() => {
    const key = `${app.folderFilter ?? ""}\u0000${app.tagFilter ?? ""}`;
    if (filterKey !== null && key !== filterKey) armCascade();
    filterKey = key;
  });

  let layoutSeen = app.layout;
  $effect.pre(() => {
    const layout = app.layout;
    if (layout === layoutSeen) return;
    layoutSeen = layout;
    quiet = true;
    clearTimeout(quietTimer);
    quietTimer = setTimeout(() => (quiet = false), 400);
  });

  /** Marks a card as it mounts; later re-renders of the same card never replay it. */
  function enter(node: HTMLElement, index: number) {
    if (quiet) return;
    if (entryMode === "stagger") {
      node.style.setProperty("--i", String(Math.min(index, 10)));
      node.classList.add("enter");
    } else {
      node.classList.add("fresh");
    }
  }

  /** Cards slide to their new place on reorder; a very large list just snaps. */
  const flipMs = $derived(shown.length > 100 || reducedMotion() ? 0 : 220);
  function glide(node: Element, pos: { from: DOMRect; to: DOMRect }) {
    return flip(node, pos, { duration: flipMs, easing: cubicOut });
  }

  /** Grid and list trade places by crossfading; the leaving one stops taking clicks. */
  function swap(node: Element, { duration = 160 }: { duration?: number } = {}) {
    return {
      duration: reducedMotion() ? 0 : duration,
      css: (t: number) => `opacity: ${t}; pointer-events: none`,
    };
  }

  const barIn = () => ({ y: 28, duration: reducedMotion() ? 0 : 300, easing: backOut });
  const barOut = () => ({ y: 20, duration: reducedMotion() ? 0 : 160, easing: cubicIn });

  // The count eases between values instead of jumping, but never counts up from zero on load.
  const countTween = new Tween(0, { easing: cubicOut });
  $effect(() => {
    const n = pages.length;
    const from = untrack(() => countTween.current);
    void countTween.set(n, { duration: from === 0 || reducedMotion() ? 0 : 220 });
  });

  onDestroy(() => {
    clearTimeout(entryTimer);
    clearTimeout(quietTimer);
  });

  /* ------------------------------------------------------------ search marks */

  /** Words worth highlighting: free terms, not `tag:x` filters or `-excluded` words. */
  const termRe = $derived.by(() => {
    const terms = [...app.search.matchAll(/"([^"]+)"|(\S+)/g)]
      .map((m) => m[1] ?? m[2])
      .filter((w) => w && !w.startsWith("-") && !/^[a-z]+:/i.test(w))
      .map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    if (terms.length === 0) return null;
    terms.sort((a, b) => b.length - a.length);
    return new RegExp(`(${terms.join("|")})`, "gi");
  });

  function markParts(text: string): Array<{ text: string; mark: boolean }> {
    if (!searching || !termRe) return [{ text, mark: false }];
    // The pattern's one capture group puts every match at an odd index.
    return text
      .split(termRe)
      .map((s, i) => ({ text: s, mark: i % 2 === 1 }))
      .filter((part) => part.text);
  }

  function onSearch() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      app.search = searchInput.trim();
      lastSearch = app.search;
      void searchLibrary();
    }, 120);
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
    void searchLibrary();
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

  /* ------------------------------------------------------- quick actions, keys */

  const label = (p: PageMeta) => p.title || t("common.untitled");

  /** Cards due today read amber, earlier than today rose, later muted. */
  function dueTone(next: number | null | undefined): "rose" | "amber" | "muted" {
    if (next == null) return "muted";
    const today = startOfToday();
    if (next < today) return "rose";
    return next < today + 86_400_000 ? "amber" : "muted";
  }

  async function scheduleOne(id: string, minutes: number) {
    try {
      await api.scheduleReview(id, minutes);
      toast(t("list.scheduled", { count: 1 }), "success");
      await reloadPages(true);
    } catch (e) {
      console.error(e);
      toast(t("list.scheduleFailed", { detail: errorMessage(e) }), "error");
    }
  }

  function openScheduleMenu(at: { x: number; y: number }, p: Row) {
    if (app.review.presets.length === 0) return;
    menu = {
      ...at,
      items: [
        { header: true, label: t("list.scheduleLabel") },
        ...app.review.presets.map((m) => ({
          id: `in-${m}`,
          label: fmtDuration(m),
          icon: "calendar-clock" as const,
          onclick: () => void scheduleOne(p.id, m),
        })),
      ],
    };
  }

  /** Nearest card in a direction: along the sequence for left/right, by column across rows for up/down. */
  function neighbour(cards: HTMLElement[], from: number, dir: "up" | "down" | "left" | "right"): HTMLElement | undefined {
    if (dir === "left") return cards[from - 1];
    if (dir === "right") return cards[from + 1];
    const step = dir === "down" ? 1 : -1;
    const here = cards[from].getBoundingClientRect();
    const cx = here.left + here.width / 2;
    let rowTop: number | null = null;
    let best: HTMLElement | undefined;
    let bestGap = Infinity;
    for (let i = from + step; i >= 0 && i < cards.length; i += step) {
      const r = cards[i].getBoundingClientRect();
      if (Math.abs(r.top - here.top) < 4) continue;
      if (rowTop === null) rowTop = r.top;
      else if (Math.abs(r.top - rowTop) >= 4) break;
      const gap = Math.abs(r.left + r.width / 2 - cx);
      if (gap < bestGap) {
        bestGap = gap;
        best = cards[i];
      }
    }
    return best;
  }

  /** Keys on a focused card: arrows move between cards, X selects, S schedules, Delete trashes. */
  function onCardKey(e: KeyboardEvent, p: Row) {
    const target = e.target as HTMLElement;
    if (!target.classList.contains("open-overlay") || e.ctrlKey || e.metaKey || e.altKey) return;
    const dir = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" }[e.key] as
      | "up"
      | "down"
      | "left"
      | "right"
      | undefined;
    if (dir) {
      const grid = (e.currentTarget as HTMLElement).parentElement;
      const cards = [...(grid?.querySelectorAll<HTMLElement>(".open-overlay") ?? [])];
      const next = neighbour(cards, cards.indexOf(target), dir);
      e.preventDefault();
      next?.focus();
    } else if (e.key.toLowerCase() === "x" && !e.shiftKey) {
      e.preventDefault();
      toggleSelect(p.id, false);
    } else if (e.key.toLowerCase() === "s" && !e.shiftKey) {
      e.preventDefault();
      const r = target.getBoundingClientRect();
      openScheduleMenu({ x: r.left + 24, y: r.bottom }, p);
    } else if (e.key === "Delete") {
      e.preventDefault();
      void deletePages([p.id]);
    } else if (e.key === "Escape" && selected.size > 0) {
      e.stopPropagation();
      clearSelection();
    }
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.defaultPrevented || !(e.ctrlKey || e.metaKey) || e.shiftKey || e.altKey) return;
    if (e.key.toLowerCase() !== "a" || pages.length === 0) return;
    const el = e.target as HTMLElement | null;
    if (el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    e.preventDefault();
    selected = new Set(pages.map((p) => p.id));
  }
</script>

<svelte:window onkeydown={onWindowKey} />

{#snippet marked(text: string)}
  {#each markParts(text) as part, i (i)}{#if part.mark}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
{/snippet}

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
          <span class="count">{plural(Math.round(countTween.current), "page")}</span>
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
      <div class="empty" in:fade={{ duration: reducedMotion() ? 0 : 160 }}>
        <Illustration
          name={app.search ? "magnifier-leaf" : filtersActive ? "empty-pot" : "seedling"}
          size={112}
          animate
        />
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
        <div class="empty-actions">
          {#if app.search}
            <button type="button" class="btn btn-sm" onclick={clearSearch}>{t("list.clearSearch")}</button>
          {/if}
          {#if filtersActive}
            <button type="button" class="btn btn-sm" onclick={clearFilters}>{t("list.clearFilters")}</button>
          {:else if !app.search}
            <button type="button" class="btn btn-primary btn-sm" onclick={() => (app.importOpen = true)}>
              <Icon name="plus" size={12} />
              {t("list.importPage")}
              <kbd class="kbd">I</kbd>
            </button>
          {/if}
        </div>
      </div>
    {:else}
      <div class="stage">
        {#key app.layout}
          <div
            class="grid"
            class:list={app.layout === "list"}
            class:selecting={selected.size > 0}
            in:swap
            out:swap
          >
            {#each shown as p, i (p.id)}
              {@const due = dueInfo(p.nextReview)}
              {@const tone = dueTone(p.nextReview)}
              {@const selectedRow = selected.has(p.id)}
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <article
                class="page"
                class:selected={selectedRow}
                use:enter={i}
                animate:glide
                draggable="true"
                ondragstart={(e) => onDragStart(e, p)}
                oncontextmenu={(e) => openPageMenu(e, p)}
                onkeydown={(e) => onCardKey(e, p)}
              >
                <button
                  type="button"
                  class="open-overlay"
                  aria-label={label(p)}
                  onclick={(e) => onOpenPage(e, p.id)}
                ></button>
                {#if app.layout === "grid"}
                  <div class="preview" class:no-preview={!app.previews[p.id]} data-color={folderLook(p.folder).color} aria-hidden="true">
                    <div class="preview-inner">
                      {#if app.previews[p.id]}
                        <Miniature digest={app.previews[p.id]} />
                      {:else if p.ext?.look?.icon}
                        <span class="placeholder-emoji">{p.ext.look.icon}</span>
                      {:else}
                        <Icon name="leaf" size={22} />
                      {/if}
                    </div>
                  </div>
                {/if}
                <div class="page-open">
                  <span class="thumb" data-color={folderLook(p.folder).color}>
                    {#if p.ext?.look?.icon}<span class="thumb-emoji">{p.ext.look.icon}</span>{:else}<Icon name="leaf" size={15} />{/if}
                  </span>
                  <span class="page-body">
                    <span class="page-title">{@render marked(label(p))}</span>
                    {#if searching && p.snippet}
                      <span class="note snippet">
                        {#each snippetParts(p.snippet) as part, j (j)}
                          {#if part.mark}<mark>{part.text}</mark>{:else}{part.text}{/if}
                        {/each}
                      </span>
                    {:else if p.note}
                      <span class="note">{@render marked(p.note)}</span>
                    {/if}
                    {#if app.layout === "grid" || p.folder || p.tags.length > 0}
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
                        {#if app.layout === "grid"}
                          <span class="leaf-slot"><VitalityLeaf meta={p} size={16} /></span>
                        {/if}
                      </span>
                    {/if}
                  </span>
                </div>
                <span class="page-side">
                  <label class="check">
                    <input
                      type="checkbox"
                      class="cbx"
                      checked={selectedRow}
                      onclick={(e) => toggleSelect(p.id, e.shiftKey)}
                      aria-label={t("list.select", { title: label(p) })}
                    />
                  </label>
                  {#if app.layout === "list"}
                    <span class="leaf-col"><VitalityLeaf meta={p} size={16} /></span>
                  {/if}
                  <span class="due-slot">
                    {#if due}
                      <span
                        class="chip due"
                        class:chip-muted={tone === "muted"}
                        data-color={tone === "muted" ? undefined : tone}
                      >
                        {due.label}
                      </span>
                    {/if}
                  </span>
                </span>
                <span class="quick">
                  <button
                    type="button"
                    class="btn btn-sm btn-icon"
                    aria-label={t("list.scheduleFor", { title: label(p) })}
                    title={`${t("list.schedule")} (S)`}
                    aria-haspopup="menu"
                    disabled={app.review.presets.length === 0}
                    onclick={(e) => openScheduleMenu(anchor(e), p)}
                  >
                    <Icon name="calendar-clock" size={14} />
                  </button>
                  <button
                    type="button"
                    class="btn btn-sm btn-icon"
                    aria-label={t("list.moreFor", { title: label(p) })}
                    title={t("list.menu")}
                    aria-haspopup="menu"
                    onclick={(e) => openPageMenu(e, p)}
                  >
                    <Icon name="dots-three" size={14} />
                  </button>
                </span>
              </article>
            {/each}
          </div>
        {/key}
      </div>
      {#if shown.length < pages.length}
        <div class="more" bind:this={sentinel} aria-hidden="true"></div>
      {/if}
    {/if}

    {#if selectedIds.length > 0}
      <div class="bulk" role="group" aria-label={t("list.bulk")} in:fly={barIn()} out:fly={barOut()}>
        <label class="bulk-all">
          <input
            type="checkbox"
            class="cbx"
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
    font-size: var(--fs-3xl);
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
    font-variant-numeric: tabular-nums;
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
  /* Floats at the bottom of the pane while pages are selected. */
  .bulk {
    position: sticky;
    bottom: 16px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 10px 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .bulk-all,
  .check {
    display: inline-flex;
    align-items: center;
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
    width: 124px;
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
  /* Grid and list sit in the same cell so a layout switch can crossfade them. */
  .stage {
    display: grid;
  }
  .grid {
    grid-area: 1 / 1;
    min-width: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 16px;
    align-content: start;
  }

  .more {
    height: 1px;
  }
  .page {
    position: relative;
    /* Off-screen cards skip layout and paint. */
    content-visibility: auto;
    contain-intrinsic-size: auto 240px;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: none;
    transition:
      border-color var(--t-med) var(--ease-out),
      box-shadow var(--t-med) var(--ease-out),
      background var(--t-med) var(--ease-out),
      transform var(--t-med) var(--ease-out);
  }
  .page:hover {
    border-color: var(--border-hover);
    box-shadow: var(--shadow);
  }
  .grid:not(.list) .page:hover {
    transform: translateY(-2px);
  }

  /* First load and filter changes cascade in; any other new card only fades. */
  .page:global(.enter) {
    animation: card-in var(--t-enter) var(--ease-out) backwards;
    animation-delay: calc(var(--i, 0) * 40ms);
  }
  .page:global(.fresh) {
    animation: card-fade var(--t-fast) var(--ease-out) backwards;
  }
  @keyframes card-in {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
  }
  @keyframes card-fade {
    from {
      opacity: 0;
    }
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
    gap: 10px;
    padding: 14px 16px 16px;
    text-align: left;
    border-radius: inherit;
  }
  /* The grid shows the page itself; the icon lives in its preview. */
  .grid:not(.list) .thumb {
    display: none;
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
    font-size: 17px;
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
  /* Matched search terms: tinted without touching metrics, so lines never reflow. */
  mark {
    background: var(--warn-soft);
    color: var(--text);
    border-radius: 2px;
    padding: 0;
    box-shadow:
      0 0 0 1px var(--warn-soft),
      inset 0 -1px 0 var(--warn-border);
    -webkit-box-decoration-break: clone;
    box-decoration-break: clone;
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
    top: 8px;
    right: 8px;
    left: 8px;
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
    margin-right: auto;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    opacity: 0;
    transition: opacity var(--t-fast) var(--ease-out);
  }
  .page:hover .check,
  .page:focus-within .check,
  .page.selected .check,
  .selecting .check {
    opacity: 1;
  }

  /* Checkbox: the tick springs in. */
  .cbx {
    appearance: none;
    -webkit-appearance: none;
    position: relative;
    width: 15px;
    height: 15px;
    margin: 0;
    border: 1px solid var(--border-input);
    border-radius: 4px;
    background: var(--surface);
    cursor: pointer;
    transition:
      background var(--t-fast) var(--ease-out),
      border-color var(--t-fast) var(--ease-out);
  }
  .cbx:hover {
    border-color: var(--border-input-hover);
  }
  .cbx:checked,
  .cbx:indeterminate {
    background: var(--leaf);
    border-color: var(--leaf);
  }
  .cbx::after {
    content: "";
    position: absolute;
    inset: 1px;
    background: var(--on-accent);
    -webkit-mask: var(--tick) center / 11px no-repeat;
    mask: var(--tick) center / 11px no-repeat;
    --tick: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 256 256'%3E%3Cpath d='M232.49 80.49l-128 128a12 12 0 0 1-17 0l-56-56a12 12 0 1 1 17-17L96 183 215.51 63.51a12 12 0 0 1 17 17Z'/%3E%3C/svg%3E");
    transform: scale(0);
    transition: transform 300ms var(--ease-spring);
  }
  .cbx:checked::after,
  .cbx:indeterminate::after {
    transform: scale(1);
  }
  .cbx:indeterminate::after {
    --tick: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 256 256'%3E%3Cpath d='M228 128a12 12 0 0 1-12 12H40a12 12 0 0 1 0-24h176a12 12 0 0 1 12 12Z'/%3E%3C/svg%3E");
  }

  /* Due chips: rose when overdue, amber when due today, muted otherwise. */
  .due {
    font-family: var(--mono);
    font-size: var(--fs-2xs);
    letter-spacing: 0;
    text-transform: none;
  }
  .due-slot {
    display: flex;
    justify-content: flex-end;
  }
  .leaf-slot {
    margin-left: auto;
    display: flex;
    align-items: center;
    pointer-events: auto;
  }
  .leaf-col {
    width: 20px;
    display: grid;
    place-items: center;
  }

  /* Quick actions rise in over the preview's corner on hover or focus. */
  .quick {
    position: absolute;
    z-index: 2;
    right: 8px;
    top: 84px;
    display: flex;
    gap: 4px;
    opacity: 0;
    transform: translateY(3px);
    pointer-events: none;
    transition:
      opacity var(--t-fast) var(--ease-out),
      transform var(--t-fast) var(--ease-out);
  }
  .page:hover .quick,
  .page:focus-within .quick {
    opacity: 1;
    transform: none;
    pointer-events: auto;
  }
  .quick .btn {
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-soft);
  }
  .quick .btn:hover:not(:disabled) {
    background: var(--sunken);
    color: var(--text);
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
  .list .loc {
    font-family: var(--mono);
    font-size: var(--fs-2xs);
  }
  .list .page-side {
    position: static;
    flex: none;
    gap: 10px;
    padding-right: 4px;
  }
  .list .due-slot {
    width: 124px;
  }
  .list .quick {
    position: static;
    flex: none;
    transform: none;
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
    height: 120px;
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
  /* The frame clips; the picture inside eases up a hair when the card is hovered. */
  .preview-inner {
    width: 100%;
    height: 100%;
    transition: transform var(--t-med) var(--ease-out);
  }
  .grid:not(.list) .page:hover .preview-inner {
    transform: scale(1.02);
  }
  .preview.no-preview {
    background: var(--sunken);
    color: var(--muted);
  }
  .no-preview .preview-inner {
    display: grid;
    place-items: center;
  }
  .preview.no-preview[data-color] {
    background: var(--chip-wash);
    color: var(--chip-ink);
  }
  .placeholder-emoji {
    font-size: 30px;
    line-height: 1;
  }
  .empty-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    margin-top: 12px;
  }
  .empty-actions .btn {
    margin-top: 0;
  }
  .empty-actions .kbd {
    margin-left: 2px;
    background: transparent;
    border-color: currentColor;
    color: inherit;
    opacity: 0.6;
  }
</style>
