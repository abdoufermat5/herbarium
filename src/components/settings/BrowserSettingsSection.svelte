<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api } from "../../lib/api";
  import { errorMessage, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { timeAgo } from "../../lib/format";
  import type { BrowserSetup, BrowserStatus } from "../../lib/types";
  import Icon from "../../lib/Icon.svelte";
  import CopyCommand from "./CopyCommand.svelte";
  import SettingsSection from "./SettingsSection.svelte";

  let browsers = $state<BrowserStatus[]>([]);
  let setup = $state<BrowserSetup | null>(null);
  let loaded = $state(false);
  let preparing = $state(false);
  /** The browser whose steps are shown. */
  let picked = $state<string | null>(null);
  let poll: ReturnType<typeof setInterval> | undefined;

  const active = $derived(browsers.find((b) => b.browser === picked) ?? null);
  const isConnected = (b: BrowserStatus) => b.seen !== null;
  const anyConnected = $derived(browsers.some(isConnected));
  const extensionsUrl = (b: BrowserStatus) =>
    b.firefox
      ? "about:debugging#/runtime/this-firefox"
      : ({ Edge: "edge://extensions", Brave: "brave://extensions", Vivaldi: "vivaldi://extensions" } as Record<string, string>)[b.browser] ??
        "chrome://extensions";

  async function load() {
    try {
      browsers = await api.browserStatus();
      if (picked === null && browsers.length > 0) {
        picked = (browsers.find((b) => !isConnected(b)) ?? browsers[0]).browser;
      }
    } catch (e) {
      console.error(e);
    } finally {
      loaded = true;
    }
  }

  /** One click: register the helper with every browser and put the extension in its folder. */
  async function prepare() {
    preparing = true;
    try {
      setup = await api.browserSetup();
      browsers = setup.browsers;
      if (picked === null && browsers.length > 0) picked = browsers[0].browser;
    } catch (e) {
      toast(`${t("browser.connectFailed")}: ${errorMessage(e)}`, "error");
    } finally {
      preparing = false;
    }
  }

  async function openBrowser(b: BrowserStatus, page: "extensions" | "package") {
    try {
      if (!setup) await prepare();
      await api.openBrowser(b.browser, page);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  async function revealFolder() {
    try {
      await api.revealExtension();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  onMount(() => {
    void load();
    // The extension reaches the app on its own once added: notice it.
    poll = setInterval(() => void load(), 3000);
  });
  onDestroy(() => clearInterval(poll));
</script>

<SettingsSection id="settings-browser" title={t("browser.title")} note={t("browser.note")}>
  <div class="wrap">
    {#if !loaded}
      <p class="muted"><span class="spinner small"></span></p>
    {:else if browsers.length === 0}
      <p class="state bad"><Icon name="info" size={15} />{t("browser.none")}</p>
      <p class="small muted">{t("browser.noneHelp")}</p>
    {:else}
      <p class="lead">{anyConnected ? t("browser.leadDone") : t("browser.lead")}</p>
      <div class="tabs" role="tablist" aria-label={t("browser.title")}>
        {#each browsers as b (b.browser)}
          <button
            type="button"
            role="tab"
            aria-selected={picked === b.browser}
            class="tab"
            class:on={picked === b.browser}
            onclick={() => (picked = b.browser)}
          >
            {#if isConnected(b)}<Icon name="circle-check" size={13} />{/if}
            {b.browser}
          </button>
        {/each}
      </div>

      {#if active}
        {#if isConnected(active)}
          <p class="state good">
            <Icon name="circle-check" size={15} />
            {t("browser.working", { browser: active.browser })}
            <span class="muted">· {t("browser.lastSeen", { when: timeAgo(active.seen!) })}</span>
          </p>
          <p class="small muted">{t("browser.workingHelp")}</p>
        {:else}
          <ol class="steps">
            <li>
              <strong>{t("browser.step.get")}</strong>
              <span class="small muted">{t("browser.step.getText")}</span>
              {#if setup}
                <span class="state good small"><Icon name="circle-check" size={13} />{t("browser.ready")}</span>
              {:else}
                <button class="btn btn-sm btn-primary" onclick={prepare} disabled={preparing}>
                  {preparing ? t("browser.connecting") : t("browser.prepare")}
                </button>
              {/if}
            </li>
            <li class:off={!setup}>
              <strong>{t("browser.step.open", { browser: active.browser })}</strong>
              <div class="actions">
                {#if active.canOpen}
                  <button class="btn btn-sm" onclick={() => void openBrowser(active, "extensions")}>
                    <Icon name="external-link" size={12} />{t("browser.openPage", { browser: active.browser })}
                  </button>
                {/if}
              </div>
              <span class="small muted">{active.canOpen ? t("browser.orType") : t("browser.typeIn")}</span>
              <CopyCommand text={extensionsUrl(active)} />
            </li>
            <li class:off={!setup}>
              {#if active.firefox}
                <strong>{t("browser.step.ff")}</strong>
                <span class="small muted">{t("browser.step.ffText")}</span>
              {:else}
                <strong>{t("browser.step.load")}</strong>
                <span class="small muted">{t("browser.step.loadText")}</span>
              {/if}
              {#if setup}
                <CopyCommand text={active.firefox ? `${setup.extensionDir}/manifest.json` : setup.extensionDir} />
                <button class="link" onclick={revealFolder}><Icon name="folder-open" size={12} />{t("browser.showFolder")}</button>
              {/if}
            </li>
            <li class="waiting" class:off={!setup}>
              <strong>{t("browser.step.done")}</strong>
              <span class="small muted"><span class="spinner small"></span>{t("browser.waiting")}</span>
            </li>
          </ol>
          {#if active.firefox}
            <p class="small muted">{t("browser.ffNote")}</p>
          {/if}
        {/if}
      {/if}
    {/if}
  </div>
</SettingsSection>

<style>
  .wrap {
    padding: 20px 28px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    align-items: flex-start;
  }
  .lead {
    margin: 0;
    font-weight: 500;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-soft);
    font: inherit;
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .tab.on {
    border-color: var(--leaf);
    background: var(--leaf-soft);
    color: var(--text);
    font-weight: 500;
  }
  .state {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    font-weight: 500;
  }
  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
    counter-reset: step;
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
  }
  .steps > li {
    counter-increment: step;
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding-left: 34px;
  }
  .steps > li.off {
    opacity: 0.55;
  }
  .steps > li::before {
    content: counter(step);
    position: absolute;
    left: 0;
    top: -1px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--text);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .steps :global(.cmd) {
    width: min(460px, 100%);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .link {
    border: 0;
    padding: 0;
    background: none;
    color: var(--text-soft);
    font: inherit;
    font-size: var(--fs-sm);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .small {
    font-size: var(--fs-sm);
    margin: 0;
  }
  .muted {
    color: var(--muted);
  }
  .good {
    color: var(--ok, var(--leaf));
  }
  .bad {
    color: var(--danger);
  }
  .waiting .small {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .spinner.small {
    width: 12px;
    height: 12px;
  }
</style>
