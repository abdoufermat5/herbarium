<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../lib/api";
  import {
    app,
    reloadPages,
    toast,
    errorMessage,
    deletePages,
    movePages,
    duplicatePage,
    nextReviewPage,
    goView,
  } from "../lib/state.svelte";
  import { registerLeaveGuard } from "../lib/navigation.svelte";
  import { confirmAction, confirmState } from "../lib/confirm.svelte";
  import { folderPickerState } from "../lib/folder-picker.svelte";
  import { fmtDate, fmtDateTime, timeAgo, fmtDuration, dueInfo, modKey } from "../lib/format";
  import { shortcutHint } from "../lib/shortcuts";
  import type { Page, PageMeta, ReviewGrade } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import PresetButtons from "./PresetButtons.svelte";
  import HtmlEditor from "./HtmlEditor.svelte";
  import { prefs } from "../lib/prefs.svelte";
  import { loadEditors, currentEditor } from "../lib/editors.svelte";

  let { id }: { id: string } = $props();

  const CLAUDE_MARKERS = [
    "window.storage",
    "window.claude",
    "claude.ai",
    "webui.chat",
    "window.parent.postMessage",
  ];

  // The draft preview renders the unsaved source in a sandboxed iframe. The
  // app's own CSP is inherited by `srcdoc`, so this policy can only tighten it;
  // it mirrors the served page's policy (assets over `herbarium:`, CDNs only
  // when the page allows them) so the preview never grants more than the page.
  const DRAFT_CSP_BLOCK =
    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; font-src herbarium: data:; img-src herbarium: data: blob:; media-src herbarium: data: blob:; connect-src 'none'; frame-src 'none'; object-src 'none'; form-action 'none'";
  const DRAFT_CSP_ALLOW =
    "default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval' herbarium: https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com https://code.jquery.com; style-src 'unsafe-inline' herbarium: https://fonts.googleapis.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; font-src herbarium: data: https://fonts.gstatic.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; img-src herbarium: https: data: blob:; media-src herbarium: https: data: blob:; connect-src herbarium: https://fonts.googleapis.com https://fonts.gstatic.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com https://code.jquery.com https://esm.sh; worker-src blob: herbarium:; frame-src 'none'; object-src 'none'; form-action 'none'";

  let page = $state<Page | null>(null);
  let error = $state<string | null>(null);
  let frameReady = $state(false);
  let saving = $state(false);
  let deleting = $state(false);
  let frameTimer: ReturnType<typeof setTimeout> | undefined;

  /** The page disappeared from the vault on disk (external delete). */
  let pageGone = $state(false);
  /** Updated-at taken from the last load/save; sent as `baseUpdatedAt` so a stale write fails. */
  let baseUpdatedAt = $state(0);
  /** Set when a save was rejected with `conflict:`; holds which draft was refused. */
  let conflict = $state<{ kind: "source" | "meta" } | null>(null);

  // Draft state for the details inspector
  let dTitle = $state("");
  let dFolder = $state("");
  let dTags = $state<string[]>([]);
  let dNote = $state("");
  let tagInput = $state("");

  const claudeDependent = $derived(
    !!page && CLAUDE_MARKERS.some((m) => page!.html.toLowerCase().includes(m)),
  );
  const due = $derived(page ? dueInfo(page.meta.nextReview) : null);
  const dirty = $derived(isDirty());

  function sameTags(a: string[], b: string[]): boolean {
    return a.length === b.length && a.every((v, i) => v === b[i]);
  }

  function isDirty(): boolean {
    if (!page) return false;
    return (
      dTitle !== page.meta.title ||
      dFolder !== (page.meta.folder ?? "") ||
      !sameTags(dTags, page.meta.tags) ||
      dNote !== page.meta.note
    );
  }

  function syncDraft(meta: PageMeta) {
    dTitle = meta.title;
    dFolder = meta.folder ?? "";
    dTags = [...meta.tags];
    dNote = meta.note;
    tagInput = "";
  }

  /* ------------------------------------------------------------------ tags */

  function normalizeTag(raw: string): string {
    return raw.trim().replace(/^#/, "").replace(/\s+/g, " ");
  }

  function normalizeTags(list: string[]): string[] {
    const out: string[] = [];
    for (const raw of list) {
      const tag = normalizeTag(raw);
      if (tag && !out.includes(tag)) out.push(tag);
    }
    return out;
  }

  function addTag(raw: string) {
    const tag = normalizeTag(raw);
    if (!tag) return;
    if (!dTags.includes(tag)) dTags = [...dTags, tag];
    tagInput = "";
  }

  function removeTag(tag: string) {
    dTags = dTags.filter((x) => x !== tag);
  }

  function onTagKey(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === "," || e.key === ";") {
      e.preventDefault();
      addTag(tagInput);
    } else if (e.key === "Backspace" && tagInput === "" && dTags.length > 0) {
      removeTag(dTags[dTags.length - 1]);
    }
  }

  function commitTagInput() {
    if (tagInput.trim()) addTag(tagInput);
  }

  /* ------------------------------------------------------------- load/save */

  async function load() {
    error = null;
    pageGone = false;
    conflict = null;
    frameReady = false;
    try {
      const loaded = await api.getPage(id);
      page = loaded;
      baseUpdatedAt = loaded.meta.updatedAt;
      source = loaded.html;
      syncDraft(loaded.meta);
      previewNonce++;
      clearTimeout(frameTimer);
      frameTimer = setTimeout(() => (frameReady = true), 1200);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  function back() {
    void goView(app.view);
  }

  function markReady() {
    clearTimeout(frameTimer);
    frameReady = true;
  }

  async function toggleNetwork() {
    const p = page;
    if (!p || pageGone) return;
    try {
      p.meta = await api.setNetwork(p.meta.id, !p.meta.allowCdn);
      baseUpdatedAt = p.meta.updatedAt;
      // The CSP is chosen when the page is served: reload so it applies now.
      previewNonce++;
      toast(
        p.meta.allowCdn ? t("read.netEnabled") : t("read.netDisabled"),
        "success",
      );
      void reloadPages();
    } catch (e) {
      toast(`${t("read.netFailed")} ${errorMessage(e)}`, "error");
    }
  }

  function toggleInspector() {
    app.inspectorOpen = !app.inspectorOpen;
  }

  /* ------------------------------------------------------------ page actions */

  async function reveal() {
    const p = page;
    if (!p) return;
    try {
      await api.revealPage(p.meta.id);
    } catch (e) {
      toast(t("read.revealFailed", { error: errorMessage(e) }), "error");
    }
  }

  async function duplicate() {
    const p = page;
    if (!p) return;
    await duplicatePage(p.meta.id);
  }

  async function move() {
    const p = page;
    if (!p) return;
    const result = await movePages([p.meta.id]);
    const fresh = result?.updated[0];
    if (!fresh) return;
    p.meta = { ...p.meta, ...fresh };
    baseUpdatedAt = p.meta.updatedAt;
    if (!dirty) syncDraft(p.meta);
    previewNonce++;
  }

  async function remove() {
    const p = page;
    if (!p || deleting) return;
    deleting = true;
    try {
      // Shared confirm + trash + Undo toast; it also asks this view's leave
      // guard before closing the reader so unsaved work is never dropped.
      await deletePages([p.meta.id]);
    } finally {
      deleting = false;
    }
  }

  /* --------------------------------------------------------- source editing */

  let editing = $state(false);
  let source = $state("");
  let savingSource = $state(false);
  let previewNonce = $state(0);
  let openingExternal = $state(false);
  /** Set after the file was handed to another editor: reload it when we regain focus. */
  let externalPending = false;
  const externalEditor = $derived(currentEditor());
  const sourceDirty = $derived(editing && !!page && source !== page.html);

  function isConflict(msg: string): boolean {
    return msg.startsWith("conflict:");
  }

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
    if (!p || !choice || sourceDirty || openingExternal || pageGone) return;
    openingExternal = true;
    try {
      const name = await api.openInEditor(p.meta.id, choice.id, choice.custom);
      externalPending = true;
      toast(t("edit.openedIn", { editor: name }), "success");
    } catch (e) {
      toast(`${t("edit.openFailed")}: ${errorMessage(e)}`, "error");
    } finally {
      openingExternal = false;
    }
  }

  async function saveSource() {
    const p = page;
    if (!p || savingSource || !sourceDirty) return;
    if (pageGone) {
      toast(t("read.deletedOnDisk"), "error");
      return;
    }
    savingSource = true;
    const draft = source;
    try {
      p.meta = await api.setPageHtml(p.meta.id, draft, baseUpdatedAt);
      baseUpdatedAt = p.meta.updatedAt;
      p.html = draft;
      conflict = null;
      previewNonce++;
      toast(t("edit.saved"), "success");
      void reloadPages(true);
    } catch (e) {
      const msg = errorMessage(e);
      if (isConflict(msg)) conflict = { kind: "source" };
      else toast(`${t("edit.saveFailed")}: ${msg}`, "error");
    } finally {
      savingSource = false;
    }
  }

  async function save() {
    const p = page;
    if (!p || saving) return;
    if (pageGone) {
      toast(t("read.deletedOnDisk"), "error");
      return;
    }
    const sTitle = dTitle;
    const sFolder = dFolder;
    const sTags = [...dTags];
    const sNote = dNote;
    const patch = {
      title: sTitle.trim() || p.meta.title || t("common.untitled"),
      folder: sFolder.trim() || null,
      tags: normalizeTags(sTags),
      note: sNote,
    };
    saving = true;
    try {
      p.meta = await api.updatePageMeta(p.meta.id, patch, baseUpdatedAt);
      baseUpdatedAt = p.meta.updatedAt;
      // Only overwrite a field the user has not kept typing in since.
      if (dTitle === sTitle) dTitle = p.meta.title;
      if (dFolder === sFolder) dFolder = p.meta.folder ?? "";
      if (sameTags(dTags, sTags)) dTags = [...p.meta.tags];
      if (dNote === sNote) dNote = p.meta.note;
      conflict = null;
      toast(t("read.saved"), "success");
      void reloadPages();
    } catch (e) {
      const msg = errorMessage(e);
      if (isConflict(msg)) conflict = { kind: "meta" };
      else toast(`${t("read.saveFailed")}: ${msg}`, "error");
    } finally {
      saving = false;
    }
  }

  function discard() {
    if (page) syncDraft(page.meta);
  }

  /* ------------------------------------------------------ conflict handling */

  function keepEditing() {
    conflict = null;
  }

  async function reloadPage() {
    conflict = null;
    await load();
  }

  /** Explicitly replace the newer on-disk version with the local draft. */
  async function overwriteConflict() {
    if (!conflict) return;
    const kind = conflict.kind;
    const ok = await confirmAction({
      title: t("read.conflict.overwriteTitle"),
      message: t("read.conflict.overwriteMessage"),
      confirmLabel: t("read.conflict.overwrite"),
      danger: true,
    });
    if (!ok) return;
    try {
      // Re-read the latest baseline so the write is not rejected as stale again.
      const fresh = await api.getPage(id);
      baseUpdatedAt = fresh.meta.updatedAt;
      conflict = null;
      if (kind === "source") await saveSource();
      else await save();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  /* ------------------------------------------------------ external revisions */

  async function refreshFromDisk(announce: boolean) {
    const p = page;
    if (!p) return;
    let fresh: Page;
    try {
      fresh = await api.getPage(p.meta.id);
    } catch (e) {
      // The page is genuinely gone only when the library no longer lists it.
      if (!app.library.some((x) => x.id === p.meta.id)) markDeleted();
      else if (announce) toast(errorMessage(e), "error");
      return;
    }
    pageGone = false;
    if (sourceDirty || dirty) {
      if (announce) toast(t("read.changedOnDisk"), "info");
      return;
    }
    page = fresh;
    baseUpdatedAt = fresh.meta.updatedAt;
    source = fresh.html;
    syncDraft(fresh.meta);
    conflict = null;
    previewNonce++;
    if (announce) toast(t("read.reloaded"), "info");
  }

  function markDeleted() {
    const clean = !sourceDirty && !dirty;
    pageGone = true;
    if (clean) {
      toast(t("read.deletedOnDiskClean"), "info");
      void goView(app.view);
    } else {
      toast(t("read.deletedOnDisk"), "error");
    }
  }

  function onFocus() {
    if (!externalPending) return;
    externalPending = false;
    void refreshFromDisk(true);
  }

  let lastRevision = app.vaultRevision;
  $effect(() => {
    const rev = app.vaultRevision;
    if (rev === lastRevision) return;
    lastRevision = rev;
    void refreshFromDisk(true);
  });

  /* ----------------------------------------------------------- draft preview */

  let draftDoc = $state("");
  let draftTimer: ReturnType<typeof setTimeout> | undefined;

  function buildDraftDoc(html: string, pageId: string, allowCdn: boolean): string {
    const head =
      `<base href="herbarium://page/${encodeURIComponent(pageId)}/">` +
      `<meta http-equiv="Content-Security-Policy" content="${allowCdn ? DRAFT_CSP_ALLOW : DRAFT_CSP_BLOCK}">`;
    const lower = html.toLowerCase();
    const headAt = lower.indexOf("<head");
    const insertAt = headAt >= 0 ? lower.indexOf(">", headAt) + 1 : -1;
    if (insertAt > 0) return html.slice(0, insertAt) + head + html.slice(insertAt);
    return head + html;
  }

  $effect(() => {
    const p = page;
    const src = source;
    const open = editing;
    const allow = p?.meta.allowCdn ?? false;
    clearTimeout(draftTimer);
    if (!open || !p) {
      draftDoc = "";
      return;
    }
    draftTimer = setTimeout(() => {
      draftDoc = buildDraftDoc(src, p.meta.id, allow);
    }, 300);
  });

  /* ------------------------------------------------------------ review flow */

  async function schedule(minutes: number) {
    const p = page;
    if (!p || saving || pageGone) return;
    saving = true;
    try {
      p.meta = await api.scheduleReview(p.meta.id, minutes);
      baseUpdatedAt = p.meta.updatedAt;
      toast(t("toast.reviewScheduled", { when: fmtDuration(minutes) }), "success");
      void reloadPages();
    } catch (e) {
      toast(`${t("read.scheduleFailed")} ${errorMessage(e)}`, "error");
    } finally {
      saving = false;
    }
  }

  async function clearReview() {
    const p = page;
    if (!p || saving || pageGone) return;
    saving = true;
    try {
      p.meta = await api.clearReview(p.meta.id);
      baseUpdatedAt = p.meta.updatedAt;
      toast(t("read.reviewCleared"), "success");
      void reloadPages();
    } catch (e) {
      toast(`${t("read.clearFailed")} ${errorMessage(e)}`, "error");
    } finally {
      saving = false;
    }
  }

  /** Record a grade; only a successful grade advances the session. */
  async function grade(g: ReviewGrade) {
    const p = page;
    if (!p || saving || pageGone) return;
    if (sourceDirty || dirty) {
      toast(t("edit.saveFirst"), "info");
      return;
    }
    saving = true;
    try {
      p.meta = await api.completeReview(p.meta.id, g);
      baseUpdatedAt = p.meta.updatedAt;
      toast(t("toast.reviewed", { when: fmtDuration(p.meta.intervalMinutes ?? 1) }), "success");
      void reloadPages();
      if (app.reviewSession) await nextReviewPage();
    } catch (e) {
      toast(`${t("read.scheduleFailed")} ${errorMessage(e)}`, "error");
    } finally {
      saving = false;
    }
  }

  async function skip() {
    const p = page;
    if (!p || saving || pageGone) return;
    // `nextReviewPage` leaves the page due but out of this session; the leave
    // guard still gets its say so a dirty draft is never dropped.
    await nextReviewPage(p.meta.id);
  }

  async function exitSession() {
    await goView(app.view);
  }

  /* --------------------------------------------------------------- shortcuts */

  function isTypingTarget(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null;
    if (!el) return false;
    const tag = el.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable) return true;
    return typeof el.closest === "function" && !!el.closest(".cm-editor");
  }

  // Capture phase so the review-session keys win over the window-level app
  // shortcuts (which would otherwise close the reader on Escape).
  function onSessionKey(e: KeyboardEvent) {
    if (!app.reviewSession || !page) return;
    if (confirmState.pending || folderPickerState.pending || app.paletteOpen || app.importOpen) return;
    if (e.ctrlKey || e.metaKey || e.altKey || e.repeat) return;
    if (isTypingTarget(e.target)) return;
    const key = e.key;
    if (key === "1") {
      e.preventDefault();
      e.stopPropagation();
      void grade("again");
    } else if (key === "2") {
      e.preventDefault();
      e.stopPropagation();
      void grade("good");
    } else if (key === "s" || key === "S") {
      e.preventDefault();
      e.stopPropagation();
      void skip();
    } else if (key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      void exitSession();
    }
  }

  let unregisterGuard: (() => void) | undefined;

  onMount(() => {
    void load();
    window.addEventListener("focus", onFocus);
    window.addEventListener("keydown", onSessionKey, true);
    // Nothing leaves this page (navigation, close, delete) without the user
    // choosing to discard unsaved source or inspector edits. Cancel is the safe default.
    unregisterGuard = registerLeaveGuard(async () => {
      if (!sourceDirty && !dirty) return true;
      const message =
        sourceDirty && dirty
          ? t("read.leave.messageBoth")
          : sourceDirty
            ? t("read.leave.messageSource")
            : t("read.leave.messageMeta");
      return confirmAction({
        title: t("read.leave.title"),
        message,
        confirmLabel: t("read.leave.discard"),
        danger: true,
      });
    });
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("keydown", onSessionKey, true);
    };
  });

  onDestroy(() => {
    clearTimeout(frameTimer);
    clearTimeout(draftTimer);
    unregisterGuard?.();
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
          disabled={pageGone}
          title={page.meta.allowCdn ? t("read.netOnHint") : t("read.netOffHint")}
        >
          <Icon name={page.meta.allowCdn ? "wifi" : "wifi-off"} size={14} />
          {page.meta.allowCdn ? t("read.netOn") : t("read.netOff")}
        </button>
        <button
          class="btn btn-sm details-btn"
          class:active={app.inspectorOpen}
          aria-pressed={app.inspectorOpen}
          onclick={toggleInspector}
          title={`${t("read.details")} · ${shortcutHint("inspector")}`}
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
    {#if pageGone}
      <div class="banner danger" role="status">
        <Icon name="info" size={14} />
        <span>{t("read.deletedOnDisk")}</span>
        <button class="btn btn-xs" onclick={back}>{t("common.back")}</button>
      </div>
    {/if}

    {#if conflict}
      <div class="banner warn" role="alert">
        <Icon name="info" size={14} />
        <div class="banner-text">
          <strong>{t("read.conflict.title")}</strong>
          <span>{t("read.conflict.message")}</span>
        </div>
        <div class="banner-actions">
          <button class="btn btn-xs" onclick={reloadPage}>{t("read.conflict.reload")}</button>
          <button class="btn btn-xs" onclick={keepEditing}>{t("read.conflict.keep")}</button>
          <button class="btn btn-xs btn-danger" onclick={overwriteConflict}>
            {t("read.conflict.overwrite")}
          </button>
        </div>
      </div>
    {/if}

    {#if app.reviewSession}
      <div class="review-bar session-bar">
        <span class="rb-label eyebrow">
          <Icon name="calendar-clock" size={12} />{t("read.session.active")}
        </span>
        <div class="rb-btns">
          <button
            class="btn btn-xs"
            onclick={() => grade("again")}
            disabled={saving || pageGone}
            title={t("read.session.againHint")}
          >
            1 · {t("review.again")}
          </button>
          <button
            class="btn btn-xs btn-primary"
            onclick={() => grade("good")}
            disabled={saving || pageGone}
            title={t("read.session.goodHint")}
          >
            2 · {t("review.good")}
          </button>
          <button
            class="btn btn-xs"
            onclick={skip}
            disabled={saving || pageGone}
            title={t("read.session.skipHint")}
          >
            S · {t("read.session.skip")}
          </button>
        </div>
        <span class="rb-sep" aria-hidden="true"></span>
        <button
          class="btn btn-xs btn-ghost"
          onclick={exitSession}
          title={t("read.session.exitHint")}
          >Esc · {t("read.session.exit")}</button
        >
        <span class="rb-keys">{t("read.session.keys")}</span>
        {#if saving}
          <span class="spinner" role="status" aria-label={t("read.saving")}></span>
        {/if}
      </div>
    {:else}
      <div class="review-bar">
        {#if page.meta.nextReview}
          <button class="btn btn-xs btn-primary done" onclick={() => grade("good")} disabled={saving}>
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
            <PresetButtons onpick={schedule} disabled={saving || pageGone} />
          </div>
          <button class="btn btn-xs btn-ghost" onclick={clearReview} disabled={saving || pageGone}>
            {t("common.clear")}
          </button>
        {:else}
          <span class="rb-label eyebrow"><Icon name="calendar-clock" size={12} />{t("read.reviewIn")}</span>
          <div class="rb-btns">
            <PresetButtons onpick={schedule} disabled={saving || pageGone} />
          </div>
        {/if}
        {#if page.meta.lastReview}
          <span class="rb-last">{t("read.lastReviewed", { when: timeAgo(page.meta.lastReview) })}</span>
        {/if}
        {#if saving}
          <span class="spinner" role="status" aria-label={t("read.saving")}></span>
        {/if}
      </div>
    {/if}

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
            <div class="editor">
              <div class="editor-bar">
                <span class="eyebrow">{t("edit.source")}</span>
                {#if sourceDirty}<span class="unsaved">· {t("edit.unsaved")}</span>{/if}
                <span class="grow"></span>
                {#if externalEditor}
                  <button
                    class="btn btn-xs"
                    onclick={openExternally}
                    disabled={sourceDirty || openingExternal || pageGone}
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
                  disabled={!sourceDirty || savingSource || pageGone}
                  title={t("edit.shortcut", { key: modKey("S") })}
                >
                  {savingSource ? t("insp.saving") : t("edit.save")}
                </button>
                <button class="btn btn-xs btn-ghost" onclick={stopEditing} disabled={sourceDirty}>
                  {t("edit.close")}
                </button>
              </div>
              <HtmlEditor
                bind:value={source}
                onsave={saveSource}
                wrap={prefs.editorWrap}
                fontSize={prefs.editorFontSize}
                lineHeight={prefs.editorLineHeight}
                tabSize={prefs.editorTabSize}
                tabIndents={prefs.editorTabIndents}
                spellcheck={prefs.editorSpellcheck}
                fontFamily={prefs.editorFont === "system"
                  ? "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
                  : "var(--mono)"}
                ariaLabel={t("edit.source")}
                readonly={pageGone}
              />
            </div>
            {#if prefs.editorPreview !== "off"}
              <div class="frame-holder">
                <iframe
                  class="ready"
                  title={t("edit.preview")}
                  srcdoc={draftDoc}
                  sandbox="allow-scripts allow-popups"
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
            sandbox="allow-scripts allow-popups"
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
              <div class="tag-editor">
                {#each dTags as tag (tag)}
                  <span class="tag-chip">
                    {tag}
                    <button
                      type="button"
                      class="tag-remove"
                      onclick={() => removeTag(tag)}
                      title={t("insp.removeTag", { tag })}
                      aria-label={t("insp.removeTag", { tag })}
                    >
                      <Icon name="x" size={10} />
                    </button>
                  </span>
                {/each}
                <input
                  id="d-tags"
                  class="tag-input"
                  type="text"
                  bind:value={tagInput}
                  onkeydown={onTagKey}
                  onblur={commitTagInput}
                  placeholder={t("insp.addTag")}
                  list="tag-options"
                />
                {#if app.tags.length > 0}
                  <datalist id="tag-options">
                    {#each app.tags as tc (tc.tag)}
                      <option value={tc.tag}></option>
                    {/each}
                  </datalist>
                {/if}
              </div>
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
              <button class="btn btn-sm btn-primary" onclick={save} disabled={!dirty || saving || pageGone}>
                {saving ? t("insp.saving") : t("insp.save")}
              </button>
            </div>
          </section>

          <section class="insp-section">
            <h2 class="eyebrow">{t("read.actions")}</h2>
            <div class="actions-row">
              <button class="btn btn-sm" onclick={move} disabled={saving || pageGone} title={t("read.moveHint")}>
                <Icon name="folder-open" size={13} />{t("read.move")}
              </button>
              <button class="btn btn-sm" onclick={duplicate} disabled={saving} title={t("read.duplicateHint")}>
                <Icon name="files" size={13} />{t("read.duplicate")}
              </button>
              <button class="btn btn-sm" onclick={reveal} disabled={pageGone} title={t("read.revealHint")}>
                <Icon name="external-link" size={13} />{t("read.reveal")}
              </button>
            </div>
          </section>

          <section class="insp-section">
            <h2 class="eyebrow">{t("insp.activity")}</h2>
            <dl class="facts">
              <div><dt>{t("insp.created")}</dt><dd>{fmtDate(page.meta.createdAt)}</dd></div>
              <div>
                <dt>{t("insp.updated")}</dt>
                <dd title={fmtDateTime(page.meta.updatedAt)}>{timeAgo(page.meta.updatedAt)}</dd>
              </div>
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
              onclick={remove}
              disabled={deleting}
            >
              <Icon name="trash-2" size={13} />
              {deleting ? t("insp.deleting") : t("insp.deletePage")}
            </button>
            <p class="hint">{t("insp.deleteHint")}</p>
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

  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
    font-size: var(--fs-sm);
    flex: none;
  }

  .banner.warn {
    background: var(--warn-soft);
    color: var(--warn);
    border-bottom-color: var(--warn-border);
  }

  .banner.danger {
    background: var(--danger-soft, var(--sunken));
    color: var(--danger);
  }

  .banner-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .banner-actions {
    display: flex;
    gap: 6px;
    flex: none;
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

  .rb-keys {
    margin-left: auto;
    font-size: var(--fs-xs);
    color: var(--muted);
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

  .tag-editor {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px 6px;
    min-height: 34px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm, 6px);
    background: var(--surface);
  }

  .tag-chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 2px 4px 2px 8px;
    border-radius: 999px;
    background: var(--sunken);
    border: 1px solid var(--border);
    font-size: var(--fs-xs);
    color: var(--text);
  }

  .tag-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
  }

  .tag-remove:hover {
    color: var(--danger);
    background: var(--danger-soft, var(--sunken));
  }

  .tag-input {
    flex: 1;
    min-width: 90px;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: var(--fs-sm);
    padding: 3px 4px;
  }

  .actions-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
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
