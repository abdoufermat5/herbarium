<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { reloadPages, toast, errorMessage } from "../lib/state.svelte";
  import { confirmAction } from "../lib/confirm.svelte";
  import type { TrashEntry } from "../lib/types";
  import { t } from "../lib/i18n.svelte";
  import { timeAgo, plural } from "../lib/format";
  import { reveal } from "../lib/reveal";
  import Icon from "../lib/Icon.svelte";

  let entries = $state<TrashEntry[]>([]);
  let loading = $state(true);
  let loadError = $state("");
  let busyId = $state<string | null>(null);
  let emptying = $state(false);

  const titleOf = (e: TrashEntry) => e.title || t("common.untitled");
  const folderOf = (e: TrashEntry) => e.folder ?? t("confirm.vaultRoot");

  async function load() {
    loading = true;
    loadError = "";
    try {
      entries = await api.listTrash();
    } catch (e) {
      loadError = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => void load());

  /** Restore one page to its original folder (or the vault root) and refresh the library. */
  async function restore(entry: TrashEntry) {
    if (busyId) return;
    busyId = entry.id;
    try {
      await api.restorePage(entry.id);
      entries = entries.filter((e) => e.id !== entry.id);
      toast(t("trash.restored", { title: titleOf(entry) }), "success");
      await reloadPages(true);
    } catch (e) {
      console.error(e);
      toast(t("trash.restoreFailed", { title: titleOf(entry), detail: errorMessage(e) }), "error");
    } finally {
      busyId = null;
    }
  }

  /** Permanently delete one trashed page, after a destructive confirmation. */
  async function purge(entry: TrashEntry) {
    if (busyId) return;
    const confirmed = await confirmAction({
      title: t("trash.purgeConfirm", { count: 1 }),
      message: t("trash.purgeMessage"),
      confirmLabel: t("trash.purge"),
      danger: true,
      subject: { icon: "trash-2", label: titleOf(entry), meta: folderOf(entry) },
    });
    if (!confirmed) return;
    busyId = entry.id;
    try {
      await api.purgePages(entry.id);
      entries = entries.filter((e) => e.id !== entry.id);
      toast(t("trash.purged", { count: 1 }), "success");
    } catch (e) {
      console.error(e);
      toast(t("trash.purgeFailed", { detail: errorMessage(e) }), "error");
    } finally {
      busyId = null;
    }
  }

  /** Delete every trashed page at once, after a destructive confirmation. */
  async function emptyTrash() {
    if (emptying || entries.length === 0) return;
    const confirmed = await confirmAction({
      title: t("trash.emptyConfirmTitle"),
      message: t("trash.emptyMessage", { count: entries.length }),
      confirmLabel: t("trash.emptyAction"),
      danger: true,
    });
    if (!confirmed) return;
    emptying = true;
    try {
      const { removed } = await api.purgePages();
      entries = [];
      toast(t("trash.emptied", { count: removed }), "success");
    } catch (e) {
      console.error(e);
      toast(t("trash.purgeFailed", { detail: errorMessage(e) }), "error");
    } finally {
      emptying = false;
    }
  }
</script>

<section class="pane" aria-label={t("trash.title")}>
  <div class="inner">
    <header class="head">
      <div class="head-text">
        <span class="eyebrow">{t("sidebar.library")}</span>
        <h1 class="display">{t("trash.title")}</h1>
        <p class="sub">{t("trash.sub")}</p>
      </div>
      <div class="head-meta">
        {#if !loading && !loadError && entries.length > 0}
          <span class="count">{plural(entries.length, "page")}</span>
          <button class="btn btn-danger btn-sm" disabled={emptying || busyId !== null} onclick={emptyTrash}>
            <Icon name="trash-2" size={12} />
            {emptying ? t("trash.emptying") : t("trash.emptyAction")}
          </button>
        {/if}
      </div>
    </header>

    {#if loading}
      <div class="empty">
        <span class="spinner" aria-hidden="true"></span>
        <span>{t("trash.loading")}</span>
      </div>
    {:else if loadError}
      <div class="empty">
        <span class="empty-icon"><Icon name="trash-2" size={20} /></span>
        <strong>{t("trash.loadFailed")}</strong>
        <span class="error-text">{loadError}</span>
        <button class="btn btn-sm" onclick={() => void load()}>{t("trash.retry")}</button>
      </div>
    {:else if entries.length === 0}
      <div class="empty">
        <span class="empty-icon"><Icon name="trash-2" size={20} /></span>
        <strong>{t("trash.empty")}</strong>
        <span>{t("trash.emptyHint")}</span>
      </div>
    {:else}
      <ul class="rows">
        {#each entries as entry (entry.id)}
          <li class="entry" use:reveal>
            <div class="body">
              <span class="title">{titleOf(entry)}</span>
              <span class="meta">
                <span class="loc"><Icon name="folder" size={11} />{folderOf(entry)}</span>
                <span class="when">{t("trash.deleted", { when: timeAgo(entry.deletedAt) })}</span>
              </span>
            </div>
            <div class="actions">
              <button
                class="btn btn-sm"
                disabled={busyId !== null || emptying}
                onclick={() => restore(entry)}
              >
                <Icon name="refresh-cw" size={12} />
                {busyId === entry.id ? t("trash.restoring") : t("trash.restore")}
              </button>
              <button
                class="btn btn-danger btn-sm"
                disabled={busyId !== null || emptying}
                onclick={() => purge(entry)}
              >
                <Icon name="trash-2" size={12} />
                {t("trash.purge")}
              </button>
            </div>
          </li>
        {/each}
      </ul>
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
    flex-wrap: wrap;
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
  .sub {
    color: var(--muted);
    font-size: var(--fs-base);
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

  .rows {
    list-style: none;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    overflow: hidden;
  }
  .error-text {
    max-width: 460px;
    color: var(--danger);
    font-size: var(--fs-sm);
    overflow-wrap: anywhere;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 14px 18px;
  }
  .entry + .entry {
    border-top: 1px solid var(--border);
  }
  .body {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .title {
    font-size: var(--fs-base);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--muted);
    font-size: var(--fs-xs);
    flex-wrap: wrap;
  }
  .loc {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }
  .actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  @media (max-width: 620px) {
    .entry {
      flex-direction: column;
      align-items: stretch;
    }
  }
</style>
