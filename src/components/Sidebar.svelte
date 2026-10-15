<script lang="ts">
  import { publishSite } from "../lib/exports";
  import { folderLook } from "../lib/appearance";
  import { onMount, tick, untrack } from "svelte";
  import { flip } from "svelte/animate";
  import {
    app,
    clearFilters,
    deletePages,
    dueLabel,
    duplicatePage,
    errorMessage,
    goAll,
    runSearch,
    deleteSavedSearch,
    openInPath,
    createPath,
    renamePath,
    deletePath,
    movePageInPath,
    setPageInPath,
    goView,
    inFolder,
    movePages,
    openPage,
    pendingFiles,
    refreshAll,
    reloadPages,
    showFolder,
    switchVault,
    toast,
  } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import Icon from "../lib/Icon.svelte";
  import Logo from "../lib/Logo.svelte";
  import { modKey, vaultLabels, vaultName as nameOf } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import { currentEditor, loadEditors } from "../lib/editors.svelte";
  import { confirmAction } from "../lib/confirm.svelte";
  import { pickFolder } from "../lib/folder-picker.svelte";
  import { shortcutHint } from "../lib/shortcuts";
  import type { PageMeta } from "../lib/types";
  import ContextMenu from "./ContextMenu.svelte";
  import DropdownMenu, { type DropdownMenuItem } from "./DropdownMenu.svelte";

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
  let asideEl: HTMLElement | undefined = $state();

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

  /** Tree geometry: each level indents by INDENT; labels clear the chevron column. */
  const INDENT = 16;
  const LABEL_GAP = 18;
  function indent(depth: number): number {
    return 4 + depth * INDENT;
  }

  function collapseAll() {
    persistExpanded(new Set());
  }

  /* ------------------------------------------------------ section folding */

  type Section = "paths" | "saved" | "files";
  const SECTIONS_KEY = "herbarium.sidebarSections";

  function loadFolded(): Set<Section> {
    try {
      return new Set(JSON.parse(localStorage.getItem(SECTIONS_KEY) ?? "[]"));
    } catch {
      return new Set();
    }
  }

  let folded = $state(loadFolded());

  function toggleSection(section: Section) {
    const next = new Set(folded);
    if (next.has(section)) next.delete(section);
    else next.add(section);
    folded = next;
    try {
      localStorage.setItem(SECTIONS_KEY, JSON.stringify([...next]));
    } catch {
      // Folding still works for this session.
    }
  }

  /* --------------------------------------------------------- resizable width */

  const WIDTH_KEY = "herbarium.sidebarWidth";
  const MIN_WIDTH = 200;
  const MAX_WIDTH = 480;
  let width = $state<number | null>(null);
  let resizing = $state(false);

  function applyWidth(next: number | null) {
    width = next === null ? null : Math.round(Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, next)));
    const root = document.documentElement;
    if (width === null) root.style.removeProperty("--sidebar-w");
    else root.style.setProperty("--sidebar-w", `${width}px`);
  }

  function saveWidth() {
    try {
      if (width === null) localStorage.removeItem(WIDTH_KEY);
      else localStorage.setItem(WIDTH_KEY, String(width));
    } catch {
      // The width still applies until the app restarts.
    }
  }

  onMount(() => {
    const stored = Number(localStorage.getItem(WIDTH_KEY));
    if (stored > 0) applyWidth(stored);
  });

  function currentWidth(): number {
    return width ?? asideEl?.getBoundingClientRect().width ?? MIN_WIDTH;
  }

  function onResizeStart(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startWidth = currentWidth();
    resizing = true;
    const move = (ev: PointerEvent) => applyWidth(startWidth + ev.clientX - startX);
    const end = () => {
      resizing = false;
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      handle.removeEventListener("pointercancel", end);
      saveWidth();
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  }

  function onResizeKey(e: KeyboardEvent) {
    const step = e.shiftKey ? 48 : 16;
    if (e.key === "ArrowLeft") applyWidth(currentWidth() - step);
    else if (e.key === "ArrowRight") applyWidth(currentWidth() + step);
    else if (e.key === "Home") applyWidth(MIN_WIDTH);
    else if (e.key === "End") applyWidth(MAX_WIDTH);
    else return;
    e.preventDefault();
    saveWidth();
  }

  function resetWidth() {
    applyWidth(null);
    saveWidth();
  }

  /* ------------------------------------------------- reveal the open page */

  // Like Obsidian's "reveal active file": opening a page unfolds its folders
  // and scrolls its row into view, so the tree always shows where you are.
  $effect(() => {
    const id = app.readId;
    if (!id) return;
    const folder = untrack(() => app.library.find((p) => p.id === id)?.folder ?? "");
    untrack(() => {
      if (folder) {
        const parts = folder.split("/");
        const missing = parts
          .map((_, i) => parts.slice(0, i + 1).join("/"))
          .filter((path) => !expanded.has(path));
        if (missing.length > 0) persistExpanded(new Set([...expanded, ...missing]));
      }
      if (folded.has("files")) return;
      void tick().then(() => {
        treeEl
          ?.querySelector<HTMLElement>(`[data-key="${cssEscape(`p:${id}`)}"]`)
          ?.scrollIntoView({ block: "nearest" });
      });
    });
  });

  const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
  let reduceMotion = $state(motionQuery.matches);
  $effect(() => {
    const onChange = (e: MediaQueryListEvent) => (reduceMotion = e.matches);
    motionQuery.addEventListener("change", onChange);
    return () => motionQuery.removeEventListener("change", onChange);
  });

  const vaultName = $derived(nameOf(app.config?.vaultPath ?? ""));

  /** The open vault first, then the other recent ones. */
  const vaults = $derived.by(() => {
    const current = app.config?.vaultPath;
    const recent = app.config?.recentVaults ?? [];
    return current ? [current, ...recent.filter((p) => p !== current)] : recent;
  });
  /** Labels for `vaults`, told apart when two share a folder name; the first is the open one. */
  const labels = $derived(vaultLabels(vaults));

  let switching = $state(false);
  const vaultItems = $derived.by((): DropdownMenuItem[] => {
    return [
      { header: true, label: t("sidebar.vaults") },
      ...vaults.map((path, i) => ({
        id: path,
        label: labels[i],
        icon: "folder" as const,
        checked: path === app.config?.vaultPath,
        disabled: switching,
        onclick: () => void openVault(path),
      })),
      { divider: true },
      { id: "manage", label: t("sidebar.manageVaults"), icon: "settings", onclick: () => void goView("settings") },
    ];
  });

  async function openVault(path: string) {
    if (switching) return;
    switching = true;
    try {
      if (await switchVault(path)) toast(t("toast.vaultOpenedNamed", { name: nameOf(path) }), "success");
    } catch (e) {
      console.error(e);
      toast(t("settings.vaultOpenFailed", { detail: errorMessage(e) }), "error");
    } finally {
      switching = false;
    }
  }

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
      { id: "look", label: t("look.menu"), icon: "leaf", onclick: () => (app.lookEdit = { kind: "folder", key: path }) },
      { id: "publish", label: t("export.siteFolder"), icon: "upload", onclick: () => void publishSite(path) },
      { id: "move", label: t("folderPicker.move"), icon: "folder", onclick: () => void moveFolder(row) },
      { id: "rename", label: t("sidebar.rename"), icon: "folder", shortcut: shortcutHint("renameItem"), onclick: () => startRename(row) },
      { id: "delete", label: t("sidebar.deleteFolder"), icon: "trash-2", danger: true, shortcut: shortcutHint("deleteItem"), onclick: () => void confirmDeleteFolder(path, row.name) },
    ]);
  }

  /** A folder's own icon, and its colour (or its nearest coloured parent's). */
  function folderIcon(path: string | undefined): string | undefined {
    return path ? app.appearance.folders[path]?.icon : undefined;
  }
  function folderColor(path: string | undefined): string | undefined {
    const color = folderLook(path).color;
    return color ? `var(--c-${color})` : undefined;
  }

  /* ---------------------------------------------------------- reading paths */

  let openPaths = $state<Set<string>>(new Set());
  /** Inline name field: a new path, or renaming one. */
  let pathDraft = $state<{ id: string | null; name: string } | null>(null);

  function togglePathOpen(id: string) {
    const next = new Set(openPaths);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    openPaths = next;
  }

  function pageTitle(id: string): string | null {
    return app.library.find((p) => p.id === id)?.title ?? null;
  }

  async function commitPathDraft() {
    const draft = pathDraft;
    if (!draft) return;
    const name = draft.name.trim();
    pathDraft = null;
    if (!name) return;
    if (draft.id) {
      await renamePath(draft.id, name);
    } else {
      const path = await createPath(name);
      if (path) openPaths = new Set([...openPaths, path.id]);
    }
  }

  function onPathDraftKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void commitPathDraft();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      pathDraft = null;
    }
  }

  function pathMenu(e: MouseEvent, id: string, name: string) {
    e.preventDefault();
    menu = {
      ...anchor(e),
      items: [
        { id: "rename", label: t("sidebar.rename"), icon: "file-text", onclick: () => (pathDraft = { id, name }) },
        { id: "delete", label: t("paths.delete"), icon: "trash-2", danger: true, onclick: () => void deletePath(id) },
      ],
    };
  }

  function pathPageMenu(e: MouseEvent, pathId: string, pageId: string, index: number, count: number) {
    e.preventDefault();
    menu = {
      ...anchor(e),
      items: [
        { id: "open", label: t("common.open"), icon: "file-text", onclick: () => void openInPath(pathId, pageId) },
        { divider: true },
        { id: "up", label: t("paths.moveUp"), disabled: index === 0, onclick: () => void movePageInPath(pathId, pageId, -1) },
        { id: "down", label: t("paths.moveDown"), disabled: index === count - 1, onclick: () => void movePageInPath(pathId, pageId, 1) },
        { divider: true },
        { id: "remove", label: t("paths.remove"), icon: "x", onclick: () => void setPageInPath(pathId, pageId, false) },
      ],
    };
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

{#snippet guides(depth: number)}
  {#each { length: depth } as _, i}
    <span class="guide" style:left="{indent(i) + 8}px" aria-hidden="true"></span>
  {/each}
{/snippet}

{#snippet creatorRow(row: TreeRow)}
  <div class="tree-row">
    {@render guides(row.depth)}
    <span class="nav-item tree-item creator" style:padding-left="{indent(row.depth) + LABEL_GAP}px">
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
    {@render guides(row.depth)}
    {#if renaming?.kind === "folder" && renaming.path === row.path}
      <span class="nav-item tree-item tree-folder" style:padding-left="{indent(row.depth) + LABEL_GAP}px">
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
        style:padding-left="{indent(row.depth) + LABEL_GAP}px"
        title={row.path}
        data-key={row.key}
        tabindex={tabbableKey === row.key ? 0 : -1}
        aria-expanded={row.expanded}
        aria-current={app.folderFilter === row.path ? "true" : undefined}
        onfocus={() => (focusedKey = row.key)}
        onkeydown={onTreeKey}
        onclick={() => void showFolder(row.path!)}
        oncontextmenu={(e) => folderMenu(e, row)}
      >
        <span class="nav-icon" style:color={folderColor(row.path)}>
          {#if folderIcon(row.path)}<span class="row-emoji">{folderIcon(row.path)}</span>{:else}<Icon name={row.expanded ? "folder-open" : "folder"} size={14} />{/if}
        </span>
        <span class="nav-label ellipsis">{row.name}</span>
      </button>
      <!-- After the label so it paints above it; positioned into the indent. -->
      <button
        type="button"
        class="chev-btn"
        style:left="{indent(row.depth)}px"
        tabindex="-1"
        aria-label={row.expanded ? t("sidebar.collapse") : t("sidebar.expand")}
        aria-expanded={row.expanded}
        onclick={() => toggleFolder(row.path!)}
      >
        <span class="chev" class:open={row.expanded}><Icon name="chevron-right" size={10} /></span>
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
    {@render guides(row.depth)}
    {#if renaming?.kind === "page" && renaming.id === row.id}
      <span class="nav-item tree-item tree-page" style:padding-left="{indent(row.depth) + LABEL_GAP}px">
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
        style:padding-left="{indent(row.depth) + LABEL_GAP}px"
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
        <span class="nav-icon">
          {#if row.page?.ext?.look?.icon}<span class="row-emoji">{row.page.ext.look.icon}</span>{:else}<Icon name="file-text" size={14} />{/if}
        </span>
        <span class="nav-label ellipsis">{row.name}</span>
      </button>
    {/if}
  </div>
{/snippet}

{#snippet sectionHead(section: Section, label: string)}
  <button
    type="button"
    class="section-title"
    aria-expanded={!folded.has(section)}
    onclick={() => toggleSection(section)}
  >
    <span class="chev" class:open={!folded.has(section)}><Icon name="chevron-right" size={9} /></span>
    <span class="ellipsis">{label}</span>
  </button>
{/snippet}

<aside class="sidebar" class:resizing bind:this={asideEl}>
  <div class="head">
    {#if vaults.length > 1}
      <DropdownMenu items={vaultItems} ariaLabel={t("sidebar.switchVault")} class="brand-switch">
        {#snippet trigger({ open, toggle })}
          <button
            class="brand brand-button"
            class:open
            aria-haspopup="menu"
            aria-expanded={open}
            title={`${t("sidebar.switchVault")} — ${app.config?.vaultPath ?? ""}`}
            onclick={toggle}
          >
            <Logo size={22} />
            <span class="brand-name ellipsis">{labels[0]}</span>
            {#if switching}<span class="spinner small"></span>{:else}<Icon name="arrow-up-down" size={12} />{/if}
          </button>
        {/snippet}
      </DropdownMenu>
    {:else}
      <div class="brand" title={app.config?.vaultPath ?? ""}>
        <Logo size={22} />
        <span class="brand-name ellipsis">{vaultName || "Herbarium"}</span>
      </div>
    {/if}
    <button
      class="search-btn"
      title="{t('sidebar.search')} · / {t('sidebar.toSearch')} · i {t('sidebar.toImport')}"
      onclick={() => (app.paletteOpen = true)}>
      <Icon name="search" size={13} />
      <span>{t("sidebar.search")}</span>
      <kbd class="kbd">{modKey("K")}</kbd>
    </button>
  </div>

  <div class="body">
    <nav class="nav" aria-label={t("sidebar.library")}>
      <button
        class="nav-item"
        class:active={!app.readId && app.view === "today"}
        aria-current={!app.readId && app.view === "today" ? "page" : undefined}
        onclick={() => void goView("today")}
      >
        <span class="nav-icon"><Icon name="sun" size={14} /></span>
        <span class="nav-label ellipsis">{t("sidebar.today")}</span>
      </button>
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
        <span class="nav-icon"><Icon name="files" size={14} /></span>
        <span class="nav-label ellipsis">{t("sidebar.all")}</span>
      </button>
      <button
        class="nav-item"
        class:active={!app.readId && app.view === "review"}
        aria-current={!app.readId && app.view === "review" ? "page" : undefined}
        title={dueLabel()}
        onclick={() => void goView("review")}
      >
        <span class="nav-icon"><Icon name="refresh-cw" size={14} /></span>
        <span class="nav-label ellipsis">{t("sidebar.review")}</span>
        {#if app.dueCount > 0}
          <span class="badge">{app.dueCount}</span>
        {/if}
      </button>
      <button
        class="nav-item"
        class:active={!app.readId && app.view === "graph"}
        aria-current={!app.readId && app.view === "graph" ? "page" : undefined}
        onclick={() => void goView("graph")}
      >
        <span class="nav-icon"><Icon name="hash" size={14} /></span>
        <span class="nav-label ellipsis">{t("sidebar.graph")}</span>
      </button>
    </nav>

    <section class="section">
      <div class="section-head">
        {@render sectionHead("paths", t("sidebar.paths"))}
        <div class="tools">
          <button
            class="tool"
            title={t("paths.new")}
            aria-label={t("paths.new")}
            onclick={() => {
              if (folded.has("paths")) toggleSection("paths");
              pathDraft = { id: null, name: "" };
            }}
          >
            <Icon name="plus" size={13} />
          </button>
        </div>
      </div>
      {#if !folded.has("paths")}
        <nav class="nav" aria-label={t("sidebar.paths")}>
          {#if pathDraft && !pathDraft.id}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="path-input"
              bind:value={pathDraft.name}
              onkeydown={onPathDraftKey}
              onblur={() => void commitPathDraft()}
              placeholder={t("paths.namePlaceholder")}
              aria-label={t("paths.nameLabel")}
              maxlength="100"
              autofocus
            />
          {/if}
          {#each app.paths as path (path.id)}
            {#if pathDraft?.id === path.id}
              <!-- svelte-ignore a11y_autofocus -->
              <input
                class="path-input"
                bind:value={pathDraft.name}
                onkeydown={onPathDraftKey}
                onblur={() => void commitPathDraft()}
                aria-label={t("paths.nameLabel")}
                maxlength="100"
                autofocus
              />
            {:else}
              <button
                class="nav-item path-head"
                aria-expanded={openPaths.has(path.id)}
                title={path.description || path.name}
                onclick={() => togglePathOpen(path.id)}
                oncontextmenu={(e) => pathMenu(e, path.id, path.name)}
              >
                <span class="chev" class:open={openPaths.has(path.id)}><Icon name="chevron-right" size={10} /></span>
                <span class="nav-label ellipsis">{path.name}</span>
                <span class="count">{path.pages.length}</span>
              </button>
            {/if}
            {#if openPaths.has(path.id)}
              <div class="path-pages">
                {#each path.pages as pageId, i (pageId)}
                  {@const title = pageTitle(pageId)}
                  <button
                    class="nav-item path-page"
                    class:active={app.readId === pageId && app.pathId === path.id}
                    class:missing={title === null}
                    disabled={title === null}
                    title={title ?? t("paths.missing")}
                    onclick={() => void openInPath(path.id, pageId)}
                    oncontextmenu={(e) => pathPageMenu(e, path.id, pageId, i, path.pages.length)}
                  >
                    <span class="path-num">{i + 1}</span>
                    <span class="nav-label ellipsis">{title ?? pageId}</span>
                  </button>
                {:else}
                  <p class="path-empty">{t("paths.empty")}</p>
                {/each}
              </div>
            {/if}
          {:else}
            {#if !pathDraft}
              <p class="path-empty">{t("paths.none")}</p>
            {/if}
          {/each}
        </nav>
      {/if}
    </section>

    {#if app.savedSearches.length > 0}
      <section class="section">
        <div class="section-head">
          {@render sectionHead("saved", t("sidebar.savedSearches"))}
        </div>
        {#if !folded.has("saved")}
          <nav class="nav" aria-label={t("sidebar.savedSearches")}>
            {#each app.savedSearches as s (s.name)}
              {@const active = !app.readId && app.view === "list" && app.search === s.query}
              <div class="saved-row">
                <button
                  class="nav-item"
                  class:active
                  aria-current={active ? "page" : undefined}
                  title={s.query}
                  onclick={() => void runSearch(s.query)}
                >
                  <span class="nav-icon"><Icon name="search" size={13} /></span>
                  <span class="nav-label ellipsis">{s.name}</span>
                </button>
                <button
                  class="tool saved-remove"
                  title={t("sidebar.removeSearch", { name: s.name })}
                  aria-label={t("sidebar.removeSearch", { name: s.name })}
                  onclick={() => void deleteSavedSearch(s.name)}
                >
                  <Icon name="x" size={12} />
                </button>
              </div>
            {/each}
          </nav>
        {/if}
      </section>
    {/if}

    <section
      class="section files"
      class:drop-root={dropTarget === ""}
      role="presentation"
      oncontextmenu={filesMenu}
      ondragover={(e) => onDragOverTarget(e, "")}
      ondragleave={() => clearDropTarget("")}
      ondrop={(e) => void onDropTarget(e, "")}
    >
      <div class="section-head">
        {@render sectionHead("files", t("sidebar.files"))}
        <div class="tools">
          <button class="tool" title={t("sidebar.newPage")} aria-label={t("sidebar.newPage")} onclick={() => { if (folded.has("files")) toggleSection("files"); startCreate("page", ""); }}>
            <Icon name="file-plus" size={13} />
          </button>
          <button class="tool" title={t("sidebar.newFolder")} aria-label={t("sidebar.newFolder")} onclick={() => { if (folded.has("files")) toggleSection("files"); startCreate("folder", ""); }}>
            <Icon name="folder-plus" size={13} />
          </button>
          <button class="tool" title={t("sidebar.collapseAll")} aria-label={t("sidebar.collapseAll")} disabled={expanded.size === 0} onclick={collapseAll}>
            <Icon name="collapse-all" size={12} />
          </button>
        </div>
      </div>
      {#if !folded.has("files")}
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
      {/if}
    </section>
  </div>

  <div class="foot">
    <button
      class="foot-btn"
      class:active={!app.readId && app.view === "settings"}
      aria-current={!app.readId && app.view === "settings" ? "page" : undefined}
      title="{t('settings.title')} ({modKey(',')})"
      onclick={() => void goView("settings")}
    >
      <Icon name="settings" size={14} />
      <span class="ellipsis">{t("settings.title")}</span>
    </button>
    <button
      class="foot-btn"
      class:active={!app.readId && app.view === "trash"}
      aria-current={!app.readId && app.view === "trash" ? "page" : undefined}
      title={t("sidebar.trash")}
      onclick={() => void goView("trash")}
    >
      <Icon name="trash-2" size={14} />
      <span class="ellipsis">{t("sidebar.trash")}</span>
    </button>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="resizer"
    role="separator"
    tabindex="0"
    aria-orientation="vertical"
    aria-label={t("sidebar.resize")}
    aria-valuemin={MIN_WIDTH}
    aria-valuemax={MAX_WIDTH}
    aria-valuenow={width ?? undefined}
    title={t("sidebar.resize")}
    onpointerdown={onResizeStart}
    onkeydown={onResizeKey}
    ondblclick={resetWidth}
  ></div>
</aside>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} ariaLabel={t("sidebar.files")} onclose={() => (menu = null)} />
{/if}

<style>
  .sidebar {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--raised);
    border-right: 1px solid var(--border);
    font-size: var(--fs-sm);
  }
  .sidebar.resizing {
    user-select: none;
  }

  /* ---------------------------------------------------------------- head */

  .head {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 8px 8px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 4px 6px;
  }
  .brand-name {
    flex: 1;
    min-width: 0;
    font-family: var(--font-display);
    font-size: var(--fs-md);
    font-weight: 500;
    letter-spacing: -0.01em;
    color: var(--text);
  }
  /* With several vaults the brand opens the vault switcher. */
  .sidebar :global(.brand-switch) {
    display: flex;
  }
  .sidebar :global(.brand-switch .dropdown-menu) {
    left: 0;
    right: 0;
    max-width: none;
  }
  .brand-button {
    flex: 1;
    min-width: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--muted);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      background var(--t-fast, 0.12s),
      border-color var(--t-fast, 0.12s);
  }
  .brand-button:hover,
  .brand-button.open {
    background: var(--sunken);
  }
  .brand-button:focus-visible {
    outline: 2px solid var(--accent-ring);
    outline-offset: 1px;
  }
  .brand-button .spinner.small {
    width: 12px;
    height: 12px;
  }
  .search-btn {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 4px 0 8px;
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

  /* ---------------------------------------------------------------- body */

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 2px 6px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
    padding: 3px 8px;
    border-radius: var(--radius-xs);
    font-size: var(--fs-sm);
    line-height: 1.35;
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
  .nav-item.drop {
    background: var(--leaf-soft);
    box-shadow: inset 0 0 0 1px var(--leaf);
  }
  .nav-icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 16px;
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

  /* ------------------------------------------------------------ sections */

  .section {
    display: flex;
    flex-direction: column;
    gap: 1px;
    border-radius: var(--radius-sm);
  }
  /* Files fill the rest of the body so its empty space takes the context menu and root drops. */
  .section.files {
    flex: 1 0 auto;
  }
  .section.drop-root {
    background: var(--leaf-soft);
    box-shadow: inset 0 0 0 1px var(--leaf);
  }
  .section-head {
    position: sticky;
    top: -2px;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 2px;
    background: var(--raised);
  }
  .section-title {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 6px 0 4px;
    border-radius: var(--radius-xs);
    font-size: var(--fs-2xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    text-align: left;
  }
  .section-title:hover {
    color: var(--text);
  }
  .section-title .chev {
    width: 12px;
  }
  .tools {
    display: flex;
    gap: 1px;
    opacity: 0.55;
    transition: opacity var(--t-fast) var(--ease-out);
  }
  .section-head:hover .tools,
  .tools:focus-within {
    opacity: 1;
  }
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
  .tool:hover:not(:disabled) {
    background: var(--sunken);
    color: var(--text);
  }
  .tool:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .chev {
    display: grid;
    place-items: center;
    transition: transform var(--t-fast) var(--ease-out);
  }
  .chev.open {
    transform: rotate(90deg);
  }

  /* ---------------------------------------------------------------- tree */

  .tree {
    display: flex;
    flex-direction: column;
  }
  .tree-row {
    position: relative;
    display: flex;
    align-items: center;
  }
  .tree-item {
    flex: 1;
    min-width: 0;
    padding-right: 8px;
    color: var(--text-soft);
  }
  .tree-folder {
    color: var(--text);
  }
  .tree-folder.active {
    color: var(--accent-strong);
  }
  .tree-row.drop {
    background: var(--leaf-soft);
    box-shadow: inset 0 0 0 1px var(--leaf);
    border-radius: var(--radius-xs);
  }
  /* Indent guides, one per ancestor level, like Obsidian's file explorer. */
  .guide {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--border-strong);
    pointer-events: none;
  }
  .tree:hover .guide {
    background: var(--border-hover);
  }
  .chev-btn {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 16px;
    height: 22px;
    display: grid;
    place-items: center;
    color: var(--muted);
    border-radius: var(--radius-xs);
  }
  .chev-btn:hover {
    color: var(--text);
    background: var(--border);
  }
  .row-actions {
    position: absolute;
    right: 2px;
    top: 50%;
    transform: translateY(-50%);
    opacity: 0;
    pointer-events: none;
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
    margin: 4px 2px;
    padding: 10px;
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--muted);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-sm);
  }
  .hint :global(svg) {
    margin-top: 2px;
  }

  /* ------------------------------------------------------- reading paths */

  .path-head .chev {
    width: 16px;
    color: var(--muted);
  }
  .count {
    flex: none;
    font-family: var(--mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .path-pages {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin-left: 15px;
    padding-left: 6px;
    border-left: 1px solid var(--border-strong);
  }
  .path-input {
    width: 100%;
    margin: 2px 0;
    font-size: var(--fs-sm);
  }
  .path-page {
    padding-left: 6px;
  }
  .path-page.missing {
    opacity: 0.55;
    text-decoration: line-through;
  }
  .path-num {
    flex: none;
    min-width: 14px;
    font-family: var(--mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
    text-align: right;
  }
  .path-empty {
    margin: 2px 0 4px 10px;
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  /* ------------------------------------------------------ saved searches */

  .saved-row {
    position: relative;
    display: flex;
    align-items: center;
  }
  .saved-row .nav-item {
    flex: 1;
    min-width: 0;
    padding-right: 26px;
  }
  .saved-remove {
    position: absolute;
    right: 2px;
    width: 22px;
    height: 22px;
    opacity: 0;
  }
  .saved-row:hover .saved-remove,
  .saved-remove:focus-visible {
    opacity: 1;
  }

  /* ---------------------------------------------------------------- foot */

  .foot {
    flex: none;
    display: flex;
    gap: 2px;
    padding: 6px;
    border-top: 1px solid var(--border);
  }
  .foot-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 28px;
    padding: 0 8px;
    border-radius: var(--radius-xs);
    color: var(--muted);
    font-size: var(--fs-xs);
    transition:
      background var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }
  .foot-btn:hover {
    background: var(--sunken);
    color: var(--text);
  }
  .foot-btn.active {
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--border);
    color: var(--accent-strong);
  }

  /* --------------------------------------------------------- resize grip */

  .resizer {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 2;
    width: 6px;
    cursor: col-resize;
    touch-action: none;
  }
  .resizer::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 2px;
    width: 2px;
    background: transparent;
    transition: background var(--t-fast) var(--ease-out);
  }
  .resizer:hover::after,
  .resizer:focus-visible::after,
  .sidebar.resizing .resizer::after {
    background: var(--border-hover);
  }
  .resizer:focus-visible {
    outline: none;
  }

  .row-emoji {
    font-size: 13px;
    line-height: 1;
  }
</style>
