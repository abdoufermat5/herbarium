<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "../lib/i18n.svelte";
  import { app, openAi, toggleSidebar } from "../lib/state.svelte";
  import { modKey } from "../lib/format";
  import Icon from "../lib/Icon.svelte";

  const win = getCurrentWindow();
  let maximized = $state(false);

  async function sync() {
    maximized = await win.isMaximized();
  }

  onMount(() => {
    void sync();
    let unlisten: (() => void) | undefined;
    void win.onResized(sync).then((fn) => (unlisten = fn));
    return () => unlisten?.();
  });
</script>

<header class="titlebar" data-tauri-drag-region>
  {#if app.config?.vaultPath}
    <button
      class="ctl toggle"
      aria-label={app.sidebarOpen ? t("sidebar.hide") : t("sidebar.show")}
      aria-pressed={app.sidebarOpen}
      title="{app.sidebarOpen ? t('sidebar.hide') : t('sidebar.show')} ({modKey('B')})"
      onclick={toggleSidebar}
    >
      <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">
        <rect x="1.5" y="2.5" width="11" height="9" rx="1.5" />
        <path d="M5.5 2.5v9" />
      </svg>
    </button>
  {/if}
  <div class="spacer" data-tauri-drag-region></div>
  {#if app.config?.vaultPath && app.initialized && !app.initError}
    <!-- The one way into every AI feature, whatever the view. -->
    <button
      class="ask-ai"
      class:on={!!app.ai}
      aria-haspopup="dialog"
      aria-keyshortcuts={modKey("J").startsWith("⌘") ? "Meta+J" : "Control+J"}
      title={t("ai.panelHint")}
      onclick={() => openAi()}
    >
      <Icon name="sparkle" size={13} />
      <span>{t("ai.panel")}</span>
      <kbd>{modKey("J")}</kbd>
    </button>
  {/if}
  <div class="controls">
    <button class="ctl" aria-label={t("window.minimize")} onclick={() => win.minimize()}>
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 5h8" /></svg>
    </button>
    <button
      class="ctl"
      aria-label={maximized ? t("window.restore") : t("window.maximize")}
      onclick={() => win.toggleMaximize()}
    >
      {#if maximized}
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M3 3V1h6v6H7M1 3h6v6H1z" />
        </svg>
      {:else}
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 1h8v8H1z" /></svg>
      {/if}
    </button>
    <button class="ctl close" aria-label={t("window.close")} onclick={() => win.close()}>
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 1l8 8M9 1l-8 8" /></svg>
    </button>
  </div>
</header>

<style>
  .titlebar {
    flex: none;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: flex-start;
    background: var(--raised);
    border-bottom: 1px solid var(--border);
    user-select: none;
  }

  .spacer {
    flex: 1;
    height: 100%;
  }
  .toggle {
    width: 40px;
    height: 100%;
  }
  .controls {
    display: flex;
    height: 100%;
  }

  .ask-ai {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    margin-right: 8px;
    padding: 0 4px 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-soft);
    font-size: var(--fs-xs);
    font-weight: 500;
    transition:
      border-color var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }
  .ask-ai :global(svg) {
    color: var(--leaf);
  }
  .ask-ai:hover,
  .ask-ai.on {
    border-color: var(--border-hover);
    color: var(--text);
  }
  .ask-ai kbd {
    padding: 1px 5px;
    border-radius: var(--radius-xs);
    background: var(--sunken);
    color: var(--muted);
    font-family: var(--mono);
    font-size: var(--fs-2xs);
    line-height: 1.4;
  }

  .ctl {
    width: 44px;
    display: grid;
    place-items: center;
    color: var(--muted);
    transition:
      background var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }

  .ctl svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .ctl:hover {
    background: var(--raised);
    color: var(--text);
  }

  .ctl.close:hover {
    background: var(--danger-fill);
    color: var(--on-danger);
  }
</style>
