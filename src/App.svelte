<script lang="ts">
  import { onMount, tick } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { app, pendingFiles, initApp, clearFilters, toggleSidebar, goView, openPage, reloadPages, toast, errorMessage, openAi } from "./lib/state.svelte";
  import { navigate } from "./lib/navigation.svelte";
  import { folderPickerState } from "./lib/folder-picker.svelte";
  import { api } from "./lib/api";
  import FolderPicker from "./components/FolderPicker.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import PageList from "./components/PageList.svelte";
  import ReviewView from "./components/ReviewView.svelte";
  import TodayView from "./components/TodayView.svelte";
  import GraphView from "./components/GraphView.svelte";
  import SettingsView from "./components/SettingsView.svelte";
  import TrashView from "./components/TrashView.svelte";
  import ReadView from "./components/ReadView.svelte";
  import ImportDialog from "./components/ImportDialog.svelte";
  import AiImportDialog from "./components/AiImportDialog.svelte";
  import LookDialog from "./components/LookDialog.svelte";
  import HealthDialog from "./components/HealthDialog.svelte";
  import AiPanel from "./components/ai/AiPanel.svelte";
  import Thumbnailer from "./components/Thumbnailer.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import TitleBar from "./components/TitleBar.svelte";
  import Toasts from "./components/Toasts.svelte";
  import ConfirmDialog from "./components/ConfirmDialog.svelte";
  import ShortcutsDialog from "./components/ShortcutsDialog.svelte";
  import { confirmState } from "./lib/confirm.svelte";
  import Icon from "./lib/Icon.svelte";
  import Logo from "./lib/Logo.svelte";
  import type { ShortcutContext } from "./lib/shortcuts";
  import type { CaptureOffer } from "./lib/types";
  import { t } from "./lib/i18n.svelte";

  let dragging = $state(false);
  let dragTimer: ReturnType<typeof setTimeout> | undefined;
  let retrying = $state(false);
  let shortcutsOpen = $state(false);

  const shortcutContext: ShortcutContext = $derived(
    app.readId ? "reader" : app.view === "review" ? "review" : app.view === "list" ? "list" : "global",
  );

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
    // The open import dialog stages its own drops for review.
    if (app.importOpen) return;
    const files = Array.from(e.dataTransfer?.files ?? []).filter((f) =>
      /\.html?$/i.test(f.name),
    );
    if (files.length === 0) return;
    pendingFiles.files = files;
    pendingFiles.folder = undefined;
    pendingFiles.allowCdn = undefined;
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
    // A confirmation or folder chooser owns the keyboard until it settles.
    if (confirmState.pending || folderPickerState.pending) return;
    // The shortcuts reference is modal: it owns every key until it closes.
    if (shortcutsOpen) return;
    const mod = e.metaKey || e.ctrlKey;

    // Command palette — works even while typing.
    if (mod && e.key.toLowerCase() === "k") {
      e.preventDefault();
      app.paletteOpen = !app.paletteOpen;
      return;
    }
    if (mod && e.key.toLowerCase() === "b" && app.config?.vaultPath && !app.importOpen && !app.paletteOpen) {
      e.preventDefault();
      toggleSidebar();
      return;
    }
    if (mod && e.key === "," && app.config?.vaultPath && !app.importOpen && !app.paletteOpen) {
      e.preventDefault();
      void goView("settings");
      return;
    }
    // Everything AI lives in one panel; it closes itself on the same keys.
    if (mod && e.key.toLowerCase() === "j" && app.config?.vaultPath && !document.querySelector('[aria-modal="true"]')) {
      e.preventDefault();
      openAi();
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
        // Leaving a field keeps the inspector draft; a second Escape closes.
        if (isTyping(e.target)) {
          (e.target as HTMLElement).blur();
          return;
        }
        void goView(app.view);
        return;
      }
      if (app.view === "settings" || app.view === "trash") {
        if (!e.defaultPrevented) void goView("list");
        return;
      }
      if (app.folderFilter || app.tagFilter) {
        clearFilters();
        return;
      }
      return;
    }

    if (isTyping(e.target) || e.defaultPrevented || e.repeat) return;
    // Single-key shortcuts never act behind an open dialog.
    if (document.querySelector('[aria-modal="true"]')) return;

    if (e.key === "?" && !mod && app.config?.vaultPath && !app.importOpen && !app.paletteOpen) {
      e.preventDefault();
      shortcutsOpen = true;
      return;
    }

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
    app.ai;
    shortcutsOpen;
    if (!focusSeen) {
      focusSeen = true;
      return;
    }
    void tick().then(() => {
      if (app.importOpen || app.paletteOpen || app.ai || shortcutsOpen) return;
      const a = document.activeElement;
      if (a && a !== document.body && a.isConnected && !a.closest("[inert]")) return;
      // Stay in the view that was opened: falling back to the sidebar put the
      // focus ring on "All pages" while another view was shown.
      document
        .querySelector<HTMLElement>(app.readId ? ".main .bar button" : ".main #page-search, .main button")
        ?.focus();
    });
  });

  // Deep links (`herbarium-app://`): the backend emits `deep-link` with
  // {kind:"open", id} or {kind:"review"}. Live links arrive while the window
  // runs; links the app was launched with are drained on mount. Both wait for
  // the vault to finish loading and go through the existing leave guards.
  interface DeepLinkPayload {
    kind: "open" | "review";
    id?: string;
  }

  const deepLinkQueue: DeepLinkPayload[] = [];
  let deepLinksReady = false;
  let deepLinkDraining = false;

  async function handleDeepLink(link: DeepLinkPayload) {
    if (!app.config?.vaultPath) return;
    if (link.kind === "review") {
      await goView("review");
      return;
    }
    const id = link.id;
    if (!id) return;
    const known = () =>
      app.library.some((p) => p.id === id) || app.pages.some((p) => p.id === id);
    // The page may have just been added on disk by `herbarium add`.
    if (!known()) await reloadPages(true);
    if (known()) await openPage(id);
    else toast(t("deepLink.notFound"), "error");
  }

  async function drainDeepLinks() {
    if (!deepLinksReady || deepLinkDraining) return;
    deepLinkDraining = true;
    try {
      while (deepLinkQueue.length > 0) await handleDeepLink(deepLinkQueue.shift()!);
    } finally {
      deepLinkDraining = false;
    }
  }

  function queueDeepLink(link: DeepLinkPayload) {
    deepLinkQueue.push(link);
    void drainDeepLinks();
  }

  // Only act once the library is loaded; anything queued before then is kept.
  $effect(() => {
    if (app.initialized && !deepLinksReady) {
      deepLinksReady = true;
      void drainDeepLinks();
    }
  });

  // Native close and tray quit both use the same serialized leave guards as
  // navigation. The backend never hides the window ahead of a draft prompt.
  let closing = false;
  let lastTrayNav = 0;

  onMount(() => {
    const win = getCurrentWindow();
    const unlisten: Array<() => void> = [];
    let disposed = false;
    const keep = (p: Promise<() => void>) =>
      void p.then((fn) => (disposed ? fn() : unlisten.push(fn))).catch((e) => console.error(e));

    async function requestClose(quit: boolean) {
      if (closing) return;
      closing = true;
      try {
        if (!(await win.isVisible())) {
          await win.show();
          await win.setFocus();
        }
        await navigate(async () => {
          if (quit || !(app.config?.closeToTray ?? true)) await api.quitApp();
          else await win.hide();
        });
      } catch (e) {
        console.error(e);
        toast(errorMessage(e), "error");
      } finally {
        closing = false;
      }
    }

    keep(win.onCloseRequested((event) => {
      event.preventDefault();
      void requestClose(false);
    }));
    keep(listen("quit-requested", () => void requestClose(true)));

    // Tray menu "Review today". The backend emits it on the window and the app, so de-duplicate.
    keep(
      listen<string>("navigate", (event) => {
        if (event.payload !== "review" || !app.config?.vaultPath) return;
        const now = Date.now();
        if (now - lastTrayNav < 500) return;
        lastTrayNav = now;
        void goView("review");
      }),
    );

    // Quick capture: pages saved from outside the window, and offers from
    // the Downloads and clipboard watchers.
    keep(listen("vault-changed", () => void reloadPages(true)));
    keep(
      listen<CaptureOffer>("download-offer", (event) => {
        const offer = event.payload;
        if (!offer.path || !app.config?.vaultPath) return;
        toast(t("capture.downloadOffer", { name: offer.name ?? offer.title }), "info", 15000, {
          label: t("capture.save"),
          run: () =>
            void api
              .saveDownload(offer.path!)
              .then((title) => toast(t("capture.saved", { title }), "success"))
              .catch((e) => toast(`${t("capture.failed")}: ${errorMessage(e)}`, "error")),
        });
      }),
    );
    keep(
      listen<CaptureOffer>("clipboard-offer", (event) => {
        if (!app.config?.vaultPath) return;
        toast(t("capture.clipboardOffer", { title: event.payload.title }), "info", 15000, {
          label: t("capture.save"),
          run: () =>
            void api
              .saveClipboardPage()
              .then((title) => toast(t("capture.saved", { title }), "success"))
              .catch((e) => toast(`${t("capture.failed")}: ${errorMessage(e)}`, "error")),
        });
      }),
    );

    // Deep links from the backend, and any the app was launched with (the
    // matching event fired before this listener existed, so ask for them).
    keep(listen<DeepLinkPayload>("deep-link", (event) => queueDeepLink(event.payload)));
    // A page following a link to another page. Only the page open in the
    // reader may: the hidden frame that draws previews runs pages too, and
    // one that redirects on load would otherwise switch pages out of nowhere.
    keep(
      listen<DeepLinkPayload>("page-link", (event) => {
        if (app.readId) queueDeepLink(event.payload);
      }),
    );
    void invoke<DeepLinkPayload[]>("take_deep_links")
      .then((links) => {
        if (disposed) return;
        for (const link of links) queueDeepLink(link);
      })
      .catch((e) => console.error(e));

    return () => {
      disposed = true;
      unlisten.forEach((fn) => fn());
    };
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

<div
  class="window"
  inert={app.importOpen || app.paletteOpen || !!app.ai || shortcutsOpen || !!confirmState.pending || !!folderPickerState.pending}
>
<TitleBar />
<div class="content">
{#if !app.initialized}
  <div class="boot">
    <div class="boot-mark"><Logo size={56} /></div>
    <span class="spinner"></span>
    <p>{t("boot.opening")}</p>
  </div>
{:else if app.initError}
  <div class="boot boot-error">
    <div class="boot-mark"><Logo size={56} /></div>
    <h1 class="display">{t("boot.failed")}</h1>
    <p class="error-text">{app.initError}</p>
    <button class="btn btn-primary" onclick={retry} disabled={retrying}>
      {retrying ? t("boot.retrying") : t("boot.retry")}
    </button>
  </div>
{:else if !app.config?.vaultPath}
  <Onboarding />
{:else}
  <div class="shell" class:collapsed={!app.sidebarOpen || (app.focusMode && !!app.readId)}>
    {#if app.sidebarOpen && !(app.focusMode && app.readId)}<Sidebar />{/if}
    <main class="main">
      {#if app.readId}
        {#key app.readId}
          <ReadView id={app.readId} />
        {/key}
      {:else if app.view === "today"}
        <TodayView />
      {:else if app.view === "graph"}
        <GraphView />
      {:else if app.view === "review"}
        <ReviewView />
      {:else if app.view === "settings"}
        <SettingsView />
      {:else if app.view === "trash"}
        <TrashView />
      {:else}
        <PageList />
      {/if}
    </main>
  </div>
{/if}

</div>
</div>

{#if app.config?.vaultPath && app.initialized}
  <Thumbnailer />
{/if}
{#if app.healthOpen}
  <HealthDialog />
{/if}
{#if app.lookEdit}
  {#key app.lookEdit}
    <LookDialog />
  {/key}
{/if}
{#if app.ai}
  <div inert={app.paletteOpen || !!confirmState.pending}>
    <AiPanel start={app.ai} />
  </div>
{/if}
{#if app.aiImportOpen}
  <div inert={app.paletteOpen || !!confirmState.pending}>
    <AiImportDialog />
  </div>
{/if}
{#if app.importOpen}
  <div inert={app.paletteOpen || shortcutsOpen || !!confirmState.pending || !!folderPickerState.pending}>
    <ImportDialog />
  </div>
{/if}

{#if app.paletteOpen}
  <div inert={shortcutsOpen || !!confirmState.pending || !!folderPickerState.pending}>
    <CommandPalette />
  </div>
{/if}

{#if folderPickerState.pending}
  <div inert={shortcutsOpen || !!confirmState.pending}>
    <FolderPicker options={folderPickerState.pending} />
  </div>
{/if}

{#if confirmState.pending}
  <ConfirmDialog options={confirmState.pending} />
{/if}

{#if shortcutsOpen}
  <ShortcutsDialog context={shortcutContext} onclose={() => (shortcutsOpen = false)} />
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
    gap: 14px;
    color: var(--muted);
    animation: fade-in var(--t-slow) var(--ease-out);
    padding: 32px;
    text-align: center;
    font-size: var(--fs-sm);
  }
  .boot-mark {
    margin-bottom: 4px;
  }
  .boot-error h1 {
    font-size: var(--fs-3xl);
  }
  .error-text {
    max-width: 460px;
    font-size: var(--fs-sm);
    color: var(--danger);
    background: var(--danger-soft);
    border-radius: var(--radius-sm);
    padding: 10px 14px;
    overflow-wrap: anywhere;
  }

  .shell {
    display: grid;
    grid-template-columns: var(--sidebar-w) 1fr;
    height: 100%;
  }
  .shell.collapsed {
    grid-template-columns: 1fr;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    background-color: var(--bg);
    /* A single warm light spot so the canvas never reads as flat. */
    background-image: radial-gradient(
      ellipse 60% 40% at 70% 0%,
      var(--glow),
      transparent 70%
    );
  }

  .drag-veil {
    position: fixed;
    inset: 0;
    z-index: var(--z-overlay);
    display: grid;
    place-items: center;
    background: var(--scrim);
    pointer-events: none;
    animation: fade-in var(--t-fast) var(--ease-out);
  }
  .drag-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 36px 48px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    border: 1px dashed var(--border-hover);
    box-shadow: var(--shadow-lg);
    color: var(--text-soft);
    text-align: center;
    animation: pop-in var(--t-med) var(--ease-out);
  }
  .drag-card :global(svg) {
    color: var(--text);
    margin-bottom: 6px;
  }
  .drag-card strong {
    font-family: var(--font-display);
    font-size: var(--fs-xl);
    font-weight: 500;
    letter-spacing: -0.02em;
    color: var(--text);
  }
  .drag-card span {
    font-size: var(--fs-sm);
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
      transform: translateY(8px);
    }
  }
</style>
