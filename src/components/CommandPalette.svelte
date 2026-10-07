<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import {
    app,
    setLayout,
    toggleTheme,
    toggleSidebar,
    dueLabel,
    errorMessage,
    toast,
    reloadPages,
    rescanVault,
    openPage,
    goView,
    goAll,
    showFolder,
    showTag,
    requestCreate,
    startReviewSession,
    movePages,
    deletePages,
    duplicatePage,
  } from "../lib/state.svelte";
  import { t, otherLocale, toggleLocale, LOCALES } from "../lib/i18n.svelte";
  import { api } from "../lib/api";
  import { confirmAction } from "../lib/confirm.svelte";
  import { currentEditor, loadEditors } from "../lib/editors.svelte";
  import { fmtDuration } from "../lib/format";
  import Icon from "../lib/Icon.svelte";
  import type { IconName } from "../lib/icons";
  import type { PageMeta, ReviewGrade, SearchHit } from "../lib/types";

  interface Item {
    key: string;
    group: string;
    label: string;
    hint?: string;
    icon: IconName;
    run: () => void | Promise<unknown>;
  }

  let q = $state("");
  let active = $state(0);
  let inputEl: HTMLInputElement | undefined;
  let prevFocus: HTMLElement | null = null;

  // Backend full-text results, tagged with the query they answer, so a slow
  // response can never overwrite a newer one and stale hits never render.
  let results = $state<{ query: string; hits: SearchHit[] }>({ query: "", hits: [] });
  let searchSeq = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  const needle = $derived(q.trim());
  const searching = $derived(needle !== "" && results.query !== needle);

  $effect(() => {
    const query = needle;
    clearTimeout(debounceTimer);
    const seq = ++searchSeq;
    if (query === "") {
      results = { query: "", hits: [] };
      return;
    }
    debounceTimer = setTimeout(() => {
      void api
        .searchPages(query)
        .then((hits) => {
          if (seq === searchSeq) results = { query, hits };
        })
        .catch((e) => {
          console.error(e);
          if (seq === searchSeq) results = { query, hits: [] };
        });
    }, 160);
  });

  const currentPage = $derived(
    app.readId
      ? (app.library.find((p) => p.id === app.readId) ??
        app.pages.find((p) => p.id === app.readId) ??
        null)
      : null,
  );
  const editor = $derived(currentEditor());

  async function revealPage(page: PageMeta) {
    try {
      await api.revealPage(page.id);
    } catch (e) {
      toast(t("list.revealFailed", { detail: errorMessage(e) }), "error");
    }
  }

  async function openExternally(page: PageMeta) {
    await loadEditors();
    const choice = currentEditor();
    if (!choice) {
      toast(t("sidebar.noEditor"), "error");
      return;
    }
    try {
      const name = await api.openInEditor(page.id, choice.id, choice.custom);
      toast(t("edit.openedIn", { editor: name }), "success");
    } catch (e) {
      toast(`${t("edit.openFailed")}: ${errorMessage(e)}`, "error");
    }
  }

  /** Whether the page saved anything (its `ext.storage` has at least one key). */
  function hasStoredData(meta: PageMeta): boolean {
    const raw = (meta.ext as Record<string, unknown> | undefined)?.["storage"];
    if (!raw || typeof raw !== "object") return false;
    for (const area of ["local", "personal", "shared"]) {
      const map = (raw as Record<string, unknown>)[area];
      if (map && typeof map === "object" && Object.keys(map).length > 0) return true;
    }
    return false;
  }

  async function resetStorage(page: PageMeta) {
    const ok = await confirmAction({
      title: t("read.resetStorageTitle"),
      message: t("read.resetStorageMessage"),
      confirmLabel: t("read.resetStorageConfirm"),
      danger: true,
    });
    if (!ok) return;
    try {
      await api.clearStorage(page.id);
      // The reader watches `vaultRevision`; it reloads the open frame with the
      // cleared state on the next refresh.
      app.vaultRevision++;
      toast(t("read.resetDone"), "success");
    } catch (e) {
      toast(`${t("read.resetFailed")}: ${errorMessage(e)}`, "error");
    }
  }

  async function gradePage(page: PageMeta, grade: ReviewGrade) {
    try {
      const meta = await api.completeReview(page.id, grade);
      toast(t("toast.reviewed", { when: fmtDuration(meta.intervalMinutes ?? 1) }), "success");
      void reloadPages(true);
    } catch (e) {
      console.error(e);
      toast(t("read.scheduleFailed"), "error");
    }
  }

  async function rescan() {
    try {
      const report = await rescanVault();
      toast(
        t("settings.rescanned", { indexed: report.indexed, removed: report.removed }),
        "success",
      );
    } catch (e) {
      console.error(e);
      toast(t("settings.rescanFailed"), "error");
    }
  }

  const actions = $derived.by<Item[]>(() => {
    const group = t("palette.actions");
    const list: Item[] = [
      {
        key: "new-page",
        group,
        label: t("palette.newPage"),
        icon: "file-plus",
        run: () => requestCreate("page", app.folderFilter ?? ""),
      },
      {
        key: "new-folder",
        group,
        label: t("palette.newFolder"),
        icon: "folder-plus",
        run: () => requestCreate("folder", app.folderFilter ?? ""),
      },
      {
        key: "import",
        group,
        label: t("palette.import"),
        hint: t("palette.importHint"),
        icon: "upload",
        run: () => {
          app.importOpen = true;
        },
      },
      {
        key: "import-ai",
        group,
        label: t("aiImport.title"),
        hint: t("aiImport.paletteHint"),
        icon: "files",
        run: () => {
          app.aiImportOpen = true;
        },
      },
      {
        key: "rescan",
        group,
        label: t("palette.rescan"),
        icon: "refresh-cw",
        run: rescan,
      },
      {
        key: "review",
        group,
        label: t("palette.goReview"),
        hint: dueLabel(),
        icon: "calendar-clock",
        run: () => goView("review"),
      },
      {
        key: "review-session",
        group,
        label: t("palette.startReview"),
        icon: "circle-check",
        run: () => startReviewSession(),
      },
      {
        key: "all",
        group,
        label: t("palette.goAll"),
        icon: "files",
        run: () => goAll(),
      },
      {
        key: "trash",
        group,
        label: t("palette.goTrash"),
        icon: "trash-2",
        run: () => goView("trash"),
      },
      {
        key: "settings",
        group,
        label: t("palette.goSettings"),
        icon: "settings",
        run: () => goView("settings"),
      },
      {
        key: "sidebar",
        group,
        label: t("palette.toggleSidebar"),
        icon: "rows-3",
        run: toggleSidebar,
      },
      {
        key: "details",
        group,
        label: t("palette.toggleDetails"),
        icon: "layout-grid",
        run: () => {
          app.inspectorOpen = !app.inspectorOpen;
        },
      },
      {
        key: "lang",
        group,
        label: t("prefs.switchLang", { language: LOCALES[otherLocale()].name }),
        icon: "languages",
        run: toggleLocale,
      },
      {
        key: "theme",
        group,
        label: app.theme === "dark" ? t("prefs.themeLight") : t("prefs.themeDark"),
        icon: app.theme === "dark" ? "sun" : "moon",
        run: toggleTheme,
      },
      {
        key: "layout",
        group,
        label: app.layout === "grid" ? t("palette.toList") : t("palette.toGrid"),
        icon: app.layout === "grid" ? "rows-3" : "layout-grid",
        run: () => setLayout(app.layout === "grid" ? "list" : "grid"),
      },
    ];
    if (app.folderFilter || app.tagFilter) {
      list.push({
        key: "clear-filters",
        group,
        label: t("palette.clearFilters"),
        icon: "x",
        run: () => goAll(),
      });
    }
    const page = currentPage;
    if (page) {
      const pg = t("palette.current");
      list.push(
        {
          key: "page-move",
          group: pg,
          label: t("palette.move", { title: page.title || t("common.untitled") }),
          icon: "folder",
          run: () => movePages([page.id]),
        },
        {
          key: "page-duplicate",
          group: pg,
          label: t("palette.duplicate"),
          icon: "files",
          run: () => duplicatePage(page.id),
        },
        {
          key: "page-reveal",
          group: pg,
          label: t("palette.reveal"),
          icon: "folder-open",
          run: () => revealPage(page),
        },
        {
          key: "page-editor",
          group: pg,
          label: editor
            ? t("sidebar.openIn", { editor: editor.name })
            : t("palette.editExternal"),
          icon: "external-link",
          run: () => openExternally(page),
        },
        {
          key: "page-history",
          group: pg,
          label: t("palette.pageHistory"),
          icon: "refresh-cw",
          run: () => {
            app.historyOpen = true;
          },
        },
        {
          key: "page-good",
          group: pg,
          label: t("palette.reviewGood"),
          icon: "circle-check",
          run: () => gradePage(page, "good"),
        },
        {
          key: "page-again",
          group: pg,
          label: t("palette.reviewAgain"),
          icon: "refresh-cw",
          run: () => gradePage(page, "again"),
        },
        {
          key: "page-trash",
          group: pg,
          label: t("palette.trashPage"),
          icon: "trash-2",
          run: () => deletePages([page.id]),
        },
      );
      if (hasStoredData(page)) {
        list.push({
          key: "page-reset-storage",
          group: pg,
          label: t("palette.resetStorage"),
          icon: "trash-2",
          run: () => resetStorage(page),
        });
      }
    }
    return list;
  });

  const pageItems = $derived.by<Item[]>(() => {
    const query = needle;
    const source: PageMeta[] = query
      ? results.query === query
        ? results.hits
        : []
      : [...app.library]
          .sort((a, b) => (b.updatedAt || b.createdAt) - (a.updatedAt || a.createdAt))
          .slice(0, 8);
    return source.map((p) => ({
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
      run: () => openPage(p.id),
    }));
  });

  const folderItems = $derived.by<Item[]>(() => {
    const query = needle.toLowerCase();
    if (!query) return [];
    return app.folders
      .filter((f) => f.toLowerCase().includes(query))
      .slice(0, 5)
      .map((f) => ({
        key: `folder-${f}`,
        group: t("palette.folders"),
        label: f,
        icon: "folder" as IconName,
        run: () => showFolder(f),
      }));
  });

  const tagItems = $derived.by<Item[]>(() => {
    const query = needle.toLowerCase();
    if (!query) return [];
    return app.tags
      .filter((tg) => tg.tag.toLowerCase().includes(query))
      .slice(0, 5)
      .map((tg) => ({
        key: `tag-${tg.tag}`,
        group: t("palette.tags"),
        label: `#${tg.tag}`,
        hint: String(tg.count),
        icon: "hash" as IconName,
        run: () => showTag(tg.tag),
      }));
  });

  const items = $derived.by<Item[]>(() => {
    const query = needle.toLowerCase();
    const acts = query ? actions.filter((a) => a.label.toLowerCase().includes(query)) : actions;
    return [...acts, ...pageItems, ...folderItems, ...tagItems];
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
    void loadEditors();
    queueMicrotask(() => inputEl?.focus());
  });

  function restoreFocus() {
    if (!prevFocus || !prevFocus.isConnected) return;
    prevFocus.focus();
  }

  onDestroy(() => {
    clearTimeout(debounceTimer);
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
    try {
      await item.run();
    } catch (e) {
      console.error(e);
      toast(errorMessage(e), "error");
    } finally {
      running = false;
    }
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
            void run(item);
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
        <div class="p-empty">
          {searching ? t("palette.searching") : t("palette.noMatches", { query: q })}
        </div>
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
    z-index: var(--z-palette);
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
    font-size: var(--fs-md);
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
    font-size: var(--fs-2xs);
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
    font-size: var(--fs-base);
    font-weight: 500;
  }
  .p-hint {
    flex: none;
    max-width: 45%;
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .p-item.active .p-hint {
    color: var(--text-soft);
  }
  .p-empty {
    padding: 22px 12px;
    text-align: center;
    font-size: var(--fs-sm);
    color: var(--muted);
  }

  .p-foot {
    display: flex;
    gap: 14px;
    padding: 9px 14px;
    border-top: 1px solid var(--border);
    background: var(--raised);
    font-size: var(--fs-xs);
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
