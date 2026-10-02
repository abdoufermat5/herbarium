<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { app, reloadPages, toast } from "../lib/state.svelte";
  import { fmtDate, timeAgo, plural, dueInfo } from "../lib/format";
  import type { Page, PageMeta } from "../lib/types";
  import Icon from "../lib/Icon.svelte";

  let { id }: { id: string } = $props();

  const REVIEW_INTERVALS = [1, 3, 7, 30];
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
      toast(
        p.meta.allowCdn
          ? "Network access enabled for this page."
          : "Network access disabled.",
        "success",
      );
      void reloadPages();
    } catch {
      toast("Couldn't change network access.", "error");
    }
  }

  function toggleInspector() {
    app.inspectorOpen = !app.inspectorOpen;
  }

  async function schedule(days: number) {
    const p = page;
    if (!p || saving) return;
    saving = true;
    try {
      p.meta = await api.scheduleReview(p.meta.id, days);
      toast(`Review scheduled in ${plural(days, "day")}.`, "success");
      void reloadPages();
    } catch {
      toast("Couldn't schedule the review.", "error");
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
      toast("Review cleared.", "success");
      void reloadPages();
    } catch {
      toast("Couldn't clear the review.", "error");
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
    try {
      p.meta = await api.updatePageMeta(p.meta.id, {
        title: dTitle.trim() || p.meta.title || "Untitled page",
        folder: dFolder.trim() || null,
        tags: dTags
          .split(",")
          .map((t) => t.trim().replace(/^#/, ""))
          .filter(Boolean),
        note: dNote,
      });
      syncDraft(p.meta);
      toast("Changes saved.", "success");
      void reloadPages();
    } catch {
      toast("Couldn't save changes.", "error");
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
      toast(`Deleted “${p.meta.title || "Untitled page"}”.`, "success");
      void reloadPages();
      back();
    } catch {
      toast("Couldn't delete the page.", "error");
      deleting = false;
      confirmDelete = false;
    }
  }

  onMount(load);
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
      title="Back to pages (Esc)"
      aria-label="Back to pages"
    >
      <Icon name="arrow-left" size={18} />
    </button>

    {#if page}
      <div class="identity">
        <h1 class="title ellipsis" title={page.meta.title || "Untitled page"}>{page.meta.title || "Untitled page"}</h1>
        <div class="meta">
          {#if page.meta.folder}
            <span class="loc"><Icon name="folder" size={12} />{page.meta.folder}</span>
          {/if}
          {#each page.meta.tags as t (t)}
            <span class="chip chip-muted">#{t}</span>
          {/each}
        </div>
      </div>

      <div class="actions">
        <button
          class="btn btn-sm net"
          class:on={page.meta.allowCdn}
          onclick={toggleNetwork}
          title={page.meta.allowCdn
            ? "Loading fonts and libraries from known CDNs is allowed for this page"
            : "This page cannot load anything from the network"}
        >
          <Icon name={page.meta.allowCdn ? "wifi" : "wifi-off"} size={14} />
          {page.meta.allowCdn ? "Network: CDNs" : "Network: off"}
        </button>
        <button
          class="btn btn-sm details-btn"
          class:active={app.inspectorOpen}
          aria-pressed={app.inspectorOpen}
          onclick={toggleInspector}
          title="Page details (i)"
        >
          <Icon name="info" size={14} />
          Details
        </button>
      </div>
    {/if}
  </header>

  {#if error}
    <div class="err-wrap">
      <div class="err card">
        <strong>Couldn't open this page.</strong>
        <p class="muted">{error}</p>
        <div class="err-actions">
          <button class="btn btn-sm" onclick={back}>Back</button>
          <button class="btn btn-sm btn-primary" onclick={load}>Try again</button>
        </div>
      </div>
    </div>
  {:else if page}
    <div class="review-bar">
      {#if page.meta.nextReview}
        <span class="rb-label"><Icon name="calendar-clock" size={13} />Next review</span>
        <span class="rb-date" class:overdue={due?.overdue}>
          {fmtDate(page.meta.nextReview)}
          {#if due?.overdue}· {due.label}{/if}
        </span>
        <span class="rb-sep" aria-hidden="true"></span>
        <span class="rb-label">Reschedule</span>
        <div class="rb-btns">
          {#each REVIEW_INTERVALS as days (days)}
            <button
              class="btn btn-xs"
              title={`Review again in ${plural(days, "day")}`}
              onclick={() => schedule(days)}
              disabled={saving}
            >
              {days}d
            </button>
          {/each}
        </div>
        <button class="btn btn-xs btn-ghost" onclick={clearReview} disabled={saving}>
          Clear
        </button>
      {:else}
        <span class="rb-label"><Icon name="calendar-clock" size={13} />Review in</span>
        <div class="rb-btns">
          {#each REVIEW_INTERVALS as days (days)}
            <button
              class="btn btn-xs"
              title={`Review again in ${plural(days, "day")}`}
              onclick={() => schedule(days)}
              disabled={saving}
            >
              {days}d
            </button>
          {/each}
        </div>
      {/if}
      {#if page.meta.lastReview}
        <span class="rb-last">Last reviewed {timeAgo(page.meta.lastReview)}</span>
      {/if}
      {#if saving}
        <span class="spinner" aria-label="Saving"></span>
      {/if}
    </div>

    <div class="content">
      <div class="frame-wrap">
        {#if claudeDependent}
          <div class="notice">
            <Icon name="info" size={14} />
            <span
              >This page appears to rely on Claude-specific APIs (storage or chat). It may not
              work outside of Claude.</span
            >
          </div>
        {/if}
        <div class="frame-holder">
          {#if !frameReady}
            <div class="frame-skel">
              <span class="spinner"></span>
              <span>Loading page…</span>
            </div>
          {/if}
          <iframe
            class:ready={frameReady}
            title="Page preview"
            src={`herbarium://page/${page.meta.id}`}
            sandbox="allow-scripts"
            onload={markReady}
          ></iframe>
        </div>
      </div>

      {#if app.inspectorOpen}
        <aside class="inspector" aria-label="Page details">
          <section class="insp-section">
            <h2 class="insp-title"><Icon name="file-text" size={13} />Details</h2>
            <div class="field">
              <label for="d-title">Title</label>
              <input id="d-title" bind:value={dTitle} />
            </div>
            <div class="field">
              <label for="d-folder">Folder</label>
              <input id="d-folder" bind:value={dFolder} placeholder="None" />
            </div>
            <div class="field">
              <label for="d-tags">Tags</label>
              <input id="d-tags" bind:value={dTags} placeholder="comma, separated" />
            </div>
            <div class="field">
              <label for="d-note">Note</label>
              <textarea
                id="d-note"
                rows={3}
                bind:value={dNote}
                placeholder="Personal notes about this page"
              ></textarea>
            </div>
            <div class="insp-actions">
              <button class="btn btn-sm" onclick={discard} disabled={!dirty || saving}>
                Discard
              </button>
              <button class="btn btn-sm btn-primary" onclick={save} disabled={!dirty || saving}>
                {saving ? "Saving…" : "Save"}
              </button>
            </div>
          </section>

          <section class="insp-section">
            <h2 class="insp-title"><Icon name="clock" size={13} />Activity</h2>
            <dl class="facts">
              <div><dt>Created</dt><dd>{fmtDate(page.meta.createdAt)}</dd></div>
              <div><dt>Updated</dt><dd>{timeAgo(page.meta.updatedAt)}</dd></div>
              <div>
                <dt>Last reviewed</dt>
                <dd>{page.meta.lastReview ? fmtDate(page.meta.lastReview) : "—"}</dd>
              </div>
              <div>
                <dt>Next review</dt>
                <dd>{page.meta.nextReview ? fmtDate(page.meta.nextReview) : "—"}</dd>
              </div>
            </dl>
          </section>

          <section class="insp-section danger-zone">
            <h2 class="insp-title"><Icon name="trash-2" size={13} />Danger zone</h2>
            <button
              class="btn btn-sm btn-danger delete-btn"
              class:confirming={confirmDelete}
              onclick={requestDelete}
              disabled={deleting}
            >
              <Icon name="trash-2" size={13} />
              {deleting ? "Deleting…" : confirmDelete ? "Confirm delete" : "Delete page"}
            </button>
            <p class="hint">
              {confirmDelete
                ? "This cannot be undone."
                : "Removes the HTML file and its metadata from your vault."}
            </p>
          </section>
        </aside>
      {/if}
    </div>
  {:else}
    <div class="load">
      <span class="spinner"></span>
      <span>Opening page…</span>
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
    gap: 10px;
    padding: 9px 14px;
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
    gap: 2px;
  }

  .title {
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.01em;
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
    font-size: 12px;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .net.on {
    color: var(--ok);
    border-color: var(--ok);
    background: var(--ok-soft);
  }

  .details-btn.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent-strong);
  }

  .review-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 7px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--raised);
    font-size: 12.5px;
    color: var(--muted);
    flex: none;
  }

  .rb-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .rb-date {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text);
    font-weight: 600;
  }

  .rb-date.overdue {
    color: var(--warn);
  }

  .rb-sep {
    width: 1px;
    height: 14px;
    background: var(--border-strong);
    margin: 0 2px;
  }

  .rb-btns {
    display: flex;
    gap: 4px;
  }

  .rb-last {
    margin-left: auto;
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
    padding: 8px 14px;
    background: var(--warn-soft);
    color: var(--warn);
    font-size: 12.5px;
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
    background: var(--surface);
    z-index: 1;
  }

  iframe {
    width: 100%;
    height: 100%;
    border: none;
    background: #fff;
    opacity: 0;
    transition: opacity var(--t-med) var(--ease-out);
    display: block;
  }

  iframe.ready {
    opacity: 1;
  }

  .inspector {
    width: var(--inspector-w);
    flex: none;
    border-left: 1px solid var(--border);
    background: var(--raised);
    overflow-y: auto;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 18px;
    animation: insp-in var(--t-med) var(--ease-out);
  }

  @keyframes insp-in {
    from {
      transform: translateX(10px);
      opacity: 0;
    }
    to {
      transform: none;
      opacity: 1;
    }
  }

  .insp-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .insp-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }

  .insp-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .facts {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
  }

  .facts > div {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    font-size: 12.5px;
  }

  .facts dt {
    color: var(--muted);
  }

  .facts dd {
    margin: 0;
    color: var(--text);
    text-align: right;
  }

  .danger-zone {
    border-top: 1px solid var(--border);
    padding-top: 14px;
  }

  .delete-btn.confirming {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }

  .hint {
    font-size: 12px;
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
    padding: 30px;
  }

  .err {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 20px 24px;
    max-width: 460px;
  }

  .err strong {
    font-size: 15px;
  }

  .err-actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
</style>
