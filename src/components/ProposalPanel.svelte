<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { fmtDateTime, timeAgo } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import { app, errorMessage } from "../lib/state.svelte";
  import type { ProposalSummary } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { confirmState } from "../lib/confirm.svelte";
  import { folderPickerState } from "../lib/folder-picker.svelte";
  import HtmlDiff from "./HtmlDiff.svelte";

  let {
    pageId,
    currentHtml,
    onAccept,
    onReject,
    onClose,
  }: {
    pageId: string;
    currentHtml: string;
    /** Apply the proposal; resolves true when it was applied. */
    onAccept: () => Promise<boolean>;
    /** Discard the proposal; resolves true when it was discarded. */
    onReject: () => Promise<boolean>;
    onClose: () => void;
  } = $props();

  let proposal = $state<ProposalSummary | null>(null);
  let proposedHtml = $state("");
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let working = $state(false);
  let closeBtn: HTMLButtonElement | undefined;

  async function load() {
    loading = true;
    loadError = null;
    try {
      const result = await api.getProposal(pageId);
      proposal = result.proposal;
      proposedHtml = result.html;
    } catch (e) {
      loadError = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function run(action: () => Promise<boolean>) {
    if (working) return;
    working = true;
    try {
      if (await action()) onClose();
    } finally {
      working = false;
    }
  }

  // Capture phase: the window-level shortcuts run first otherwise, and their
  // Escape would also close the reader or leave Settings.
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !app.paletteOpen && !confirmState.pending && !folderPickerState.pending) {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  }

  onMount(() => {
    closeBtn?.focus();
    void load();
  });
</script>

<svelte:window onkeydowncapture={onKeydown} />

<div
  class="overlay"
  role="presentation"
  onmousedown={(e) => e.target === e.currentTarget && onClose()}
>
  <div class="panel" role="dialog" aria-modal="true" aria-label={proposal?.source === "remix" ? t("proposal.titleRemix") : t("proposal.title")}>
    <header class="head">
      <span class="eyebrow">{#if proposal?.source === "remix"}<Icon name="sparkle" size={13} />{t("proposal.titleRemix")}{:else}<Icon name="file-text" size={13} />{t("proposal.title")}{/if}</span>
      <button
        bind:this={closeBtn}
        class="btn btn-ghost btn-icon"
        onclick={onClose}
        title={t("common.close")}
        aria-label={t("common.close")}
      >
        <Icon name="x" size={15} />
      </button>
    </header>

    {#if loading}
      <div class="state"><span class="spinner"></span>{t("proposal.loading")}</div>
    {:else if loadError || !proposal}
      <div class="state error">{t("proposal.loadFailed")}: {loadError}</div>
    {:else}
      <p class="summary">
        <span title={fmtDateTime(proposal.at)}>{t(proposal.source === "remix" ? "proposal.summaryRemix" : "proposal.summary", { when: timeAgo(proposal.at) })}</span>
        {#if proposal.stale}
          <span class="stale"><Icon name="info" size={12} />{t("proposal.stale")}</span>
        {/if}
      </p>
      <div class="diff-head">
        <span>{t("history.current")}</span>
        <span>{t("proposal.proposed")}</span>
      </div>
      <HtmlDiff before={currentHtml} after={proposedHtml} label={t("proposal.diffLabel")} />
      <div class="actions">
        <span class="hint">{t("proposal.historyHint")}</span>
        <button class="btn btn-sm" onclick={() => run(onReject)} disabled={working}>
          {t("proposal.reject")}
        </button>
        <button class="btn btn-sm btn-primary" onclick={() => run(onAccept)} disabled={working}>
          {working ? t("proposal.applying") : t("proposal.accept")}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-palette);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4vh 24px;
    background: var(--scrim);
  }
  .panel {
    width: 1040px;
    max-width: 100%;
    height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
  }
  .head .eyebrow {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: 1;
    min-width: 0;
  }
  .state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .state.error {
    color: var(--danger);
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    margin: 0;
    padding: 10px 14px;
    font-size: var(--fs-sm);
    color: var(--text-soft);
    border-bottom: 1px solid var(--border);
  }
  .stale {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--warn);
  }
  .diff-head {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    font-size: var(--fs-2xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 10px 12px;
    border-top: 1px solid var(--border);
  }
  .hint {
    flex: 1;
    font-size: var(--fs-xs);
    color: var(--muted);
  }
</style>
