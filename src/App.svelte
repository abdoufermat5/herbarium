<script lang="ts">
  import { tick } from "svelte";
  import { app, pendingFiles, initApp, clearFilters } from "./lib/state.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import PageList from "./components/PageList.svelte";
  import ReviewView from "./components/ReviewView.svelte";
  import SettingsView from "./components/SettingsView.svelte";
  import ReadView from "./components/ReadView.svelte";
  import ImportDialog from "./components/ImportDialog.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import TitleBar from "./components/TitleBar.svelte";
  import Toasts from "./components/Toasts.svelte";
  import Icon from "./lib/Icon.svelte";
  import { t } from "./lib/i18n.svelte";

  let dragging = $state(false);
  let dragTimer: ReturnType<typeof setTimeout> | undefined;
  let retrying = $state(false);

  function hasFiles(e: DragEvent): boolean {
    return !!e.dataTransfer && Array.from(e.dataTransfer.types).includes("Files");
  }

  function onDragOver(e: DragEvent) {
    if (!hasFiles(e)) return;
    e.preventDefault();
    if (app.config?.vaultPath) dragging = true;
    clearTimeout(dragTimer);
    dragTimer = setTimeout(() => (dragging = false), 180);
  }

  function onDrop(e: DragEvent) {
    if (!hasFiles(e)) return;
    e.preventDefault();
    clearTimeout(dragTimer);
    dragging = false;
    const files = Array.from(e.dataTransfer?.files ?? []).filter((f) =>
      /\.html?$/i.test(f.name),
    );
    if (files.length === 0) return;
    pendingFiles.files = files;
    app.importOpen = true;
  }

  function isTyping(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null;
    if (!el) return false;
    const tag = el.tagName;
    return (
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      el.isContentEditable
    );
  }

  function onKeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;

    // Command palette — works even while typing.
    if (mod && e.key.toLowerCase() === "k") {
      e.preventDefault();
      app.paletteOpen = !app.paletteOpen;
      return;
    }
    if (mod && e.key === "," && app.config?.vaultPath && !app.importOpen && !app.paletteOpen) {
      e.preventDefault();
      app.readId = null;
      app.view = "settings";
      return;
    }


    if (e.key === "Escape") {
      if (app.paletteOpen) {
        // Topmost modal consumes Escape so the dialog beneath keeps its draft.
        e.preventDefault();
        app.paletteOpen = false;
        return;
      }
      if (app.importOpen) return; // handled by the dialog
      if (app.readId) {
        app.readId = null;
        return;
      }
      if (app.view === "settings") {
        if (!e.defaultPrevented) app.view = "list";
        return;
      }
      if (app.folderFilter || app.tagFilter) {
        clearFilters();
        return;
      }
      return;
    }

    if (isTyping(e.target) || e.defaultPrevented || e.repeat) return;

    if (e.key === "/") {
      const input = document.getElementById("page-search") as HTMLInputElement | null;
      if (input) {
        e.preventDefault();
        input.focus();
        input.select();
      }
      return;
    }

    if (e.key.toLowerCase() === "i") {
      if (app.readId) {
        app.inspectorOpen = !app.inspectorOpen;
      } else if (!app.importOpen && !app.paletteOpen) {
        app.importOpen = true;
      }
    }
  }

  // When a view change or a closing modal leaves focus on a removed element,
  // hand it to a stable control instead of dropping it on <body>.
  let focusSeen = false;
  $effect(() => {
    app.readId;
    app.view;
    app.importOpen;
    app.paletteOpen;
    if (!focusSeen) {
      focusSeen = true;
      return;
    }
    void tick().then(() => {
      if (app.importOpen || app.paletteOpen) return;
      const a = document.activeElement;
      if (a && a !== document.body && a.isConnected && !a.closest("[inert]")) return;
      document
        .querySelector<HTMLElement>(app.readId ? ".content .bar button" : "#page-search, .shell button")
        ?.focus();
    });
  });

  async function retry() {
    retrying = true;
    await initApp();
    retrying = false;
  }
</script>

<svelte:window
  ondragover={onDragOver}
  ondrop={onDrop}
  onkeydown={onKeydown}
  ondragend={() => (dragging = false)}
/>

<div class="window" inert={app.importOpen || app.paletteOpen}>
<TitleBar />
<div class="content">
{#if !app.initialized}
  <div class="boot">
    <div class="boot-mark"><Icon name="leaf" size={22} /></div>
    <span class="spinner"></span>
    <p>{t("boot.opening")}</p>
  </div>
{:else if app.initError}
  <div class="boot boot-error">
    <div class="boot-mark"><Icon name="leaf" size={22} /></div>
    <h1>{t("boot.failed")}</h1>
    <p class="error-text">{app.initError}</p>
    <button class="btn btn-primary" onclick={retry} disabled={retrying}>
      {retrying ? t("boot.retrying") : t("boot.retry")}
    </button>
  </div>
{:else if !app.config?.vaultPath}
  <Onboarding />
{:else if app.readId}
  {#key app.readId}
    <ReadView id={app.readId} />
  {/key}
{:else}
  <div class="shell">
    <Sidebar />
    <main class="main">
      {#if app.view === "review"}
        <ReviewView />
      {:else if app.view === "settings"}
        <SettingsView />
      {:else}
        <PageList />
      {/if}
    </main>
  </div>
{/if}

</div>
</div>

{#if app.importOpen}
  <div inert={app.paletteOpen}>
    <ImportDialog />
  </div>
{/if}

{#if app.paletteOpen}
  <CommandPalette />
{/if}

{#if dragging}
  <div class="drag-veil" aria-hidden="true">
    <div class="drag-card">
      <Icon name="upload" size={26} />
      <strong>{t("drag.title")}</strong>
      <span>{t("drag.sub")}</span>
    </div>
  </div>
{/if}

<Toasts />

<style>
  .window {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .content {
    flex: 1;
    min-height: 0;
  }
  .boot {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--muted);
    animation: fade-in var(--t-slow) var(--ease-out);
    padding: 32px;
    text-align: center;
  }
  .boot-mark {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    background: var(--accent-soft);
    color: var(--accent-strong);
    margin-bottom: 4px;
  }
  .boot-error h1 {
    font-family: var(--font-display);
    font-size: 22px;
    color: var(--text);
    letter-spacing: -0.01em;
  }
  .error-text {
    max-width: 460px;
    font-size: 13px;
    color: var(--danger);
    background: var(--danger-soft);
    border: 1px solid var(--danger-border);
    border-radius: var(--radius-sm);
    padding: 8px 14px;
    overflow-wrap: anywhere;
  }

  .shell {
    display: grid;
    grid-template-columns: var(--sidebar-w) 1fr;
    height: 100%;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    background: var(--bg);
  }

  .drag-veil {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    background: rgba(35, 40, 29, 0.28);
    backdrop-filter: blur(2px);
    pointer-events: none;
    animation: fade-in var(--t-fast) var(--ease-out);
  }
  .drag-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 28px 36px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    border: 2px dashed var(--accent);
    box-shadow: var(--shadow-lg);
    color: var(--text-soft);
    text-align: center;
    animation: pop-in var(--t-med) var(--ease-spring);
  }
  .drag-card :global(svg) {
    color: var(--accent);
  }
  .drag-card strong {
    font-size: 15px;
    color: var(--text);
  }
  .drag-card span {
    font-size: 12.5px;
    color: var(--muted);
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
  @keyframes pop-in {
    from {
      opacity: 0;
      transform: scale(0.96);
    }
  }
</style>
