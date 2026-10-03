<script lang="ts">
  import { tick } from "svelte";
  import { flip } from "svelte/animate";
  import {
    app,
    clearFilters,
    deletePages,
    dueLabel,
    duplicatePage,
    errorMessage,
    goAll,
    goView,
    inFolder,
    movePages,
    openPage,
    pendingFiles,
    refreshAll,
    reloadPages,
    showFolder,
    toast,
  } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import Icon from "../lib/Icon.svelte";
  import { modKey } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import { currentEditor, loadEditors } from "../lib/editors.svelte";
  import { confirmAction } from "../lib/confirm.svelte";
  import { pickFolder } from "../lib/folder-picker.svelte";
  import { shortcutHint } from "../lib/shortcuts";
  import type { PageMeta } from "../lib/types";
  import ContextMenu from "./ContextMenu.svelte";
  import type { DropdownMenuItem } from "./DropdownMenu.svelte";

  interface Branch {
    name: string;
    path: string;
    folders: Branch[];
    pages: PageMeta[];
  }

  /** One rendered line of the tree: a page, a folder or the inline creator. */
  interface TreeRow {
    kind: "page" | "folder" | "creator";
    key: string;
    depth: number;
    parent: string;
    name: string;
    path?: string;
    id?: string;
    page?: PageMeta;
    expanded?: boolean;
    createKind?: "page" | "folder";
  }

  const STORAGE_KEY = "herbarium.expanded";
  /** Drag payload shared with the page list; JSON array of page ids. */
  const PAGE_DRAG_MIME = "application/x-herbarium-page-ids";

  function loadExpanded(): Set<string> {
    try {
      return new Set(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]"));
    } catch {
      return new Set();
    }
  }

  let expanded = $state(loadExpanded());
  let creating = $state<{ kind: "page" | "folder"; parent: string } | null>(null);
  let draftName = $state("");
  let renaming = $state<{
    kind: "page" | "folder";
    id?: string;
    path?: string;
    parent: string;
  } | null>(null);
  let renameValue = $state("");
  let focusedKey = $state("");
  /** Folder path under the pointer during a drop, "" for the vault root, null for none. */
  let dropTarget = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; items: DropdownMenuItem[] } | null>(null);
  let treeEl: HTMLElement | undefined = $state();

  function persistExpanded(next: Set<string>) {
    expanded = next;
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...next]));
  }

  function setExpanded(path: string, open: boolean) {
    const next = new Set(expanded);
    if (open) next.add(path);
    else next.delete(path);
    persistExpanded(next);
  }

  function toggleFolder(path: string) {
    setExpanded(path, !expanded.has(path));
  }

  const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
  let reduceMotion = $state(motionQuery.matches);
  $effect(() => {
    const onChange = (e: MediaQueryListEvent) => (reduceMotion = e.matches);
    motionQuery.addEventListener("change", onChange);
    return () => motionQuery.removeEventListener("change", onChange);
  });

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

  /**
   * Flattened display order used by the roving keyboard focus: exactly the
   * rows that are visible (a folder's children only while it is expanded).
   */
  const rows = $derived.by(() => {
    const out: TreeRow[] = [];
    const walk = (node: Branch, depth: number) => {
      if (creating && creating.parent === node.path) {
        out.push({ kind: "creator", key: `c:${node.path}`, depth, parent: node.path, name: "", createKind: creating.kind });
      }
      for (const folder of node.folders) {
        const open = expanded.has(folder.path);
        out.push({ kind: "folder", key: `f:${folder.path}`, depth, parent: node.path, name: folder.name, path: folder.path, expanded: open });
        if (open) walk(folder, depth + 1);
      }
      for (const page of node.pages) {
        out.push({ kind: "page", key: `p:${page.id}`, depth, parent: node.path, name: page.title || t("common.untitled"), id: page.id, page });
      }
    };
    walk(tree, 0);
    return out;
  });

  /** The row that owns the single tab stop; keeps one row reachable if focus was lost. */
  const tabbableKey = $derived(
    rows.some((r) => r.key === focusedKey) ? focusedKey : (rows[0]?.key ?? ""),
  );

  function cssEscape(value: string): string {
    try {
      return CSS.escape(value);
    } catch {
      return value.replace(/["\\]/g, "\\$&");
    }
  }

  function focusRow(key: string) {
    focusedKey = key;
    void tick().then(() => {
      treeEl?.querySelector<HTMLElement>(`[data-key="${cssEscape(key)}"]`)?.focus();
    });
  }

  function moveFocus(index: number) {
    if (rows.length === 0) return;
    const clamped = Math.max(0, Math.min(rows.length - 1, index));
    focusRow(rows[clamped].key);
  }

  function expandRow(index: number, row: TreeRow) {
    if (row.kind !== "folder") return;
    if (!row.expanded) setExpanded(row.path!, true);
    else moveFocus(index + 1);
  }

  function collapseRow(row: TreeRow) {
    if (row.kind === "folder" && row.expanded) {
      setExpanded(row.path!, false);
      return;
    }
    if (row.parent === "") return;
    const parentIndex = rows.findIndex((r) => r.key === `f:${row.parent}`);
    if (parentIndex >= 0) focusRow(rows[parentIndex].key);
  }

  function onTreeKey(e: KeyboardEvent) {
    // Inputs (creator, rename) keep their own arrow/caret behaviour.
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    const key = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-key]")?.dataset.key ?? focusedKey;
    const index = rows.findIndex((r) => r.key === key);
    if (index < 0) return;
    const row = rows[index];
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        moveFocus(index + 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        moveFocus(index - 1);
        break;
      case "Home":
        e.preventDefault();
        moveFocus(0);
        break;
      case "End":
        e.preventDefault();
        moveFocus(rows.length - 1);
        break;
      case "ArrowRight":
        e.preventDefault();
        expandRow(index, row);
        break;
      case "ArrowLeft":
        e.preventDefault();
        collapseRow(row);
        break;
      case "F2":
        e.preventDefault();
        if (row.kind !== "creator") startRename(row);
        break;
      case "Delete":
        e.preventDefault();
        if (row.kind !== "creator") void removeRow(row);
        break;
    }
  }

  /* ------------------------------------------------------------------ create */

  function startCreate(kind: "page" | "folder", parent: string) {
    if (parent) setExpanded(parent, true);
    draftName = "";
    renaming = null;
    creating = { kind, parent };
  }

  function cancelCreate() {
    creating = null;
  }

  async function commitCreate() {
    const target = creating;
    if (!target) return;
    const name = draftName.trim();
    if (!name) return;
    const path = target.parent ? `${target.parent}/${name}` : name;
    if (target.kind === "folder") {
      try {
        const made = await api.createFolder(path);
        for (let p = made; p; p = p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "") setExpanded(p, true);
        creating = null;
        draftName = "";
        await refreshAll();
        toast(t("sidebar.folderCreated", { name: made }), "success");
      } catch (e) {
        toast(`${t("sidebar.folderFailed")}: ${errorMessage(e)}`, "error");
      }
    } else {
      try {
        const page = await api.createPage(name, target.parent || null);
        creating = null;
        draftName = "";
        await reloadPages();
        await openPage(page.id);
      } catch (e) {
        toast(`${t("sidebar.pageFailed")}: ${errorMessage(e)}`, "error");
      }
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

  // A creation asked for elsewhere (palette, menus): consume it, then clear it.
  $effect(() => {
    const request = app.createRequest;
    if (!request) return;
    app.createRequest = null;
    startCreate(request.kind, request.parent);
  });

  /* ------------------------------------------------------------------ rename */

  function startRename(row: TreeRow) {
    if (row.kind === "page") {
      renaming = { kind: "page", id: row.id, parent: row.parent };
      renameValue = row.page?.title ?? row.name;
    } else if (row.kind === "folder") {
      renaming = { kind: "folder", path: row.path, parent: row.parent };
      renameValue = row.name;
    }
  }

  function cancelRename() {
    renaming = null;
  }

  function onRenameKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      void commitRename();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelRename();
    }
  }

  function afterFolderMoved(from: string, to: string) {
    const next = new Set<string>();
    for (const p of expanded) {
      if (p === from) next.add(to);
      else if (p.startsWith(`${from}/`)) next.add(`${to}${p.slice(from.length)}`);
      else next.add(p);
    }
    persistExpanded(next);
    if (app.folderFilter && inFolder(app.folderFilter, from)) {
      app.folderFilter = `${to}${app.folderFilter.slice(from.length)}`;
    }
  }

  async function commitRename() {
    const target = renaming;
    if (!target) return;
    const value = renameValue.trim();
    if (!value) {
      cancelRename();
      return;
    }
    if (target.kind === "page") {
      const id = target.id!;
      try {
        await api.updatePageMeta(id, { title: value });
        renaming = null;
        await reloadPages(true);
        focusRow(`p:${id}`);
      } catch (e) {
        toast(t("sidebar.renameFailed", { detail: errorMessage(e) }), "error");
      }
      return;
    }
    if (target.kind === "folder") {
      const from = target.path!;
      const to = target.parent ? `${target.parent}/${value}` : value;
      if (to === from) {
        cancelRename();
        return;
      }
      try {
        const res = await api.renameFolder(from, to);
        renaming = null;
        afterFolderMoved(from, res.folder);
        await refreshAll();
        focusRow(`f:${res.folder}`);
      } catch (e) {
        toast(t("sidebar.renameFailed", { detail: errorMessage(e) }), "error");
      }
      return;
    }
  }

  /* ------------------------------------------------------------------ delete */

  async function removeRow(row: TreeRow) {
    if (row.kind === "page") await deletePages([row.id!]);
    else if (row.kind === "folder") await confirmDeleteFolder(row.path!, row.name);
  }

  async function restorePages(ids: string[]) {
    const restored: string[] = [];
    const errors: string[] = [];
    for (const id of ids) {
      try {
        await api.restorePage(id);
        restored.push(id);
      } catch (e) {
        errors.push(errorMessage(e));
      }
    }
    if (restored.length > 0) toast(t("toast.restored", { count: restored.length }), "success");
    if (errors.length > 0) toast(t("toast.restoreFailed", { detail: errors[0] }), "error");
    await reloadPages(true);
  }

  async function confirmDeleteFolder(path: string, name: string) {
    const pages = app.library.filter((p) => inFolder(p.folder, path));
    const folders = app.folders.filter((f) => f !== path && inFolder(f, path));
    const confirmed = await confirmAction({
      title: t("sidebar.deleteFolderTitle", { name }),
      message:
        pages.length > 0
          ? t("sidebar.deleteFolderWithPages", { count: pages.length })
          : folders.length > 0
            ? t("sidebar.deleteFolderSubfolders")
            : t("sidebar.deleteFolderMessage"),
      confirmLabel: pages.length > 0 ? t("sidebar.deleteFolderAndPages") : t("sidebar.deleteFolder"),
      danger: true,
      subject: {
        icon: "folder",
        label: name,
        meta: path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : t("confirm.vaultRoot"),
      },
    });
    if (!confirmed) return;
    const ids = pages.map((p) => p.id);
    try {
      const res = await api.deleteFolder(path, ids.length > 0);
      if (app.folderFilter && inFolder(app.folderFilter, path)) clearFilters();
      setExpanded(path, false);
      await refreshAll();
      if (res.trashed > 0) {
        toast(t("toast.trashed", { count: res.trashed }), "success", 8000, {
          label: t("toast.undo"),
          run: () => void restorePages(ids),
        });
      } else {
        toast(t("sidebar.folderDeleted", { name }), "success");
      }
    } catch (e) {
      toast(t("sidebar.folderDeleteFailed", { detail: errorMessage(e) }), "error");
    }
  }

  /* ------------------------------------------------------------------- menus */

  function isInputTarget(e: Event): boolean {
    const el = e.target as HTMLElement | null;
    return !!el && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el.isContentEditable);
  }

  /** Right click anchors at the pointer; the context-menu key (clientX/Y = 0) at the row. */
  function anchor(e: MouseEvent): { x: number; y: number } {
    e.preventDefault();
    e.stopPropagation();
    if (e.clientX === 0 && e.clientY === 0 && e.currentTarget instanceof HTMLElement) {
      const r = e.currentTarget.getBoundingClientRect();
      return { x: r.left + 24, y: r.bottom };
    }
    return { x: e.clientX, y: e.clientY };
  }

  function openMenu(e: MouseEvent, items: DropdownMenuItem[]) {
    menu = { ...anchor(e), items };
  }

  function createItems(parent: string): DropdownMenuItem[] {
    return [
      { id: "new-page", label: t("sidebar.newPage"), icon: "file-plus", onclick: () => startCreate("page", parent) },
      { id: "new-folder", label: t("sidebar.newFolder"), icon: "folder-plus", onclick: () => startCreate("folder", parent) },
    ];
  }

  function filesMenu(e: MouseEvent) {
    // Keep the native cut/copy/paste menu inside a name input.
    if (isInputTarget(e)) return;
    openMenu(e, createItems(""));
  }

  function folderMenu(e: MouseEvent, row: TreeRow) {
    if (isInputTarget(e)) return;
    const path = row.path!;
    const open = !!row.expanded;
    openMenu(e, [
      ...createItems(path),
      { divider: true },
      { id: "show", label: t("sidebar.showPages"), icon: "files", shortcut: shortcutHint("openItem"), onclick: () => void showFolder(path) },
      {
        id: "toggle",
        label: open ? t("sidebar.collapse") : t("sidebar.expand"),
        icon: open ? "folder" : "folder-open",
        onclick: () => toggleFolder(path),
      },
      { divider: true },
      { id: "move", label: t("folderPicker.move"), icon: "folder", onclick: () => void moveFolder(row) },
      { id: "rename", label: t("sidebar.rename"), icon: "folder", shortcut: shortcutHint("renameItem"), onclick: () => startRename(row) },
      { id: "delete", label: t("sidebar.deleteFolder"), icon: "trash-2", danger: true, shortcut: shortcutHint("deleteItem"), onclick: () => void confirmDeleteFolder(path, row.name) },
    ]);
  }

  function pageMenu(e: MouseEvent, row: TreeRow) {
    if (isInputTarget(e)) return;
    const page = row.page;
    if (!page) return;
    const at = anchor(e);
    // Resolve the editor first so its name is in the label.
    void (async () => {
      await loadEditors();
      const editor = currentEditor();
      menu = {
        ...at,
        items: [
          { id: "open", label: t("common.open"), icon: "file-text", shortcut: shortcutHint("openItem"), onclick: () => void openPage(page.id) },
          {
            id: "editor",
            label: editor ? t("sidebar.openIn", { editor: editor.name }) : t("sidebar.noEditor"),
            icon: "external-link",
            disabled: !editor,
            onclick: () => void openInEditor(page),
          },
          { divider: true },
          { id: "move", label: t("folderPicker.move"), icon: "folder", onclick: () => void movePages([page.id]) },
          { id: "duplicate", label: t("list.duplicate"), icon: "files", onclick: () => void duplicatePage(page.id) },
          { id: "reveal", label: t("list.reveal"), icon: "external-link", onclick: () => void revealPage(page) },
          { divider: true },
          { id: "rename", label: t("sidebar.rename"), icon: "file-text", shortcut: shortcutHint("renameItem"), onclick: () => startRename(row) },
          { id: "delete", label: t("insp.deletePage"), icon: "trash-2", danger: true, shortcut: shortcutHint("deleteItem"), onclick: () => void deletePages([page.id]) },
        ],
      };
    })();
  }

  async function openInEditor(page: PageMeta) {
    const editor = currentEditor();
    if (!editor) return;
    try {
      const name = await api.openInEditor(page.id, editor.id, editor.custom);
      toast(t("edit.openedIn", { editor: name }), "success");
    } catch (e) {
      toast(`${t("edit.openFailed")}: ${errorMessage(e)}`, "error");
    }
  }

  async function revealPage(page: PageMeta) {
    try {
      await api.revealPage(page.id);
    } catch (e) {
      toast(t("list.revealFailed", { detail: errorMessage(e) }), "error");
    }
  }

  async function moveFolder(row: TreeRow) {
    const from = row.path!;
    const parent = from.includes("/") ? from.slice(0, from.lastIndexOf("/")) : "";
    const dest = await pickFolder({ title: t("folderPicker.move"), initial: parent || null, exclude: from });
    if (dest === undefined) return;
    const to = dest ? `${dest}/${row.name}` : row.name;
    if (to === from) return;
    try {
      const res = await api.renameFolder(from, to);
      afterFolderMoved(from, res.folder);
      await refreshAll();
      toast(t("sidebar.folderMoved", { name: res.folder }), "success");
    } catch (e) {
      toast(t("sidebar.folderMoveFailed", { detail: errorMessage(e) }), "error");
    }
  }

  /* ------------------------------------------------------------- drag and drop */

  function hasOsFiles(e: DragEvent): boolean {
    return !!e.dataTransfer && Array.from(e.dataTransfer.types).includes("Files");
  }

  function hasPageDrag(e: DragEvent): boolean {
    return !!e.dataTransfer && Array.from(e.dataTransfer.types).includes(PAGE_DRAG_MIME);
  }

  function readPageIds(dataTransfer: DataTransfer | null): string[] {
    const raw = dataTransfer?.getData(PAGE_DRAG_MIME);
    if (!raw) return [];
    try {
      const parsed: unknown = JSON.parse(raw);
      return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : [];
    } catch {
      return [];
    }
  }

  function onPageDragStart(e: DragEvent, id: string) {
    const dt = e.dataTransfer;
    if (!dt) return;
    dt.setData(PAGE_DRAG_MIME, JSON.stringify([id]));
    dt.effectAllowed = "move";
  }

  function stageOsDrop(e: DragEvent, folder: string) {
    const files = Array.from(e.dataTransfer?.files ?? []).filter((f) => /\.html?$/i.test(f.name));
    if (files.length === 0) return;
    pendingFiles.files = files;
    // "" is the vault root; null tells the importer explicitly to use the root.
    pendingFiles.folder = folder === "" ? null : folder;
    pendingFiles.allowCdn = undefined;
    app.importOpen = true;
  }

  async function moveDropped(ids: string[], folder: string) {
    const dest = folder === "" ? null : folder;
    try {
      const result = await api.bulkUpdate(ids, { folder: dest });
      if (result.errors.length > 0) {
        toast(
          t("toast.movePartial", { done: result.updated.length, failed: result.errors.length, detail: result.errors[0].error }),
          "error",
        );
      } else {
        toast(t("toast.moved", { count: result.updated.length }), "success");
      }
      await reloadPages(true);
    } catch (e) {
      toast(t("toast.moveFailed", { detail: errorMessage(e) }), "error");
    }
  }

  function onDragOverTarget(e: DragEvent, folder: string) {
    if (!hasPageDrag(e) && !hasOsFiles(e)) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = hasOsFiles(e) ? "copy" : "move";
    dropTarget = folder;
  }

  async function onDropTarget(e: DragEvent, folder: string) {
    if (!hasPageDrag(e) && !hasOsFiles(e)) return;
    e.preventDefault();
    e.stopPropagation();
    dropTarget = null;
    if (hasOsFiles(e)) {
      stageOsDrop(e, folder);
      return;
    }
    const ids = readPageIds(e.dataTransfer);
    if (ids.length > 0) await moveDropped(ids, folder);
  }

  function clearDropTarget(folder: string) {
    if (dropTarget === folder) dropTarget = null;
  }
