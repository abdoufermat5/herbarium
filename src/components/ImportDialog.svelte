<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { app, pendingFiles, reloadPages, toast } from "../lib/state.svelte";
  import { plural } from "../lib/format";
  import type { ImportFile } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";

  type Tab = "files" | "paste";

  let tab = $state<Tab>("files");
  let pasted = $state("");
  let dragDepth = $state(0);
  let staged = $state<File[]>([]);
  let dragOver = $derived(dragDepth > 0);
  let busy = $state(false);
  let message = $state("");
  let messageErr = $state(false);

  let dialogEl = $state<HTMLDivElement | undefined>(undefined);
  let dropBtn = $state<HTMLButtonElement | undefined>(undefined);
  let pasteArea = $state<HTMLTextAreaElement | undefined>(undefined);
  let fileInput = $state<HTMLInputElement | undefined>(undefined);
  let prevFocus: HTMLElement | null = null;

  function close(force = false) {
    if (busy && !force) return;
    app.importOpen = false;
  }

  function setTab(next: Tab) {
    if (tab === next) return;
    tab = next;
    setTimeout(() => {
      if (next === "files") dropBtn?.focus();
      else pasteArea?.focus();
    });
  }

  function onTabKeys(e: KeyboardEvent) {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    setTab(tab === "files" ? "paste" : "files");
  }

  function focusables(): HTMLElement[] {
    if (!dialogEl) return [];
    return Array.from(
      dialogEl.querySelectorAll<HTMLElement>(
        'button:not(:disabled), [href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])',
      ),
    ).filter((el) => el.tabIndex >= 0 && !el.classList.contains("sr-only"));
  }

  function onKey(e: KeyboardEvent) {
    // The command palette, when open, is the topmost modal and owns Escape/Tab.
    if (app.paletteOpen || e.defaultPrevented) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close(true);
      return;
    }
    if (e.key !== "Tab" && e.code !== "Tab") return;
    const els = focusables();
    if (els.length === 0) return;
    const first = els[0];
    const last = els[els.length - 1];
    const active = document.activeElement as HTMLElement | null;
    const inside = !!active && !!dialogEl?.contains(active);
    if (e.shiftKey) {
      if (!inside || active === first) {
        e.preventDefault();
        last.focus();
      }
    } else if (!inside || active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  function onBackdrop(e: MouseEvent) {
    if (e.target === e.currentTarget) close(true);
  }

  const isHtml = (f: File) => /\.html?$/i.test(f.name);
  const fileKey = (f: File) => `${f.name}:${f.size}:${f.lastModified}`;

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  // DOMParser accepts any text, so require a real element in head/body.
  async function sniffHtml(f: File): Promise<boolean> {
    const head = await f.slice(0, 64 * 1024).text();
    if (head.includes("\0")) return false;
    const doc = new DOMParser().parseFromString(head, "text/html");
    return doc.querySelector("head *, body *") !== null;
  }

  async function stage(files: File[]) {
    const byExt = files.filter(isHtml);
    const verdicts = await Promise.all(byExt.map(sniffHtml));
    const html = byExt.filter((_, i) => verdicts[i]);
    const known = new Set(staged.map(fileKey));
    const fresh = html.filter((f) => {
      const k = fileKey(f);
      if (known.has(k)) return false;
      known.add(k);
      return true;
    });
    staged = [...staged, ...fresh];
    const skipped = files.length - html.length;
    messageErr = skipped > 0;
    message =
      skipped > 0
        ? html.length === 0 && staged.length === 0
          ? t("import.needHtml")
          : t("import.skipped", { skipped: plural(skipped, "file") })
        : "";
  }

  function unstage(file: File) {
    staged = staged.filter((f) => f !== file);
    message = "";
  }

  function onDragEnter(e: DragEvent) {
    e.preventDefault();
    dragDepth++;
  }

  function onDragOver(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  }

  async function doImport(files: File[]) {
    const htmlFiles = files.filter(isHtml);
    if (htmlFiles.length === 0) {
      messageErr = true;
      message = t("import.needHtml");
      return;
    }
    busy = true;
    message = "";
    messageErr = false;
    try {
      const payload: ImportFile[] = [];
      for (const f of htmlFiles) payload.push({ name: f.name, content: await f.text() });
      const res = await api.importFiles(payload);
      if (res.errors.length > 0) {
        messageErr = true;
        message = t("import.partial", {
          imported: res.imported,
          failed: plural(res.errors.length, "file"),
          errors: res.errors.join("\n"),
        });
        staged = [];
        if (res.imported > 0) void reloadPages();
      } else if (res.imported > 0) {
        toast(t("toast.imported", { count: res.imported }), "success");
        staged = [];
        void reloadPages();
        busy = false;
        close(true);
      } else {
        message = t("import.nothing");
      }
    } catch (e) {
      messageErr = true;
      message = t("import.failed", { error: String(e) });
    } finally {
      busy = false;
    }
  }

  async function pasteImport() {
    const content = pasted.trim();
    if (!content || busy) return;
    busy = true;
    message = "";
    messageErr = false;
    try {
      const res = await api.importFiles([{ name: null, content }]);
      if (res.errors.length > 0) {
        messageErr = true;
        message = t("import.failed", { error: res.errors.join("\n") });
      } else {
        toast(t("toast.imported", { count: res.imported }), "success");
        void reloadPages();
        busy = false;
        close(true);
      }
    } catch (e) {
      messageErr = true;
      message = t("import.failed", { error: String(e) });
    } finally {
      busy = false;
    }
  }

  function onPick(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    input.value = "";
    if (files.length > 0) void stage(files);
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragDepth = 0;
    const files = Array.from(e.dataTransfer?.files ?? []);
    if (files.length > 0) void stage(files);
  }

  $effect(() => {
    if (pendingFiles.files.length === 0 || busy) return;
    const files = pendingFiles.files;
    pendingFiles.files = [];
    void doImport(files);
  });

  onMount(() => {
    prevFocus = document.activeElement as HTMLElement | null;
    window.addEventListener("keydown", onKey);
    queueMicrotask(() => {
      if (!app.paletteOpen) dropBtn?.focus();
    });
  });

  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    // A disconnected opener is skipped; App hands focus to a stable control.
    if (prevFocus?.isConnected && !app.paletteOpen) prevFocus.focus();
  });
