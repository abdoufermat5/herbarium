<script lang="ts">
  import { app, dueLabel, clearFilters, refreshAll, reloadPages, toast } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import Icon from "../lib/Icon.svelte";
  import { modKey } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import { slide } from "svelte/transition";
  import type { PageMeta } from "../lib/types";

  interface Branch {
    name: string;
    path: string;
    folders: Branch[];
    pages: PageMeta[];
  }

  const STORAGE_KEY = "herbarium.expanded";

  function loadExpanded(): Set<string> {
    try {
      return new Set(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]"));
    } catch {
      return new Set();
    }
  }

  let expanded = $state(loadExpanded());

  const vaultName = $derived(
    app.config?.vaultPath?.split(/[\\/]/).filter(Boolean).pop() ?? "",
  );

  /** Nested folders (including empty intermediate ones) with their pages. */
  const tree = $derived.by(() => {
    const root: Branch = { name: "", path: "", folders: [], pages: [] };
    const byPath = new Map<string, Branch>([["", root]]);
    const ensure = (path: string): Branch => {
      const known = byPath.get(path);
      if (known) return known;
      const cut = path.lastIndexOf("/");
      const parent = ensure(cut < 0 ? "" : path.slice(0, cut));
      const node: Branch = { name: path.slice(cut + 1), path, folders: [], pages: [] };
      parent.folders.push(node);
      byPath.set(path, node);
      return node;
    };
    for (const f of app.folders) ensure(f);
    for (const p of app.library) ensure(p.folder ?? "").pages.push(p);
    const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });
    for (const node of byPath.values()) {
      node.folders.sort((a, b) => collator.compare(a.name, b.name));
      node.pages.sort((a, b) => collator.compare(a.title, b.title));
    }
    return root;
  });

  function toggleFolder(path: string) {
    const next = new Set(expanded);
    if (!next.delete(path)) next.add(path);
    expanded = next;
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...next]));
  }

  function openPage(id: string) {
    app.readId = id;
  }

  let creating = $state<{ kind: "page" | "folder"; parent: string } | null>(null);
  let draftName = $state("");

  function setExpanded(path: string, open: boolean) {
    const next = new Set(expanded);
    if (open) next.add(path);
    else next.delete(path);
    expanded = next;
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...next]));
  }

  function startCreate(kind: "page" | "folder", parent: string) {
    if (parent) setExpanded(parent, true);
    draftName = "";
    creating = { kind, parent };
  }

  function cancelCreate() {
    creating = null;
  }

  async function commitCreate() {
    const target = creating;
    const name = draftName.trim();
    creating = null;
    if (!target || !name) return;
    const path = target.parent ? `${target.parent}/${name}` : name;
    try {
      if (target.kind === "folder") {
        const made = await api.createFolder(path);
        for (let p = made; p; p = p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "") setExpanded(p, true);
        await refreshAll();
        toast(t("sidebar.folderCreated", { name: made }), "success");
      } else {
        const page = await api.createPage(name, target.parent || null);
        await reloadPages();
        app.readId = page.id;
      }
    } catch (e) {
      console.error(e);
      toast(t(target.kind === "folder" ? "sidebar.folderFailed" : "sidebar.pageFailed"), "error");
    }
  }

  function onCreateKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void commitCreate();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cancelCreate();
    }
  }

  function goAll() {
    app.readId = null;
    app.view = "list";
    clearFilters();
  }

  function goReview() {
    app.readId = null;
    app.view = "review";
  }
</script>

