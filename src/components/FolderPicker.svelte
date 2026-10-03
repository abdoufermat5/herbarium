<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { api } from "../lib/api";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import { isExcluded, settleFolder, type PickFolderOptions } from "../lib/folder-picker.svelte";
  import { app, errorMessage, refreshAll } from "../lib/state.svelte";

  let { options }: { options: PickFolderOptions } = $props();

  // One dialog instance per request: the options are fixed for its lifetime.
  const fixed = untrack(() => options);
  const exclude = fixed.exclude;
  const prevFocus = document.activeElement as HTMLElement | null;

  let dialogEl: HTMLDivElement | undefined = $state();
  let searchEl: HTMLInputElement | undefined = $state();
  let nameEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();

  let query = $state("");
  let selected = $state<string | null>(
    fixed.initial && !isExcluded(fixed.initial, exclude) ? fixed.initial : null,
  );
  let creating = $state(false);
  let draft = $state("");
  let createError = $state("");
  let busy = $state(false);

  const needle = $derived(query.trim().toLowerCase());
  const folders = $derived([...app.folders].sort((a, b) => a.localeCompare(b)));
  const rows = $derived(
    folders
      .filter((path) => !needle || path.toLowerCase().includes(needle))
      .map((path) => ({
        path,
        depth: path.split("/").length - 1,
        name: path.slice(path.lastIndexOf("/") + 1),
        disabled: isExcluded(path, exclude),
      })),
  );
  const showRoot = $derived(!needle || t("folderPicker.root").toLowerCase().includes(needle));
  const hasExcluded = $derived(!!exclude && folders.some((p) => isExcluded(p, exclude)));

  function cancel() {
    settleFolder(undefined);
  }

  function choose() {
    if (busy) return;
    settleFolder(selected);
  }

  function options_(): HTMLButtonElement[] {
    return Array.from(listEl?.querySelectorAll<HTMLButtonElement>('[role="option"]:not(:disabled)') ?? []);
  }

  function onListKey(e: KeyboardEvent) {
    const items = options_();
    if (items.length === 0) return;
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    let next = -1;
    if (e.key === "ArrowDown") next = Math.min(items.length - 1, i + 1);
    else if (e.key === "ArrowUp") next = Math.max(0, i - 1);
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = items.length - 1;
    if (next < 0) return;
    e.preventDefault();
    items[next].focus();
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      options_()[0]?.focus();
    } else if (e.key === "Enter") {
      e.preventDefault();
      choose();
    }
  }

  // Capture phase: the dialog owns Escape/Tab before App's shortcuts see them.
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (creating) creating = false;
      else cancel();
      return;
    }
    if (e.key !== "Tab" || !dialogEl) return;
    const focusable = Array.from(
      dialogEl.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled)"),
    ).filter((el) => el.tabIndex >= 0);
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || !dialogEl.contains(active))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !dialogEl.contains(active))) {
      e.preventDefault();
      first.focus();
    }
  }

  async function startCreate() {
    creating = true;
    createError = "";
    draft = "";
    await tick();
    nameEl?.focus();
  }

  async function commitCreate() {
    const name = draft.trim().replace(/^\/+|\/+$/g, "");
    if (!name || busy) return;
    busy = true;
    createError = "";
    try {
      const path = await api.createFolder(selected ? `${selected}/${name}` : name);
      await refreshAll();
      selected = path;
      creating = false;
      await tick();
      listEl?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
    } catch (e) {
      createError = t("folderPicker.createFailed", { detail: errorMessage(e) });
    } finally {
      busy = false;
    }
  }

  function onNameKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void commitCreate();
    }
  }

  onMount(() => {
    searchEl?.focus();
    window.addEventListener("keydown", onKey, true);
    void tick().then(() =>
      listEl?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" }),
    );
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (prevFocus?.isConnected) prevFocus.focus();
    };
  });
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && cancel()}>
  <div
    class="dialog card"
    role="dialog"
    aria-modal="true"
    aria-labelledby="picker-title"
    tabindex="-1"
    bind:this={dialogEl}
  >
    <h2 id="picker-title" class="display">{options.title ?? t("folderPicker.title")}</h2>

    <div class="search">
      <Icon name="search" size={14} />
      <input
        bind:this={searchEl}
        bind:value={query}
        type="search"
        placeholder={t("folderPicker.search")}
        aria-label={t("folderPicker.search")}
        autocomplete="off"
        spellcheck="false"
        onkeydown={onSearchKey}
      />
    </div>

    <div
      class="list"
      role="listbox"
      aria-label={t("folderPicker.list")}
      tabindex="-1"
      bind:this={listEl}
      onkeydown={onListKey}
    >
      {#if showRoot}
        <button
          class="row"
          role="option"
          aria-selected={selected === null}
          class:selected={selected === null}
          onclick={() => (selected = null)}
          ondblclick={choose}
        >
          <Icon name="folder-open" size={14} />
          <span class="ellipsis">{t("folderPicker.root")}</span>
          {#if selected === null}<Icon name="check" size={13} />{/if}
        </button>
      {/if}
      {#each rows as row (row.path)}
        <button
          class="row"
          role="option"
          aria-selected={selected === row.path}
          aria-disabled={row.disabled}
          class:selected={selected === row.path}
          disabled={row.disabled}
          style:padding-left={needle ? undefined : `${10 + row.depth * 16}px`}
          title={row.disabled ? t("folderPicker.excluded") : row.path}
          onclick={() => (selected = row.path)}
          ondblclick={choose}
        >
          <Icon name="folder" size={14} />
          <span class="ellipsis">{needle ? row.path : row.name}</span>
          {#if selected === row.path}<Icon name="check" size={13} />{/if}
        </button>
      {/each}
      {#if !showRoot && rows.length === 0}
        <p class="empty">{t("folderPicker.empty")}</p>
      {/if}
    </div>

    {#if hasExcluded}<p class="hint">{t("folderPicker.excluded")}</p>{/if}

    {#if creating}
      <div class="create">
        <input
          bind:this={nameEl}
          bind:value={draft}
          type="text"
          placeholder={t("folderPicker.newName")}
          aria-label={t("folderPicker.newName")}
          autocomplete="off"
          spellcheck="false"
          disabled={busy}
          onkeydown={onNameKey}
        />
        <button class="btn btn-primary" onclick={commitCreate} disabled={busy || !draft.trim()}>
          {t("folderPicker.create")}
        </button>
      </div>
      {#if createError}<p class="error" role="alert">{createError}</p>{/if}
    {/if}

    <footer class="actions">
      {#if !creating}
        <button class="btn btn-ghost new" onclick={startCreate}>
          <Icon name="folder-plus" size={14} />
          {t("folderPicker.new")}
        </button>
      {/if}
      <span class="spacer"></span>
      <button class="btn" onclick={cancel}>{t("common.cancel")}</button>
      <button class="btn btn-primary" onclick={choose} disabled={busy}>{t("folderPicker.choose")}</button>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 12vh 24px 24px;
    background: var(--scrim);
    animation: picker-fade var(--t-fast) var(--ease-out);
  }
  .dialog {
    width: 440px;
    max-width: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 22px 18px;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    outline: none;
    animation: picker-pop var(--t-med) var(--ease-out);
  }
  h2 {
    font-size: var(--fs-xl);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    border: 1px solid var(--border-input);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--muted);
  }
  .search input {
    flex: 1;
    min-width: 0;
    padding: 8px 0;
    border: none;
    outline: none;
    background: transparent;
    font-size: var(--fs-sm);
  }
  .search:focus-within {
    border-color: var(--border-input-hover);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    max-height: 280px;
    overflow-y: auto;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--sunken);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: var(--radius-xs, 6px);
    background: transparent;
    color: var(--text);
    font-size: var(--fs-sm);
    text-align: left;
  }
  .row .ellipsis {
    flex: 1;
    min-width: 0;
  }
  .row:hover:not(:disabled) {
    background: var(--surface);
  }
  .row.selected {
    background: var(--leaf-soft);
    color: var(--text);
  }
  .row:disabled {
    opacity: 0.45;
  }
  .row:focus-visible {
    outline: 2px solid var(--leaf);
    outline-offset: -2px;
  }
  .empty,
  .hint {
    font-size: var(--fs-xs);
    color: var(--muted);
    padding: 6px 8px;
  }
  .hint {
    padding: 0;
  }
  .create {
    display: flex;
    gap: 8px;
  }
  .create input {
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    border: 1px solid var(--border-input);
    border-radius: var(--radius-sm);
    background: var(--surface);
    font-size: var(--fs-sm);
  }
  .error {
    font-size: var(--fs-xs);
    color: var(--danger);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 2px;
  }
  .spacer {
    flex: 1;
  }
  @keyframes picker-fade {
    from {
      opacity: 0;
    }
  }
  @keyframes picker-pop {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .overlay,
    .dialog {
      animation: none;
    }
  }
</style>
