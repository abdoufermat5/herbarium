<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { app, reloadPages, toast } from "../lib/state.svelte";
  import { fmtDate, fmtDateTime, timeAgo, fmtDuration, dueInfo } from "../lib/format";
  import type { Page, PageMeta } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import PresetButtons from "./PresetButtons.svelte";
  import { prefs } from "../lib/prefs.svelte";
  import { indent, applyToTextarea } from "../lib/editor";
  import { loadEditors, currentEditor } from "../lib/editors.svelte";

  let { id }: { id: string } = $props();

  const CLAUDE_MARKERS = [
    "window.storage",
    "window.claude",
    "claude.ai",
    "webui.chat",
    "window.parent.postMessage",
  ];

  let page = $state<Page | null>(null);
  let error = $state<string | null>(null);
  let frameReady = $state(false);
  let saving = $state(false);
  let deleting = $state(false);
  let confirmDelete = $state(false);
  let frameTimer: ReturnType<typeof setTimeout> | undefined;
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  // Draft state for the details inspector
  let dTitle = $state("");
  let dFolder = $state("");
  let dTags = $state("");
  let dNote = $state("");

  const claudeDependent = $derived(
    !!page && CLAUDE_MARKERS.some((m) => page!.html.toLowerCase().includes(m)),
  );
  const due = $derived(page ? dueInfo(page.meta.nextReview) : null);
  const dirty = $derived(isDirty());

  function isDirty(): boolean {
    if (!page) return false;
    return (
      dTitle !== page.meta.title ||
      dFolder !== (page.meta.folder ?? "") ||
      dTags !== page.meta.tags.join(", ") ||
      dNote !== page.meta.note
    );
  }

  function syncDraft(meta: PageMeta) {
    dTitle = meta.title;
    dFolder = meta.folder ?? "";
    dTags = meta.tags.join(", ");
    dNote = meta.note;
  }

  async function load() {
    error = null;
    page = null;
    frameReady = false;
    try {
      const loaded = await api.getPage(id);
      page = loaded;
      syncDraft(loaded.meta);
      clearTimeout(frameTimer);
      frameTimer = setTimeout(() => (frameReady = true), 1200);
    } catch (e) {
      error = String(e);
    }
  }

  function back() {
    app.readId = null;
  }

  function markReady() {
    clearTimeout(frameTimer);
    frameReady = true;
  }

  async function toggleNetwork() {
    const p = page;
    if (!p) return;
    try {
      p.meta = await api.setNetwork(p.meta.id, !p.meta.allowCdn);
      // The CSP is chosen when the page is served: reload so it applies now.
      previewNonce++;
      toast(
        p.meta.allowCdn ? t("read.netEnabled") : t("read.netDisabled"),
        "success",
      );
      void reloadPages();
    } catch {
      toast(t("read.netFailed"), "error");
    }
  }

  function toggleInspector() {
    app.inspectorOpen = !app.inspectorOpen;
  }

  let editing = $state(false);
  let source = $state("");
  let savingSource = $state(false);
  let previewNonce = $state(0);
  let openingExternal = $state(false);
  /** Set after the file was handed to another editor: reload it when we regain focus. */
  let externalPending = false;
  const externalEditor = $derived(currentEditor());
  const sourceDirty = $derived(editing && !!page && source !== page.html);

  function startEditing() {
    if (!page) return;
    source = page.html;
    editing = true;
    void loadEditors();
  }

  function stopEditing() {
    if (!sourceDirty) editing = false;
  }

  function discardSource() {
    if (page) source = page.html;
  }

  async function openExternally() {
    const p = page;
    const choice = externalEditor;
    if (!p || !choice || sourceDirty || openingExternal) return;
    openingExternal = true;
    try {
      const name = await api.openInEditor(p.meta.id, choice.id, choice.custom);
      externalPending = true;
      toast(t("edit.openedIn", { editor: name }), "success");
    } catch (e) {
      toast(`${t("edit.openFailed")}: ${e}`, "error");
    } finally {
      openingExternal = false;
    }
  }

  /** Pick up edits made in another editor while this window was in the background. */
  async function reloadFromDisk() {
    if (!externalPending) return;
    externalPending = false;
    const p = page;
    if (!p || sourceDirty) return;
    try {
      const fresh = await api.getPage(p.meta.id);
      if (fresh.html === p.html) return;
      p.html = fresh.html;
      if (editing) source = fresh.html;
      previewNonce++;
      toast(t("edit.reloaded"), "info");
    } catch (e) {
      console.error(e);
    }
  }

  async function saveSource() {
    const p = page;
    if (!p || savingSource || !sourceDirty) return;
    savingSource = true;
    const saved = source;
    try {
      p.meta = await api.setPageHtml(p.meta.id, saved);
      p.html = saved;
      previewNonce++;
      toast(t("edit.saved"), "success");
      void reloadPages(true);
    } catch (e) {
      toast(`${t("edit.saveFailed")}: ${e}`, "error");
    } finally {
      savingSource = false;
    }
  }

  function onSourceKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void saveSource();
    } else if (e.key === "Escape") {
      // Leave the field instead of closing the page and losing the draft.
      e.stopPropagation();
      (e.target as HTMLElement).blur();
    } else if (e.key === "Tab" && prefs.editorTabIndents && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      const ta = e.currentTarget as HTMLTextAreaElement;
      applyToTextarea(ta, indent(ta.value, ta.selectionStart, ta.selectionEnd, prefs.editorTabSize, e.shiftKey));
    }
  }

  async function schedule(minutes: number) {
    const p = page;
    if (!p || saving) return;
    saving = true;
    try {
      p.meta = await api.scheduleReview(p.meta.id, minutes);
      toast(t("toast.reviewScheduled", { when: fmtDuration(minutes) }), "success");
      void reloadPages();
    } catch {
      toast(t("read.scheduleFailed"), "error");
    } finally {
      saving = false;
    }
  }

  async function completeReview() {
    const p = page;
    if (!p || saving) return;
    saving = true;
    try {
      p.meta = await api.completeReview(p.meta.id);
      toast(t("toast.reviewed", { when: fmtDuration(p.meta.intervalMinutes ?? 1) }), "success");
      void reloadPages();
    } catch {
      toast(t("read.scheduleFailed"), "error");
    } finally {
      saving = false;
    }
  }

  async function clearReview() {
    const p = page;
    if (!p || saving) return;
    saving = true;
    try {
      p.meta = await api.clearReview(p.meta.id);
      toast(t("read.reviewCleared"), "success");
      void reloadPages();
    } catch {
      toast(t("read.clearFailed"), "error");
    } finally {
      saving = false;
    }
  }

  function discard() {
    if (page) syncDraft(page.meta);
  }

  async function save() {
    const p = page;
    if (!p || saving) return;
    saving = true;
    const sTitle = dTitle;
    const sFolder = dFolder;
    const sTags = dTags;
    const sNote = dNote;
    try {
      p.meta = await api.updatePageMeta(p.meta.id, {
        title: sTitle.trim() || p.meta.title || t("common.untitled"),
        folder: sFolder.trim() || null,
        tags: sTags
          .split(",")
          .map((tag) => tag.trim().replace(/^#/, ""))
          .filter(Boolean),
        note: sNote,
      });
      if (dTitle === sTitle) dTitle = p.meta.title;
      if (dFolder === sFolder) dFolder = p.meta.folder ?? "";
      if (dTags === sTags) dTags = p.meta.tags.join(", ");
      if (dNote === sNote) dNote = p.meta.note;
      toast(t("read.saved"), "success");
      void reloadPages();
    } catch {
      toast(t("read.saveFailed"), "error");
    } finally {
      saving = false;
    }
  }

  function requestDelete() {
    if (!confirmDelete) {
      confirmDelete = true;
      clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => (confirmDelete = false), 3000);
      return;
    }
    void remove();
  }

  async function remove() {
    const p = page;
    if (!p || deleting) return;
    deleting = true;
    try {
      await api.deletePage(p.meta.id);
      toast(t("toast.deleted", { title: p.meta.title || t("common.untitled") }), "success");
      void reloadPages();
      back();
    } catch {
      toast(t("toast.deleteFailed"), "error");
      deleting = false;
      confirmDelete = false;
    }
  }

  onMount(() => {
    void load();
    window.addEventListener("focus", reloadFromDisk);
    return () => window.removeEventListener("focus", reloadFromDisk);
  });
  onDestroy(() => {
    clearTimeout(frameTimer);
    clearTimeout(confirmTimer);
  });
</script>

<div class="read">
  <header class="bar">
    <button
      class="btn btn-ghost btn-icon"
      onclick={back}
      title={t("read.back")}
      aria-label={t("read.backLabel")}
    >
      <Icon name="arrow-left" size={16} />
    </button>

    {#if page}
      <div class="identity">
        <h1 class="title ellipsis" title={page.meta.title || t("common.untitled")}>
          {page.meta.title || t("common.untitled")}
        </h1>
        <div class="meta">
          {#if page.meta.folder}
            <span class="loc"><Icon name="folder" size={12} />{page.meta.folder}</span>
          {/if}
          {#each page.meta.tags as tag (tag)}
            <span class="chip chip-muted">{tag}</span>
          {/each}
        </div>
      </div>

      <div class="actions">
        <button
          class="btn btn-sm details-btn"
          class:active={editing}
          aria-pressed={editing}
          onclick={() => (editing ? stopEditing() : startEditing())}
          disabled={editing && sourceDirty}
          title={t("read.editHint")}
        >
          <Icon name="file-text" size={14} />
          {t("read.edit")}
        </button>
        <button
          class="btn btn-sm net"
          class:on={page.meta.allowCdn}
          aria-pressed={page.meta.allowCdn}
          onclick={toggleNetwork}
          title={page.meta.allowCdn
            ? t("read.netOnHint")
            : t("read.netOffHint")}
        >
          <Icon name={page.meta.allowCdn ? "wifi" : "wifi-off"} size={14} />
          {page.meta.allowCdn ? t("read.netOn") : t("read.netOff")}
        </button>
        <button
          class="btn btn-sm details-btn"
          class:active={app.inspectorOpen}
          aria-pressed={app.inspectorOpen}
          onclick={toggleInspector}
          title={t("read.detailsHint")}
        >
          <Icon name="info" size={14} />
          {t("read.details")}
        </button>
      </div>
    {/if}
  </header>

  {#if error}
    <div class="err-wrap">
      <div class="err card">
        <strong>{t("read.openFailed")}</strong>
        <p class="muted">{error}</p>
        <div class="err-actions">
          <button class="btn btn-sm" onclick={back}>{t("common.back")}</button>
          <button class="btn btn-sm btn-primary" onclick={load}>{t("boot.retry")}</button>
        </div>
      </div>
    </div>
  {:else if page}
    <div class="review-bar">
      {#if page.meta.nextReview}
        <button class="btn btn-xs btn-primary done" onclick={completeReview} disabled={saving}>
          <Icon name="check" size={12} />{t("review.done")}
        </button>
        <span class="rb-label eyebrow"><Icon name="calendar-clock" size={12} />{t("read.nextReview")}</span>
        <span class="rb-date" class:overdue={due?.overdue}>
          {fmtDateTime(page.meta.nextReview)}
          {#if due?.overdue}· {due.label}{/if}
        </span>
        <span class="rb-sep" aria-hidden="true"></span>
        <span class="rb-label eyebrow">{t("read.reschedule")}</span>
        <div class="rb-btns">
          <PresetButtons onpick={schedule} disabled={saving} />
        </div>
        <button class="btn btn-xs btn-ghost" onclick={clearReview} disabled={saving}>
          {t("common.clear")}
        </button>
      {:else}
        <span class="rb-label eyebrow"><Icon name="calendar-clock" size={12} />{t("read.reviewIn")}</span>
        <div class="rb-btns">
          <PresetButtons onpick={schedule} disabled={saving} />
        </div>
      {/if}
      {#if page.meta.lastReview}
        <span class="rb-last">{t("read.lastReviewed", { when: timeAgo(page.meta.lastReview) })}</span>
      {/if}
      {#if saving}
        <span class="spinner" role="status" aria-label={t("read.saving")}></span>
      {/if}
    </div>

    <div class="content">
      <div class="frame-wrap">
        {#if claudeDependent}
          <div class="notice">
            <Icon name="info" size={14} />
            <span>{t("read.claudeNotice")}</span>
          </div>
        {/if}
        {#if editing}
          <div class="split" class:below={prefs.editorPreview === "below"} class:solo={prefs.editorPreview === "off"}>
            <div
              class="editor"
              style:--ed-font={prefs.editorFont === "system"
                ? "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
                : "var(--mono)"}
              style:--ed-size="{prefs.editorFontSize}px"
              style:--ed-lh={prefs.editorLineHeight}
              style:--ed-tab={prefs.editorTabSize}
              style:--ed-wrap={prefs.editorWrap ? "pre-wrap" : "pre"}
            >
              <div class="editor-bar">
                <span class="eyebrow">{t("edit.source")}</span>
                {#if sourceDirty}<span class="unsaved">· {t("edit.unsaved")}</span>{/if}
                <span class="grow"></span>
                {#if externalEditor}
                  <button
                    class="btn btn-xs"
                    onclick={openExternally}
                    disabled={sourceDirty || openingExternal}
                    title={sourceDirty ? t("edit.saveFirst") : t("edit.openInHint", { editor: externalEditor.name })}
                  >
                    <Icon name="external-link" size={12} />{externalEditor.name}
                  </button>
                {/if}
                <button class="btn btn-xs" onclick={discardSource} disabled={!sourceDirty || savingSource}>
                  {t("edit.discard")}
                </button>
                <button
                  class="btn btn-xs btn-primary"
                  onclick={saveSource}
                  disabled={!sourceDirty || savingSource}
                  title={t("edit.shortcut")}
                >
                  {t("edit.save")}
                </button>
                <button class="btn btn-xs btn-ghost" onclick={stopEditing} disabled={sourceDirty}>
                  {t("edit.close")}
                </button>
              </div>
              <textarea
                class="source"
                aria-label={t("edit.source")}
                spellcheck={prefs.editorSpellcheck}
                bind:value={source}
                onkeydown={onSourceKey}
              ></textarea>
            </div>
            {#if prefs.editorPreview !== "off"}
              <div class="frame-holder">
                <iframe
                  class="ready"
                  title={t("edit.preview")}
                  src={`herbarium://page/${encodeURIComponent(page.meta.id)}?v=${previewNonce}`}
                  sandbox="allow-scripts"
                ></iframe>
              </div>
            {/if}
          </div>
        {:else}
        <div class="frame-holder">
          {#if !frameReady}
            <div class="frame-skel">
              <span class="spinner"></span>
              <span>{t("read.loadingPage")}</span>
            </div>
          {/if}
          <iframe
            class:ready={frameReady}
            title={t("read.preview")}
            src={`herbarium://page/${encodeURIComponent(page.meta.id)}?v=${previewNonce}`}
            sandbox="allow-scripts"
            onload={markReady}
          ></iframe>
        </div>
        {/if}
      </div>

      {#if app.inspectorOpen}
        <aside class="inspector" aria-label={t("insp.label")}>
          <section class="insp-section">
            <h2 class="eyebrow">{t("read.details")}</h2>
            <div class="field">
              <label for="d-title">{t("insp.title")}</label>
              <input id="d-title" type="text" bind:value={dTitle} />
            </div>
            <div class="field">
              <label for="d-folder">{t("insp.folder")}</label>
              <div class="folder-input-wrap">
                <input
                  id="d-folder"
                  type="text"
                  bind:value={dFolder}
                  placeholder={t("common.none")}
                  list="folder-options"
                />
                {#if app.folders.length > 0}
                  <datalist id="folder-options">
                    {#each app.folders as folder (folder)}
                      <option value={folder}></option>
                    {/each}
                  </datalist>
                {/if}
              </div>
            </div>
            <div class="field">
              <label for="d-tags">{t("insp.tags")}</label>
              <input id="d-tags" type="text" bind:value={dTags} placeholder={t("insp.tagsPlaceholder")} />
            </div>
            <div class="field">
              <label for="d-note">{t("insp.note")}</label>
              <textarea
                id="d-note"
                rows={3}
                bind:value={dNote}
                placeholder={t("insp.notePlaceholder")}
              ></textarea>
            </div>
            <div class="insp-actions">
              <button class="btn btn-sm" onclick={discard} disabled={!dirty || saving}>
                {t("insp.discard")}
              </button>
              <button class="btn btn-sm btn-primary" onclick={save} disabled={!dirty || saving}>
                {saving ? t("insp.saving") : t("insp.save")}
              </button>
            </div>
          </section>

          <section class="insp-section">
            <h2 class="eyebrow">{t("insp.activity")}</h2>
            <dl class="facts">
              <div><dt>{t("insp.created")}</dt><dd>{fmtDate(page.meta.createdAt)}</dd></div>
              <div><dt>{t("insp.updated")}</dt><dd>{timeAgo(page.meta.updatedAt)}</dd></div>
              <div>
                <dt>{t("insp.lastReviewed")}</dt>
                <dd>{page.meta.lastReview ? fmtDateTime(page.meta.lastReview) : "—"}</dd>
              </div>
              <div>
                <dt>{t("insp.nextReview")}</dt>
                <dd>{page.meta.nextReview ? fmtDateTime(page.meta.nextReview) : "—"}</dd>
              </div>
            </dl>
          </section>

          <section class="insp-section">
            <h2 class="eyebrow">{t("insp.danger")}</h2>
            <button
              class="btn btn-sm btn-danger delete-btn"
              class:confirming={confirmDelete}
              onclick={requestDelete}
              disabled={deleting}
            >
              <Icon name="trash-2" size={13} />
              {deleting
                ? t("insp.deleting")
                : confirmDelete
                  ? t("insp.confirmDelete")
                  : t("insp.deletePage")}
            </button>
            <p class="hint">
              {confirmDelete
                ? t("insp.irreversible")
                : t("insp.deleteHint")}
            </p>
          </section>
        </aside>
      {/if}
    </div>
  {:else}
    <div class="load">
      <span class="spinner"></span>
      <span>{t("read.opening")}</span>
    </div>
  {/if}
</div>

<style>
  .read {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--surface);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    z-index: 5;
    flex: none;
  }

  .identity {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .title {
    font-family: var(--font-display);
    font-size: var(--fs-lg);
    font-weight: 500;
    line-height: 1.2;
    letter-spacing: -0.02em;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .loc {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-size: var(--fs-xs);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .net.on {
    color: var(--ok);
    border-color: transparent;
    background: var(--ok-soft);
  }

  .details-btn.active {
    background: var(--sunken);
    border-color: var(--border-hover);
    color: var(--accent-strong);
  }

  .review-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--raised);
    font-size: var(--fs-sm);
    color: var(--muted);
    flex: none;
  }

  .rb-label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .rb-date {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text);
    font-family: var(--mono);
    font-size: var(--fs-xs);
  }

  .rb-date.overdue {
    color: var(--danger);
  }

  .rb-sep {
    width: 1px;
    height: 14px;
    background: var(--border-strong);
    margin: 0 4px;
  }

  .rb-btns {
    display: flex;
    gap: 4px;
  }

  .rb-last {
    margin-left: auto;
    font-size: var(--fs-xs);
  }

  .content {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .frame-wrap {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .notice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: var(--warn-soft);
    color: var(--warn);
    font-size: var(--fs-sm);
    border-bottom: 1px solid var(--warn-border);
    flex: none;
  }

  .frame-holder {
    position: relative;
    flex: 1;
    min-height: 0;
  }

  .frame-skel {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--muted);
    font-size: var(--fs-sm);
    background: var(--surface);
    z-index: 1;
  }

  /* Saved pages render on white regardless of theme — they bring their own styles. */
  iframe {
    width: 100%;
    height: 100%;
    border: none;
    background: var(--page-canvas);
    opacity: 0;
    transition: opacity var(--t-slow) var(--ease-out);
    display: block;
  }

  iframe.ready {
    opacity: 1;
  }

  .split {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }
  .split.below {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
  }
  .split.solo {
    grid-template-columns: minmax(0, 1fr);
  }
  .editor {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    border-right: 1px solid var(--border);
  }
  .below .editor {
    border-right: none;
    border-bottom: 1px solid var(--border);
  }
  .solo .editor {
    border-right: none;
  }
  .editor-bar {
    flex: none;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--raised);
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .editor-bar .grow {
    flex: 1;
  }
  .editor-bar .unsaved {
    color: var(--warn, var(--accent-strong));
  }
  .source {
    flex: 1;
    min-height: 0;
    width: 100%;
    resize: none;
    border: none;
    outline: none;
    padding: 14px 16px;
    background: var(--surface);
    color: var(--text);
    font-family: var(--ed-font, var(--mono));
    font-size: var(--ed-size, 12.5px);
    line-height: var(--ed-lh, 1.6);
    tab-size: var(--ed-tab, 2);
    white-space: var(--ed-wrap, pre);
    overflow-wrap: anywhere;
    overflow: auto;
  }

  .inspector {
    width: var(--inspector-w);
    flex: none;
    border-left: 1px solid var(--border);
    background: var(--raised);
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 24px 20px;
    display: flex;
    flex-direction: column;
    gap: 28px;
    animation: insp-in var(--t-slow) var(--ease-out);
  }

  @keyframes insp-in {
    from {
      transform: translateX(12px);
      opacity: 0;
    }
  }

  .insp-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .insp-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .facts {
    display: flex;
    flex-direction: column;
    margin: 0;
  }

  .facts > div {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 0;
    font-size: var(--fs-sm);
    border-bottom: 1px solid var(--border);
  }

  .facts > div:first-child {
    border-top: 1px solid var(--border);
  }

  .facts dt {
    color: var(--muted);
  }

  .facts dd {
    margin: 0;
    color: var(--text);
    font-family: var(--mono);
    font-size: var(--fs-xs);
    text-align: right;
  }

  .delete-btn {
    align-self: flex-start;
  }

  .delete-btn.confirming {
    background: var(--danger-fill);
    border-color: var(--danger-fill);
    color: var(--on-danger);
  }

  .hint {
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .load,
  .err-wrap {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--muted);
    padding: 32px;
  }

  .err {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 28px 32px;
    max-width: 460px;
  }

  .err strong {
    font-family: var(--font-display);
    font-size: var(--fs-xl);
    font-weight: 500;
    letter-spacing: -0.02em;
  }

  .err-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
</style>
