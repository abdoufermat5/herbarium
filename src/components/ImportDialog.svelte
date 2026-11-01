<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { api } from "../lib/api";
  import { app, pendingFiles, reloadPages } from "../lib/state.svelte";
  import { plural } from "../lib/format";
  import type { ImportFile } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import Illustration from "../lib/Illustration.svelte";
  import { t } from "../lib/i18n.svelte";
  import { prefs, lastImportFolder, rememberImportFolder } from "../lib/prefs.svelte";
  import Select, { type SelectOption } from "./Select.svelte";
  import Switch from "./settings/Switch.svelte";

  type Tab = "files" | "paste";

  let tab = $state<Tab>("files");
  let pasted = $state("");
  /** A large paste, kept out of the textarea: laying out megabytes of text
   *  there would freeze the window. Shown as a summary instead. */
  let bigPaste = $state<string | null>(null);
  const BIG_PASTE = 100_000;
  const pasteContent = $derived(bigPaste ?? pasted);
  const bigPasteTitle = $derived(
    bigPaste ? (/<title[^>]*>([^<]{0,200})/i.exec(bigPaste.slice(0, 20_000))?.[1]?.trim() ?? "") : "",
  );

  function onPaste(e: ClipboardEvent) {
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (text.length < BIG_PASTE) return;
    e.preventDefault();
    bigPaste = text;
    pasted = "";
  }
  let dragDepth = $state(0);
  let staged = $state<File[]>([]);
  let dragOver = $derived(dragDepth > 0);
  let busy = $state(false);
  let message = $state("");
  let messageErr = $state(false);
  /** Files that went in during this visit, shown ticked above the ones still waiting. */
  let imported = $state<File[]>([]);
  /** Why each staged file failed, by file key (empty when the importer gave no reason). */
  let failed = $state<Record<string, string>>({});
  /** Set once everything is in: the dialog then shows the result instead of the form. */
  let outcome = $state<{ count: number; folder: string } | null>(null);
  let doneBtn = $state<HTMLButtonElement | undefined>(undefined);
  let importedTotal = 0;
  // Network access for the pages about to be imported; starts from the vault
  // default and is overridable per import. `cdnTouched` keeps a late-arriving
  // default from clobbering an explicit choice (drop metadata or the switch).
  let allowCdn = $state(false);
  let cdnTouched = false;

  // Destination folder; "" is the vault root. Where it starts is a setting.
  let destination = $state(
    prefs.importTarget === "root" ? "" : prefs.importTarget === "last" ? lastImportFolder() : (app.folderFilter ?? ""),
  );
  // A folder that vanished since it was chosen (e.g. deleted on disk) falls back to the root.
  const target = $derived(app.folders.includes(destination) ? destination : "");
  const destOptions = $derived<SelectOption<string>[]>([
    { value: "", label: t("import.rootFolder"), icon: "folder" },
    ...app.folders.map((f) => ({ value: f, label: f, icon: "folder" as const })),
  ]);

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
      close();
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
    if (e.target === e.currentTarget) close();
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

  // Keep the staged files the backend named in its errors so they can be
  // retried. `pages.import` labels each failure "<name>: <reason>". When no
  // label matches, a total failure keeps everything staged and a partial one
  // clears it (the importer reported no per-file detail to attribute).
  function failedStaged(files: File[], errors: string[], imported: number): File[] {
    const failed = new Set<string>();
    for (const err of errors) {
      const label = err.split(": ")[0];
      if (label && files.some((f) => f.name === label)) failed.add(label);
    }
    if (failed.size === 0) return imported === 0 ? files : [];
    return files.filter((f) => failed.has(f.name));
  }

  type FileState = "ready" | "busy" | "done" | "failed";
  function stateOf(f: File): FileState {
    if (imported.includes(f)) return "done";
    if (failed[fileKey(f)] !== undefined) return "failed";
    return busy ? "busy" : "ready";
  }

  /** "<name>: <reason>" lines from the importer, matched back to the files they name. */
  function reasonsFor(files: File[], errors: string[]): Record<string, string> {
    const out: Record<string, string> = {};
    for (const f of files) {
      const own = errors.find((e) => e.startsWith(`${f.name}: `));
      out[fileKey(f)] = own ? own.slice(f.name.length + 2) : "";
    }
    return out;
  }

  async function finish(count: number) {
    busy = false;
    bigPaste = null;
    outcome = { count, folder: target || t("import.rootFolder") };
    await tick();
    doneBtn?.focus();
  }

  function importMore() {
    outcome = null;
    imported = [];
    failed = {};
    importedTotal = 0;
    message = "";
    tab = "files";
    setTimeout(() => dropBtn?.focus());
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
    failed = {};
    try {
      const payload: ImportFile[] = [];
      for (const f of htmlFiles) payload.push({ name: f.name, content: await f.text() });
      const res = await api.importFiles(payload, target || null, allowCdn);
      if (res.imported > 0) rememberImportFolder(target);
      importedTotal += res.imported;
      if (res.errors.length > 0) {
        messageErr = true;
        message = t("import.partial", {
          imported: res.imported,
          failed: plural(res.errors.length, "file"),
          errors: res.errors.join("\n"),
        });
        // Keep the files the backend named so they can be retried; the rest stay ticked.
        const left = failedStaged(htmlFiles, res.errors, res.imported);
        imported = [...imported, ...htmlFiles.filter((f) => !left.includes(f))];
        staged = left;
        failed = reasonsFor(left, res.errors);
        if (res.imported > 0) void reloadPages();
      } else if (res.imported > 0) {
        imported = [...imported, ...htmlFiles];
        staged = [];
        void reloadPages();
        await finish(importedTotal);
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
    const content = pasteContent.trim();
    if (!content || busy) return;
    busy = true;
    message = "";
    messageErr = false;
    try {
      const res = await api.importFiles([{ name: null, content }], target || null, allowCdn);
      if (res.imported > 0) rememberImportFolder(target);
      if (res.errors.length > 0) {
        messageErr = true;
        message = t("import.failed", { error: res.errors.join("\n") });
      } else {
        importedTotal += res.imported;
        void reloadPages();
        await finish(importedTotal);
      }
    } catch (e) {
      messageErr = true;
      message = t("import.failed", { error: String(e) });
    } finally {
      busy = false;
    }
  }

  const rows = $derived([...imported, ...staged]);

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

  // Files dropped on the window (or a folder) wait here for review; they are
  // never imported automatically, and the destination/network choice they
  // carry is preserved.
  $effect(() => {
    if (pendingFiles.files.length === 0) return;
    const files = pendingFiles.files;
    const folder = pendingFiles.folder;
    const cdn = pendingFiles.allowCdn;
    pendingFiles.files = [];
    pendingFiles.folder = undefined;
    pendingFiles.allowCdn = undefined;
    tab = "files";
    if (folder !== undefined) destination = folder ?? "";
    if (cdn !== undefined) {
      allowCdn = cdn;
      cdnTouched = true;
    }
    void stage(files);
  });

  onMount(() => {
    prevFocus = document.activeElement as HTMLElement | null;
    window.addEventListener("keydown", onKey);
    void api
      .networkSettings()
      .then((s) => {
        if (!cdnTouched) allowCdn = s.defaultAllowCdn;
      })
      .catch((e) => console.error(e));
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
      <h2 id="import-title" class="display">{t("import.title")}</h2>
      <button
        class="btn btn-ghost btn-icon"
        aria-label={t("common.close")}
        disabled={busy}
        onclick={() => close(true)}
      >
        <Icon name="x" size={16} />
      </button>
    </header>

    {#if outcome}
      <div class="success" role="status">
        <Illustration name="seedling" size={104} animate />
        <h3>{t("import.doneTitle", { count: outcome.count })}</h3>
        <p>{t("import.doneIn", { folder: outcome.folder })}</p>
        <div class="success-actions">
          <button type="button" class="btn btn-ghost" onclick={importMore}>{t("import.more")}</button>
          <button type="button" class="btn btn-primary" bind:this={doneBtn} onclick={() => close(true)}>
            {t("import.done")}
          </button>
        </div>
      </div>
    {:else}
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

      {#if app.folders.length > 0}
        <div class="dest">
          <span class="dest-label">{t("import.destination")}</span>
          <Select
            value={target}
            options={destOptions}
            ariaLabel={t("import.destination")}
            disabled={busy}
            onchange={(v) => (destination = v)}
          />
        </div>
      {/if}

      <div class="cdn">
        <div class="cdn-text">
          <span class="cdn-label">{t("import.allowCdn")}</span>
          <span class="cdn-hint">{t("import.allowCdnHint")}</span>
        </div>
        <Switch
          checked={allowCdn}
          label={t("import.allowCdn")}
          disabled={busy}
          onchange={(v) => {
            allowCdn = v;
            cdnTouched = true;
          }}
        />
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
            <span class="drop-icon"><Icon name="upload" size={18} /></span>
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
          {#if rows.length > 0}
            <ul class="staged" aria-label={t("import.selected")}>
              {#each rows as file, n (fileKey(file))}
                {@const state = stateOf(file)}
                <li style:--n={Math.min(n, 6)}>
                  <span class="tick {state}" role="img" aria-label={t(`import.status.${state}`)}>
                    {#if state === "done"}
                      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 8.5 7 11.5 12 5" pathLength="1" /></svg>
                    {:else if state === "failed"}
                      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 5 11 11M11 5 5 11" pathLength="1" /></svg>
                    {/if}
                  </span>
                  <span class="fname" title={file.name}>{file.name}</span>
                  {#if failed[fileKey(file)]}
                    <span class="freason" title={failed[fileKey(file)]}>{failed[fileKey(file)]}</span>
                  {/if}
                  <span class="fsize">{formatSize(file.size)}</span>
                  {#if state === "done"}
                    <span class="fgap"></span>
                  {:else}
                    <button
                      class="btn btn-ghost btn-icon"
                      aria-label={t("import.remove", { name: file.name })}
                      disabled={busy}
                      onclick={() => unstage(file)}
                    >
                      <Icon name="x" size={14} />
                    </button>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {:else}
        <div class="panel" role="tabpanel" id="panel-paste" aria-labelledby="tab-paste">
          {#if bigPaste !== null}
            <div class="big-paste">
              <Icon name="file-text" size={16} />
              <span class="bp-text">
                <strong>{bigPasteTitle || t("common.untitled")}</strong>
                <small>{t("import.bigPaste", { size: (bigPaste.length / 1024 / 1024).toFixed(1) })}</small>
              </span>
              <button class="btn btn-sm btn-ghost" onclick={() => ((bigPaste = null), queueMicrotask(() => pasteArea?.focus()))}>
                {t("common.clear")}
              </button>
            </div>
          {:else}
            <textarea
              bind:this={pasteArea}
              bind:value={pasted}
              onpaste={onPaste}
              rows={6}
              aria-label={t("import.tabPaste")}
              placeholder={t("import.pastePlaceholder")}
              spellcheck={false}
            ></textarea>
          {/if}
        </div>
      {/if}

      <button
        class="btn btn-ghost btn-sm ai-link"
        onclick={() => {
          app.importOpen = false;
          app.aiImportOpen = true;
        }}
      >
        <Icon name="files" size={13} />{t("aiImport.fromDialog")}
      </button>

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
            disabled={busy || !pasteContent.trim()}
            onclick={pasteImport}
          >
            {busy ? t("import.importing") : t("import.button")}
          </button>
        {/if}
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
    gap: 16px;
    padding: 24px 28px 22px;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow-y: auto;
    overscroll-behavior: contain;
    animation: pop-in var(--t-med) var(--ease-out);
  }

  .dlg-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    font-size: var(--fs-2xl);
  }

  .tabs {
    display: flex;
    gap: 16px;
    border-bottom: 1px solid var(--border);
  }

  .tab {
    padding: 6px 0 8px;
    margin-bottom: -1px;
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--muted);
    border-bottom: 1.5px solid transparent;
    border-radius: 0;
    transition:
      color var(--t-fast) var(--ease-out),
      border-color var(--t-fast) var(--ease-out);
  }

  .tab:hover {
    color: var(--text);
  }

  .tab.active {
    color: var(--accent-strong);
    border-bottom-color: var(--accent-strong);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 36px 16px;
    border: 1px dashed var(--border-hover);
    border-radius: var(--radius);
    background: var(--raised);
    color: var(--muted);
    text-align: center;
    transition:
      border-color var(--t-fast) var(--ease-out),
      background var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out),
      transform var(--t-enter) var(--ease-spring),
      box-shadow var(--t-med) var(--ease-out);
  }

  .dropzone:hover {
    border-color: var(--text-soft);
    color: var(--text-soft);
  }

  /* Pressed paper: the sheet sinks a little into the page and the icon settles tilted. */
  .dropzone.over {
    border-style: solid;
    border-color: var(--leaf);
    background: var(--leaf-soft);
    color: var(--leaf);
    box-shadow: inset 0 0 0 1px var(--leaf);
    transform: scale(0.985);
  }

  .drop-icon {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    margin-bottom: 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    transition: transform var(--t-enter) var(--ease-spring);
  }
  .dropzone.over .drop-icon {
    transform: translateY(3px) rotate(-6deg);
  }

  .dropzone strong {
    color: var(--text);
    font-size: var(--fs-base);
    font-weight: 500;
  }

  .dropzone span {
    font-size: var(--fs-sm);
  }

  .staged {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 168px;
    overflow-y: auto;
    overscroll-behavior: contain;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .staged li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 4px 10px;
    font-size: var(--fs-sm);
    color: var(--muted);
    animation: row-in var(--t-fast) var(--ease-out) backwards;
    animation-delay: calc(var(--n, 0) * 30ms);
  }

  .staged li + li {
    border-top: 1px solid var(--border);
  }

  @keyframes row-in {
    from {
      opacity: 0;
      transform: translateY(3px);
    }
  }

  /* Status tick: empty ring while waiting, spinning arc while importing, a drawn check when in. */
  .tick {
    flex: none;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border: 1.5px solid var(--border-input);
    border-radius: 50%;
    transition:
      background var(--t-fast) var(--ease-out),
      border-color var(--t-fast) var(--ease-out);
  }
  .tick.busy {
    border-color: var(--border);
    border-top-color: var(--leaf);
    animation: spin 700ms linear infinite;
  }
  .tick.done {
    background: var(--leaf);
    border-color: var(--leaf);
    animation: tick-pop 360ms var(--ease-spring) backwards;
    animation-delay: calc(var(--n, 0) * 60ms);
  }
  .tick.failed {
    background: var(--danger-soft);
    border-color: var(--danger);
  }
  .tick svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .tick.done svg {
    stroke: var(--on-accent);
  }
  .tick.failed svg {
    stroke: var(--danger);
  }
  .tick path {
    stroke-dasharray: 1;
    animation: tick-draw 280ms var(--ease-out) backwards;
    animation-delay: calc(var(--n, 0) * 60ms + 120ms);
  }
  @keyframes tick-pop {
    from {
      transform: scale(0.4);
      opacity: 0;
    }
  }
  @keyframes tick-draw {
    from {
      stroke-dashoffset: 1;
    }
  }
  .freason {
    flex: 0 1 40%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--danger);
    font-size: var(--fs-xs);
  }
  .fgap {
    flex: none;
    width: 28px;
  }

  /* Everything is in: the seedling draws itself and the dialog says how many. */
  .success {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 8px 0 2px;
    text-align: center;
  }
  .success h3 {
    margin: 6px 0 0;
    font-family: var(--font-display);
    font-size: var(--fs-2xl);
    font-weight: 500;
    letter-spacing: -0.02em;
    line-height: 1.1;
    color: var(--text);
  }
  .success p {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .success-actions {
    display: flex;
    gap: 8px;
    margin-top: 14px;
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

  .big-paste {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--sunken);
    color: var(--muted);
  }
  .bp-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .bp-text strong {
    color: var(--text);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bp-text small {
    font-size: var(--fs-xs);
  }
  textarea {
    width: 100%;
    min-height: 130px;
    font-family: var(--mono);
    font-size: var(--fs-sm);
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
    font-size: var(--fs-sm);
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

  .dest {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .dest-label {
    font-size: var(--fs-xs);
    font-weight: 500;
    color: var(--muted);
  }

  .cdn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .cdn-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .cdn-label {
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--text);
  }

  .cdn-hint {
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .ai-link {
    align-self: flex-start;
  }
</style>