{#snippet creator(parent: string, depth: number)}
  {#if creating && creating.parent === parent}
    <div class="tree-row">
      <span class="nav-item tree-item creator" style:padding-left="{10 + depth * 14 + (creating.kind === "page" ? 16 : 0)}px">
        {#if creating.kind === "folder"}
          <span class="chev"><Icon name="chevron-right" size={10} /></span>
        {/if}
        <span class="nav-icon"><Icon name={creating.kind === "folder" ? "folder" : "file-text"} size={14} /></span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="create-input"
          autofocus
          spellcheck="false"
          aria-label={creating.kind === "folder" ? t("sidebar.folderName") : t("sidebar.pageName")}
          placeholder={creating.kind === "folder" ? t("sidebar.folderName") : t("sidebar.pageName")}
          bind:value={draftName}
          onkeydown={onCreateKey}
          onblur={cancelCreate}
        />
      </span>
    </div>
  {/if}
{/snippet}

{#snippet branch(node: Branch, depth: number)}
  {@render creator(node.path, depth)}
  {#each node.folders as folder (folder.path)}
    {@const open = expanded.has(folder.path)}
    <div class="tree-row">
      <button
        class="nav-item tree-item"
        style:padding-left="{10 + depth * 14}px"
        role="treeitem"
        aria-selected="false"
        aria-expanded={open}
        title={folder.path}
        onclick={() => toggleFolder(folder.path)}
      >
        <span class="chev" class:open><Icon name="chevron-right" size={10} /></span>
        <span class="nav-icon"><Icon name={open ? "folder-open" : "folder"} size={14} /></span>
        <span class="nav-label ellipsis">{folder.name}</span>
      </button>
      <span class="row-actions">
        <button class="tool" title={t("sidebar.newPage")} aria-label={t("sidebar.newPage")} onclick={() => startCreate("page", folder.path)}>
          <Icon name="file-plus" size={13} />
        </button>
        <button class="tool" title={t("sidebar.newFolder")} aria-label={t("sidebar.newFolder")} onclick={() => startCreate("folder", folder.path)}>
          <Icon name="folder-plus" size={13} />
        </button>
      </span>
    </div>
    {#if open}
      <div role="group" transition:slide={{ duration: 120 }}>
        {@render branch(folder, depth + 1)}
      </div>
    {/if}
  {/each}
  {#each node.pages as page (page.id)}
    <button
      class="nav-item tree-item tree-page"
      class:active={app.readId === page.id}
      style:padding-left="{10 + depth * 14 + 16}px"
      role="treeitem"
      aria-selected={app.readId === page.id}
      title={page.title}
      onclick={() => openPage(page.id)}
    >
      <span class="nav-icon"><Icon name="file-text" size={14} /></span>
      <span class="nav-label ellipsis">{page.title}</span>
    </button>
  {/each}
{/snippet}

<aside class="sidebar">
  <div class="brand">
    <div class="brand-mark"><Icon name="leaf" size={16} /></div>
    <div class="brand-text">
      <strong>Herbarium</strong>
      {#if vaultName && vaultName.toLowerCase() !== "herbarium"}
        <span class="brand-vault ellipsis" title={app.config?.vaultPath ?? ""}>{vaultName}</span>
      {/if}
    </div>
  </div>

  <div class="body">
    <nav class="nav" aria-label={t("sidebar.library")}>
      <button
        class="nav-item"
        class:active={!app.readId && app.view === "list" && !app.folderFilter && !app.tagFilter}
        aria-current={!app.readId && app.view === "list" && !app.folderFilter && !app.tagFilter ? "page" : undefined}
        onclick={goAll}
      >
        <span class="nav-icon"><Icon name="files" size={15} /></span>
        <span class="nav-label ellipsis">{t("sidebar.all")}</span>
      </button>
      <button
        class="nav-item"
        class:active={!app.readId && app.view === "review"}
        aria-current={!app.readId && app.view === "review" ? "page" : undefined}
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

    <div class="group">
      <div class="group-head">
        <div class="group-title static">{t("sidebar.files")}</div>
        <div class="tools">
          <button class="tool" title={t("sidebar.newPage")} aria-label={t("sidebar.newPage")} onclick={() => startCreate("page", "")}>
            <Icon name="file-plus" size={14} />
          </button>
          <button class="tool" title={t("sidebar.newFolder")} aria-label={t("sidebar.newFolder")} onclick={() => startCreate("folder", "")}>
            <Icon name="folder-plus" size={14} />
          </button>
        </div>
      </div>
      <div class="tree" role="tree" aria-label={t("sidebar.files")}>
        {@render branch(tree, 0)}
      </div>
      {#if app.library.length === 0 && app.folders.length === 0 && !creating}
        <div class="hint">
          <Icon name="info" size={14} />
          <span>{t("sidebar.hint")}</span>
        </div>
      {/if}
    </div>
  </div>

  <div class="foot">
    <nav class="nav" aria-label={t("settings.title")}>
      <button
        class="nav-item"
        class:active={!app.readId && app.view === "settings"}
        aria-current={!app.readId && app.view === "settings" ? "page" : undefined}
        title={modKey(",")}
        onclick={() => {
          app.readId = null;
          app.view = "settings";
        }}
      >
        <span class="nav-icon"><Icon name="settings" size={15} /></span>
        <span class="nav-label ellipsis">{t("settings.title")}</span>
      </button>
    </nav>
    <button class="search-btn" onclick={() => (app.paletteOpen = true)}>
      <Icon name="search" size={14} />
      <span>{t("sidebar.search")}</span>
      <kbd class="kbd">{modKey("K")}</kbd>
    </button>
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
    padding: 20px 18px 18px;
  }
  .brand-mark {
    flex: none;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--leaf-soft);
    color: var(--leaf);
  }
  .brand-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.2;
  }
  .brand-text strong {
    font-family: var(--font-display);
    font-size: 19px;
    font-weight: 500;
    letter-spacing: -0.02em;
    color: var(--text);
  }
  .brand-vault {
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--muted);
  }

  .body {
    flex: 1;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 4px 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    font-size: 13.5px;
    color: var(--text-soft);
    text-align: left;
    transition:
      background var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }
  .nav-item:hover {
    background: var(--sunken);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--border);
    color: var(--accent-strong);
    font-weight: 500;
  }
  .nav-icon {
    flex: none;
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .nav-item.active .nav-icon {
    color: var(--accent-strong);
  }
  .nav-label {
    flex: 1;
    min-width: 0;
  }
  .nav-item .badge {
    margin-left: auto;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .group-title {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 10px 6px;
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
    border-radius: var(--radius-xs);
  }
  .group-title:hover {
    color: var(--text);
  }
  .chev {
    display: grid;
    place-items: center;
    transition: transform var(--t-fast) var(--ease-out);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .group-title.static {
    cursor: default;
  }
  .group-title.static:hover {
    color: var(--muted);
  }
  .tree {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .tree-item {
    width: 100%;
    font-size: 13px;
    color: var(--text-soft);
    gap: 6px;
  }
  .tree-page {
    color: var(--muted);
  }
  .group-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 4px;
  }
  .tools,
  .row-actions {
    display: flex;
    gap: 1px;
  }
  .tool {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: var(--radius-xs);
    color: var(--muted);
  }
  .tool:hover {
    background: var(--sunken);
    color: var(--text);
  }
  .tree-row {
    position: relative;
  }
  .row-actions {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    opacity: 0;
    pointer-events: none;
    background: var(--raised);
    border-radius: var(--radius-xs);
  }
  .tree-row:hover .row-actions,
  .tree-row:focus-within .row-actions {
    opacity: 1;
    pointer-events: auto;
  }
  .creator {
    cursor: text;
  }
  .create-input {
    flex: 1;
    min-width: 0;
    padding: 0 4px;
    height: 20px;
    font: inherit;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border-hover);
    border-radius: var(--radius-xs);
    outline: none;
  }

  .hint {
    display: flex;
    gap: 8px;
    padding: 12px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .hint :global(svg) {
    margin-top: 2px;
  }

  .foot {
    flex: none;
    padding: 10px 10px 14px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .search-btn {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 6px 6px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
    font-size: 13px;
    transition:
      border-color var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }
  .search-btn:hover {
    border-color: var(--border-hover);
    color: var(--text);
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