</script>

<div class="overlay" role="presentation" onmousedown={onBackdrop}>
  <div
    class="dialog card"
    role="dialog"
    aria-modal="true"
    aria-labelledby="import-title"
    bind:this={dialogEl}
    tabindex={-1}
  >
    <header class="dlg-head">
      <h2 id="import-title">{t("import.title")}</h2>
      <button
        class="btn btn-ghost btn-icon"
        aria-label={t("common.close")}
        disabled={busy}
        onclick={() => close(true)}
      >
        <Icon name="x" size={16} />
      </button>
    </header>

    <div class="tabs" role="tablist" aria-label={t("import.method")} tabindex={-1} onkeydown={onTabKeys}>
      <button
        class="tab"
        class:active={tab === "files"}
        role="tab"
        id="tab-files"
        aria-selected={tab === "files"}
        aria-controls="panel-files"
        tabindex={tab === "files" ? 0 : -1}
        onclick={() => setTab("files")}
      >
        {t("import.tabFiles")}
      </button>
      <button
        class="tab"
        class:active={tab === "paste"}
        role="tab"
        id="tab-paste"
        aria-selected={tab === "paste"}
        aria-controls="panel-paste"
        tabindex={tab === "paste" ? 0 : -1}
        onclick={() => setTab("paste")}
      >
        {t("import.tabPaste")}
      </button>
    </div>

    {#if tab === "files"}
      <div class="panel" role="tabpanel" id="panel-files" aria-labelledby="tab-files">
        <button
          type="button"
          class="dropzone"
          class:over={dragOver}
          bind:this={dropBtn}
          onclick={() => fileInput?.click()}
          ondragenter={onDragEnter}
          ondragover={onDragOver}
          ondragleave={() => (dragDepth = Math.max(0, dragDepth - 1))}
          ondrop={onDrop}
        >
          <Icon name="upload" size={26} />
          <strong>{dragOver ? t("import.release") : t("import.drop")}</strong>
          <span>{t("import.browse")}</span>
        </button>
        <input
          class="sr-only"
          type="file"
          tabindex="-1"
          aria-hidden="true"
          accept=".html,.htm,text/html"
          multiple
          bind:this={fileInput}
          onchange={onPick}
        />
        {#if staged.length > 0}
          <ul class="staged" aria-label={t("import.selected")}>
            {#each staged as file (fileKey(file))}
              <li>
                <Icon name="file-text" size={14} />
                <span class="fname" title={file.name}>{file.name}</span>
                <span class="fsize">{formatSize(file.size)}</span>
                <button
                  class="btn btn-ghost btn-icon"
                  aria-label={t("import.remove", { name: file.name })}
                  disabled={busy}
                  onclick={() => unstage(file)}
                >
                  <Icon name="x" size={14} />
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {:else}
      <div class="panel" role="tabpanel" id="panel-paste" aria-labelledby="tab-paste">
        <textarea
          bind:this={pasteArea}
          bind:value={pasted}
          rows={6}
          aria-label={t("import.tabPaste")}
          placeholder={t("import.pastePlaceholder")}
          spellcheck={false}
        ></textarea>
      </div>
    {/if}

    <footer class="foot">
      <p class="message" class:err={messageErr} aria-live="polite">{message}</p>
      {#if busy}
        <span class="spinner" aria-label={t("import.importingLabel")}></span>
      {/if}
      {#if tab === "files" && staged.length > 0}
        <button class="btn btn-primary" disabled={busy} onclick={() => doImport(staged)}>
          {busy ? t("import.importing") : t("import.importN", { count: staged.length })}
        </button>
      {/if}
      {#if tab === "paste"}
        <button
          class="btn btn-primary"
          disabled={busy || !pasted.trim()}
          onclick={pasteImport}
        >
          {busy ? t("import.importing") : t("import.button")}
        </button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 12vh 24px 24px;
    background: var(--scrim);
    animation: fade-in var(--t-fast) var(--ease-out);
  }

  .dialog {
    width: 560px;
    max-width: 100%;
    max-height: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px 20px 16px;
    overflow-y: auto;
    animation: pop-in var(--t-med) var(--ease-spring);
  }

  .dlg-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--border);
  }

  .tab {
    padding: 6px 10px;
    margin-bottom: -1px;
    font-size: 13px;
    font-weight: 500;
    color: var(--muted);
    border-bottom: 2px solid transparent;
    transition: color var(--t-fast) var(--ease-out);
  }

  .tab:hover {
    color: var(--text);
  }

  .tab.active {
    color: var(--accent-strong);
    border-bottom-color: var(--accent);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 30px 16px;
    border: 1.5px dashed var(--border-strong);
    border-radius: var(--radius);
    background: var(--raised);
    color: var(--muted);
    text-align: center;
    transition:
      border-color var(--t-fast) var(--ease-out),
      background var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }

  .dropzone:hover {
    border-color: var(--accent);
    color: var(--text-soft);
  }

  .dropzone.over {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-strong);
  }

  .dropzone strong {
    color: var(--text);
    font-size: 14px;
    margin-top: 2px;
  }

  .dropzone span {
    font-size: 12.5px;
  }

  .staged {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 168px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .staged li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 4px 10px;
    font-size: 12.5px;
    color: var(--muted);
  }

  .staged li + li {
    border-top: 1px solid var(--border);
  }

  .fname {
    flex: 1;
    min-width: 0;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fsize {
    font-variant-numeric: tabular-nums;
  }

  textarea {
    width: 100%;
    min-height: 130px;
    font-family: var(--mono);
    font-size: 12.5px;
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }

  .message {
    flex: 1;
    min-width: 0;
    font-size: 12.5px;
    color: var(--muted);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 90px;
    overflow-y: auto;
  }

  .message.err {
    color: var(--danger);
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }

  @keyframes pop-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }
</style>
