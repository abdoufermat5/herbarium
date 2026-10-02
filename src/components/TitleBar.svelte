<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";

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
  <div class="brand" data-tauri-drag-region>
    <Icon name="leaf" size={14} />
    <span data-tauri-drag-region>Herbarium</span>
  </div>
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
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-left: 12px;
    background: var(--surface, var(--bg));
    border-bottom: 1px solid var(--border);
    user-select: none;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-soft);
    pointer-events: none;
  }

  .brand :global(svg) {
    color: var(--accent);
  }

  .controls {
    display: flex;
    height: 100%;
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
    background: var(--danger);
    color: #fff;
  }
</style>