</script>

{#snippet creatorRow(row: TreeRow)}
  <div class="tree-row">
    <span
      class="nav-item tree-item creator"
      style:padding-left="{10 + row.depth * 14 + (row.createKind === "page" ? 16 : 0)}px"
    >
      {#if row.createKind === "folder"}
        <span class="chev"><Icon name="chevron-right" size={10} /></span>
      {/if}
      <span class="nav-icon"><Icon name={row.createKind === "folder" ? "folder" : "file-text"} size={14} /></span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="create-input"
        autofocus
        spellcheck="false"
        aria-label={row.createKind === "folder" ? t("sidebar.folderName") : t("sidebar.pageName")}
        placeholder={row.createKind === "folder" ? t("sidebar.folderName") : t("sidebar.pageName")}
        bind:value={draftName}
        onkeydown={onCreateKey}
        onblur={cancelCreate}
      />
    </span>
  </div>
{/snippet}

{#snippet folderRow(row: TreeRow)}
  <div
    class="tree-row"
    class:drop={dropTarget === row.path}
    role="presentation"
    ondragover={(e) => onDragOverTarget(e, row.path!)}
    ondragleave={() => clearDropTarget(row.path!)}
    ondrop={(e) => void onDropTarget(e, row.path!)}
  >
    <button
      type="button"
      class="chev-btn"
      tabindex="-1"
      aria-label={row.expanded ? t("sidebar.collapse") : t("sidebar.expand")}
      aria-expanded={row.expanded}
      onclick={() => toggleFolder(row.path!)}
    >
      <span class="chev" class:open={row.expanded}><Icon name="chevron-right" size={10} /></span>
    </button>
    {#if renaming?.kind === "folder" && renaming.path === row.path}
      <span class="nav-item tree-item tree-folder" style:padding-left="{4 + row.depth * 14}px">
        <span class="nav-icon"><Icon name={row.expanded ? "folder-open" : "folder"} size={14} /></span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="create-input"
          autofocus
          spellcheck="false"
          aria-label={t("sidebar.folderName")}
          bind:value={renameValue}
          onkeydown={onRenameKey}
          onblur={cancelRename}
        />
      </span>
    {:else}
      <button
        type="button"
        class="nav-item tree-item tree-folder"
        class:active={app.folderFilter === row.path}
        style:padding-left="{4 + row.depth * 14}px"
        title={row.path}
        data-key={row.key}
        tabindex={tabbableKey === row.key ? 0 : -1}
        aria-current={app.folderFilter === row.path ? "true" : undefined}
        onfocus={() => (focusedKey = row.key)}
        onkeydown={onTreeKey}
        onclick={() => void showFolder(row.path!)}
        oncontextmenu={(e) => folderMenu(e, row)}
      >
        <span class="nav-icon"><Icon name={row.expanded ? "folder-open" : "folder"} size={14} /></span>
        <span class="nav-label ellipsis">{row.name}</span>
      </button>
      <span class="row-actions">
        <button class="tool" title={t("sidebar.newPage")} aria-label={t("sidebar.newPage")} onclick={() => startCreate("page", row.path!)}>
          <Icon name="file-plus" size={13} />
        </button>
        <button class="tool" title={t("sidebar.newFolder")} aria-label={t("sidebar.newFolder")} onclick={() => startCreate("folder", row.path!)}>
          <Icon name="folder-plus" size={13} />
        </button>
      </span>
    {/if}
  </div>
{/snippet}

{#snippet pageRow(row: TreeRow)}
  <div class="tree-row">
    {#if renaming?.kind === "page" && renaming.id === row.id}
      <span class="nav-item tree-item tree-page" style:padding-left="{10 + row.depth * 14 + 16}px">
        <span class="nav-icon"><Icon name="file-text" size={14} /></span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="create-input"
          autofocus
          spellcheck="false"
          aria-label={t("sidebar.pageName")}
          bind:value={renameValue}
          onkeydown={onRenameKey}
          onblur={cancelRename}
        />
      </span>
    {:else}
      <button
        type="button"
        class="nav-item tree-item tree-page"
        class:active={app.readId === row.id}
        style:padding-left="{10 + row.depth * 14 + 16}px"
        title={row.name}
        data-key={row.key}
        tabindex={tabbableKey === row.key ? 0 : -1}
        aria-current={app.readId === row.id ? "page" : undefined}
        draggable="true"
        onfocus={() => (focusedKey = row.key)}
        onkeydown={onTreeKey}
        onclick={() => void openPage(row.id!)}
        oncontextmenu={(e) => pageMenu(e, row)}
        ondragstart={(e) => onPageDragStart(e, row.id!)}
        ondragend={() => (dropTarget = null)}
        ondragover={(e) => e.stopPropagation()}
        ondrop={(e) => e.stopPropagation()}
      >
        <span class="nav-icon"><Icon name="file-text" size={14} /></span>
        <span class="nav-label ellipsis">{row.name}</span>
      </button>
    {/if}
  </div>
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
        class:drop={dropTarget === ""}
        aria-current={!app.readId && app.view === "list" && !app.folderFilter && !app.tagFilter ? "page" : undefined}
        title={t("sidebar.all")}
        onclick={() => void goAll()}
        ondragover={(e) => onDragOverTarget(e, "")}
        ondragleave={() => clearDropTarget("")}
        ondrop={(e) => void onDropTarget(e, "")}
      >
        <span class="nav-icon"><Icon name="files" size={15} /></span>
        <span class="nav-label ellipsis">{t("sidebar.all")}</span>
      </button>
      <button
        class="nav-item"
        class:active={!app.readId && app.view === "review"}
        aria-current={!app.readId && app.view === "review" ? "page" : undefined}
        title={dueLabel()}
        onclick={() => void goView("review")}
      >
        <span class="nav-icon"><Icon name="refresh-cw" size={15} /></span>
        <span class="nav-label ellipsis">{t("sidebar.review")}</span>
        {#if app.dueCount > 0}
          <span class="badge">{app.dueCount}</span>
        {/if}
      </button>
    </nav>

    <div
      class="group"
      class:drop-root={dropTarget === ""}
      role="presentation"
      oncontextmenu={filesMenu}
      ondragover={(e) => onDragOverTarget(e, "")}
      ondragleave={() => clearDropTarget("")}
      ondrop={(e) => void onDropTarget(e, "")}
    >
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
      <nav class="tree" aria-label={t("sidebar.files")} bind:this={treeEl}>
        {#each rows as row (row.key)}
          <div animate:flip={{ duration: reduceMotion ? 0 : 120 }}>
            {#if row.kind === "creator"}
              {@render creatorRow(row)}
            {:else if row.kind === "folder"}
              {@render folderRow(row)}
            {:else}
              {@render pageRow(row)}
            {/if}
          </div>
        {/each}
      </nav>
      {#if app.library.length === 0 && app.folders.length === 0 && !creating}
        <div class="hint">
          <Icon name="info" size={14} />
          <span>{t("sidebar.hint")}</span>
        </div>
      {/if}
    </div>

  </div>
  <div class="foot">
    <nav class="nav" aria-label={t("sidebar.more")}>
      <button
        class="nav-item"
        class:active={!app.readId && app.view === "settings"}
        aria-current={!app.readId && app.view === "settings" ? "page" : undefined}
        title={modKey(",")}
        onclick={() => void goView("settings")}
      >
        <span class="nav-icon"><Icon name="settings" size={15} /></span>
        <span class="nav-label ellipsis">{t("settings.title")}</span>
      </button>
      <button
        class="nav-item"
        class:active={!app.readId && app.view === "trash"}
        aria-current={!app.readId && app.view === "trash" ? "page" : undefined}
        onclick={() => void goView("trash")}
      >
        <span class="nav-icon"><Icon name="trash-2" size={15} /></span>
        <span class="nav-label ellipsis">{t("sidebar.trash")}</span>
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

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} ariaLabel={t("sidebar.files")} onclose={() => (menu = null)} />
{/if}

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
    font-size: var(--fs-lg);
    font-weight: 500;
    letter-spacing: -0.02em;
    color: var(--text);
  }
  .brand-vault {
    font-family: var(--mono);
    font-size: var(--fs-2xs);
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
    font-size: var(--fs-base);
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
    /* Fill the rest of the body so its empty space takes the files context menu and root drops. */
    flex: 1 0 auto;
    border-radius: var(--radius);
  }
  .group.drop-root {
    background: var(--leaf-soft);
    box-shadow: inset 0 0 0 1px var(--leaf);
  }
  .group-title {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 10px 6px;
    font-size: var(--fs-2xs);
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
    font-size: var(--fs-sm);
    color: var(--text-soft);
    gap: 6px;
  }
  .tree-page {
    color: var(--muted);
  }
  .tree-folder.active {
    color: var(--accent-strong);
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
    width: 24px;
    height: 24px;
    border-radius: var(--radius-xs);
    color: var(--muted);
  }
  .tool:hover {
    background: var(--sunken);
    color: var(--text);
  }
  .tree-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0;
  }
  .tree-row.drop {
    background: var(--leaf-soft);
    box-shadow: inset 0 0 0 1px var(--leaf);
    border-radius: var(--radius-sm);
  }
  .chev-btn {
    flex: none;
    width: 18px;
    height: 24px;
    display: grid;
    place-items: center;
    color: var(--muted);
    border-radius: var(--radius-xs);
  }
  .chev-btn:hover {
    color: var(--text);
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
    background: var(--sunken);
  }
  .tree-row:hover .tree-item,
  .tree-row:focus-within .tree-item {
    padding-right: 56px;
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
    font-size: var(--fs-xs);
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
    font-size: var(--fs-sm);
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
    font-size: var(--fs-2xs);
    color: var(--muted);
    text-align: center;
  }
</style>
