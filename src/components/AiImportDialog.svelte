<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { app, errorMessage, reloadPages, toast } from "../lib/state.svelte";
  import { fmtDate } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import type { AiExportListing } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { confirmState } from "../lib/confirm.svelte";
  import { folderPickerState } from "../lib/folder-picker.svelte";

  type Step = "pick" | "scanning" | "choose" | "importing";

  let step = $state<Step>("pick");
  let listing = $state<AiExportListing | null>(null);
  let selected = $state<Set<string>>(new Set());
  let filter = $state("");
  let folder = $state("Imported");
  let error = $state("");
  let closeBtn: HTMLButtonElement | undefined = $state();

  const fresh = $derived(listing?.candidates.filter((c) => !c.alreadyImported) ?? []);
  const shown = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    const all = listing?.candidates ?? [];
    if (!q) return all;
    return all.filter(
      (c) =>
        c.title.toLowerCase().includes(q) ||
        c.conversation.toLowerCase().includes(q) ||
        c.prompt.toLowerCase().includes(q),
    );
  });
  const allShownSelected = $derived(
    shown.some((c) => !c.alreadyImported) && shown.every((c) => c.alreadyImported || selected.has(c.key)),
  );

  function close() {
    if (step === "scanning" || step === "importing") return;
    app.aiImportOpen = false;
  }

  async function pickFile() {
    error = "";
    const path = await open({
      title: t("aiImport.pickTitle"),
      filters: [{ name: t("aiImport.filterName"), extensions: ["zip", "json"] }],
    });
    if (typeof path !== "string") return;
    step = "scanning";
    try {
      listing = await api.scanAiExport(path);
      selected = new Set(listing.candidates.filter((c) => !c.alreadyImported).map((c) => c.key));
      step = "choose";
    } catch (e) {
      error = errorMessage(e);
      step = "pick";
    }
  }

  function toggle(key: string) {
    const next = new Set(selected);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    selected = next;
  }

  function toggleAllShown() {
    const next = new Set(selected);
    const keys = shown.filter((c) => !c.alreadyImported).map((c) => c.key);
    if (allShownSelected) keys.forEach((k) => next.delete(k));
    else keys.forEach((k) => next.add(k));
    selected = next;
  }

  async function runImport() {
    if (selected.size === 0) return;
    step = "importing";
    try {
      const report = await api.importAiExport([...selected], folder.trim() || null);
      await reloadPages(true);
      toast(t("aiImport.done", { imported: report.imported, skipped: report.skipped }), "success");
      if (report.errors.length > 0) {
        toast(`${t("aiImport.someFailed", { count: report.errors.length })}: ${report.errors[0]}`, "error");
      }
      app.aiImportOpen = false;
    } catch (e) {
      error = errorMessage(e);
      step = "choose";
    }
  }

  // Capture phase: the window-level shortcuts run first otherwise, and their
  // Escape would also close the reader or leave Settings.
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !app.paletteOpen && !confirmState.pending && !folderPickerState.pending) {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  onMount(() => closeBtn?.focus());
</script>

<svelte:window onkeydowncapture={onKey} />

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && close()}>
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="ai-import-title">
    <header class="dlg-head">
      <h2 id="ai-import-title" class="display">{t("aiImport.title")}</h2>
      <button
        bind:this={closeBtn}
        class="btn btn-ghost btn-icon"
        aria-label={t("common.close")}
        disabled={step === "scanning" || step === "importing"}
        onclick={close}
      >
        <Icon name="x" size={16} />
      </button>
    </header>

    {#if step === "pick" || step === "scanning"}
      <p class="lead">{t("aiImport.lead")}</p>
      <ol class="how">
        <li><strong>Claude</strong> · {t("aiImport.howClaude")}</li>
        <li><strong>ChatGPT</strong> · {t("aiImport.howChatgpt")}</li>
      </ol>
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <footer class="foot">
        {#if step === "scanning"}
          <span class="spinner" role="status" aria-label={t("aiImport.scanning")}></span>
          <span class="muted">{t("aiImport.scanning")}</span>
        {/if}
        <button class="btn btn-primary" onclick={pickFile} disabled={step === "scanning"}>
          <Icon name="upload" size={13} />{t("aiImport.pick")}
        </button>
      </footer>
    {:else if listing}
      <p class="summary">
        {t("aiImport.found", {
          count: listing.candidates.length,
          conversations: listing.conversations,
          fresh: fresh.length,
        })}
        {#if listing.unsupported > 0}<span class="muted">· {t("aiImport.unsupported", { count: listing.unsupported })}</span>{/if}
      </p>

      {#if listing.candidates.length === 0}
        <p class="muted">{t("aiImport.none")}</p>
      {:else}
        <div class="controls">
          <input type="search" bind:value={filter} placeholder={t("aiImport.filter")} aria-label={t("aiImport.filter")} />
          <button class="btn btn-sm btn-ghost" onclick={toggleAllShown} disabled={!shown.some((c) => !c.alreadyImported)}>
            {allShownSelected ? t("aiImport.selectNone") : t("aiImport.selectAll")}
          </button>
        </div>
        <ul class="list" aria-label={t("aiImport.listLabel")}>
          {#each shown as c (c.key)}
            <li class:done={c.alreadyImported}>
              <label>
                <input
                  type="checkbox"
                  checked={c.alreadyImported || selected.has(c.key)}
                  disabled={c.alreadyImported}
                  onchange={() => toggle(c.key)}
                />
                <span class="main">
                  <span class="title ellipsis">{c.title}</span>
                  <span class="sub ellipsis">
                    {c.tool} · {fmtDate(c.createdAt)}{#if c.conversation}{" · "}{c.conversation}{/if}
                    {#if c.alreadyImported}{" · "}{t("aiImport.already")}{/if}
                  </span>
                </span>
              </label>
            </li>
          {/each}
        </ul>
        <label class="folder">
          <span>{t("aiImport.folder")}</span>
          <input type="text" bind:value={folder} placeholder={t("import.rootFolder")} />
        </label>
      {/if}
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <footer class="foot">
        {#if step === "importing"}
          <span class="spinner" role="status" aria-label={t("aiImport.importing")}></span>
        {/if}
        <button class="btn" onclick={() => (step = "pick")} disabled={step === "importing"}>{t("common.back")}</button>
        <button class="btn btn-primary" onclick={runImport} disabled={selected.size === 0 || step === "importing"}>
          {t("aiImport.import", { count: selected.size })}
        </button>
      </footer>
    {/if}
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
    padding: 10vh 24px 24px;
    background: var(--scrim);
  }
  .dialog {
    width: 620px;
    max-width: 100%;
    max-height: 100%;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 24px 28px 22px;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
  }
  .dlg-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    font-size: var(--fs-2xl);
  }
  .lead,
  .summary {
    margin: 0;
    color: var(--text-soft);
    font-size: var(--fs-sm);
  }
  .how {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--text-soft);
  }
  .muted {
    color: var(--muted);
  }
  .err {
    margin: 0;
    color: var(--danger);
    font-size: var(--fs-sm);
  }
  .controls {
    display: flex;
    gap: 8px;
  }
  .controls input {
    flex: 1;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    max-height: 40vh;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .list li + li {
    border-top: 1px solid var(--border);
  }
  .list label {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    cursor: pointer;
  }
  .list li.done {
    opacity: 0.6;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .sub {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--fs-sm);
  }
  .folder input {
    flex: 1;
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }
</style>
