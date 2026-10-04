<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { MergeView } from "@codemirror/merge";
  import { EditorState } from "@codemirror/state";
  import { EditorView } from "@codemirror/view";
  import { syntaxHighlighting, HighlightStyle } from "@codemirror/language";
  import { html } from "@codemirror/lang-html";
  import { tags as lezerTags } from "@lezer/highlight";
  import { api } from "../lib/api";
  import { confirmAction } from "../lib/confirm.svelte";
  import { timeAgo, fmtDateTime } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import { errorMessage } from "../lib/state.svelte";
  import type { HistoryEntry } from "../lib/types";
  import Icon from "../lib/Icon.svelte";

  let {
    pageId,
    currentHtml,
    onRestore,
    onClose,
  }: {
    pageId: string;
    currentHtml: string;
    /** Restore `at`; resolves true when the reader reloaded the restored page. */
    onRestore: (at: number) => Promise<boolean>;
    onClose: () => void;
  } = $props();

  let versions = $state<HistoryEntry[]>([]);
  let selected = $state<HistoryEntry | null>(null);
  let selectedHtml = $state("");
  let loading = $state(true);
  let listError = $state<string | null>(null);
  let versionError = $state<string | null>(null);
  let restoring = $state(false);

  let mergeHost: HTMLElement | undefined = $state();
  let mergeView: MergeView | undefined;
  let closeBtn: HTMLButtonElement | undefined;

  // The diff reuses the editor's CSS variables and HTML token colors, trimmed
  // to a read-only view (no gutters, folds or autocomplete).
  const highlight = HighlightStyle.define([
    { tag: lezerTags.tagName, color: "var(--danger, #9f2f2d)", fontWeight: "500" },
    { tag: lezerTags.angleBracket, color: "var(--muted, #6b6a68)" },
    { tag: lezerTags.attributeName, color: "var(--warn, #956400)" },
    { tag: [lezerTags.attributeValue, lezerTags.string], color: "var(--leaf, #346538)" },
    { tag: lezerTags.comment, color: "var(--muted, #6b6a68)", fontStyle: "italic" },
    { tag: lezerTags.keyword, color: "var(--danger, #9f2f2d)" },
    { tag: lezerTags.number, color: "var(--info, #1f6c9f)" },
    { tag: lezerTags.bool, color: "var(--danger, #9f2f2d)" },
  ]);

  const diffTheme = EditorView.theme({
    "&": {
      height: "100%",
      backgroundColor: "var(--surface)",
      color: "var(--text)",
      fontSize: "12.5px",
    },
    ".cm-scroller": {
      fontFamily: "var(--mono)",
      lineHeight: "1.6",
      overflow: "auto",
    },
    ".cm-content": { padding: "8px 0" },
    ".cm-gutters": {
      backgroundColor: "var(--raised)",
      color: "var(--muted)",
      border: "none",
    },
  });

  function side(doc: string) {
    return {
      doc,
      extensions: [
        html(),
        syntaxHighlighting(highlight),
        diffTheme,
        EditorState.readOnly.of(true),
        EditorView.editable.of(false),
        EditorView.lineWrapping,
      ],
    };
  }

  let selectSeq = 0;

  async function select(v: HistoryEntry | null) {
    const seq = ++selectSeq;
    selected = v;
    versionError = null;
    if (!v) {
      selectedHtml = "";
      return;
    }
    try {
      const result = await api.getHistory(pageId, v.at);
      if (seq !== selectSeq) return;
      selectedHtml = result.html;
    } catch (e) {
      if (seq !== selectSeq) return;
      selectedHtml = "";
      versionError = errorMessage(e);
    }
  }

  async function loadList() {
    loading = true;
    listError = null;
    try {
      const list = await api.listHistory(pageId);
      versions = list;
      const keep = selected && list.some((v) => v.at === selected!.at);
      if (!keep) await select(list[0] ?? null);
    } catch (e) {
      listError = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function restore() {
    const chosen = selected;
    if (!chosen || restoring) return;
    const ok = await confirmAction({
      title: t("history.restoreConfirmTitle"),
      message: t("history.restoreConfirmMessage"),
      confirmLabel: t("history.restore"),
    });
    if (!ok) return;
    restoring = true;
    try {
      if (await onRestore(chosen.at)) {
        versions = await api.listHistory(pageId);
        await select(chosen);
      }
    } catch (e) {
      versionError = errorMessage(e);
    } finally {
      restoring = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    closeBtn?.focus();
    void loadList();
  });

  onDestroy(() => mergeView?.destroy());

  // Rebuild the side-by-side diff whenever the selection or the current page changes.
  $effect(() => {
    const host = mergeHost;
    const older = selectedHtml;
    const current = currentHtml;
    if (!host || !older) return;
    mergeView = new MergeView({
      a: side(older),
      b: side(current),
      parent: host,
      highlightChanges: true,
      gutter: true,
      collapseUnchanged: { margin: 3, minSize: 6 },
    });
    return () => {
      mergeView?.destroy();
      mergeView = undefined;
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="overlay"
  role="presentation"
  onmousedown={(e) => e.target === e.currentTarget && onClose()}
>
  <div class="panel" role="dialog" aria-modal="true" aria-label={t("history.title")}>
    <header class="head">
      <span class="eyebrow"><Icon name="refresh-cw" size={13} />{t("history.title")}</span>
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
      <div class="state"><span class="spinner"></span>{t("history.loading")}</div>
    {:else if listError}
      <div class="state error">{t("history.loadFailed")}: {listError}</div>
    {:else if versions.length === 0}
      <div class="state empty">{t("history.empty")}</div>
    {:else}
      <div class="body">
        <ul class="versions" aria-label={t("history.title")}>
          {#each versions as v (v.at)}
            <li>
              <button
                class="version"
                class:active={selected?.at === v.at}
                aria-pressed={selected?.at === v.at}
                title={fmtDateTime(v.at)}
                onclick={() => select(v)}
              >
                <span class="when">{timeAgo(v.at)}</span>
                <span class="sub">
                  {v.caller === "agent" ? t("history.agent") : t("history.you")} · {fmtDateTime(v.at)}
                </span>
              </button>
            </li>
          {/each}
        </ul>

        <div class="diff">
          {#if versionError}
            <div class="state error">{t("history.versionFailed")}: {versionError}</div>
          {:else}
            <div class="diff-head">
              <span>{t("history.version")} · {selected ? fmtDateTime(selected.at) : ""}</span>
              <span>{t("history.current")}</span>
            </div>
            <div class="merge" bind:this={mergeHost} aria-label={t("history.diffLabel")}></div>
          {/if}
          <div class="actions">
            <button
              class="btn btn-sm btn-primary"
              onclick={restore}
              disabled={restoring || !selected}
            >
              {restoring ? t("history.restoring") : t("history.restore")}
            </button>
          </div>
        </div>
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
    animation: fade-in var(--t-fast) var(--ease-out);
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
    text-align: center;
  }
  .state.error {
    color: var(--danger);
  }
  .body {
    flex: 1;
    display: grid;
    grid-template-columns: 220px 1fr;
    min-height: 0;
  }
  .versions {
    list-style: none;
    margin: 0;
    padding: 6px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--raised);
  }
  .version {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-soft);
    text-align: left;
    cursor: pointer;
  }
  .version:hover {
    background: var(--sunken);
  }
  .version.active {
    background: var(--sunken);
    color: var(--accent-strong);
  }
  .version .when {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .version .sub {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .diff {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
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
  .merge {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .merge :global(.cm-mergeView) {
    height: 100%;
  }
  .merge :global(.cm-mergeViewEditors) {
    height: 100%;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    padding: 10px 12px;
    border-top: 1px solid var(--border);
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
</style>
