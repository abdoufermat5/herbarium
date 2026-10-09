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

  /**
   * `ok`: the extension reached the app and the helper still works.
   * `broken`: the extension was added, but the helper is missing or points
   * at an executable that is gone (Herbarium moved or was reinstalled).
   * `off`: the extension was never added.
   */
  type Health = "ok" | "broken" | "off";
  const health = (b: BrowserStatus): Health => (b.seen === null ? "off" : b.connected ? "ok" : "broken");
  const healthLabel = (h: Health) =>
    t(h === "ok" ? "browser.health.ok" : h === "broken" ? "browser.health.broken" : "browser.health.off");

  const active = $derived(browsers.find((b) => b.browser === picked) ?? null);
  const anyWorking = $derived(browsers.some((b) => health(b) === "ok"));
  const extensionsUrl = (b: BrowserStatus) =>
    b.firefox
      ? "about:debugging#/runtime/this-firefox"
      : ({ Edge: "edge://extensions", Brave: "brave://extensions", Vivaldi: "vivaldi://extensions" } as Record<string, string>)[b.browser] ??
        "chrome://extensions";

  async function load() {
    try {
      browsers = await api.browserStatus();
      if (picked === null && browsers.length > 0) {
        picked = (browsers.find((b) => health(b) === "broken") ?? browsers.find((b) => health(b) === "off") ?? browsers[0]).browser;
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

  async function repair() {
    await prepare();
    if (active && health(active) === "ok") toast(t("browser.repaired", { browser: active.browser }));
  }

  async function openBrowser(b: BrowserStatus, page: "extensions" | "package") {
    try {
      if (!setup) await prepare();
      await api.openBrowser(b.browser, page);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  async function revealFolder(firefox: boolean) {
    try {
      await api.revealExtension(firefox);
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
      <div class="banner off">
        <Icon name="info" size={16} />
        <div>
          <strong>{t("browser.none")}</strong>
          <p class="small muted">{t("browser.noneHelp")}</p>
        </div>
      </div>
    {:else}
      <p class="lead">{anyWorking ? t("browser.leadDone") : t("browser.lead")}</p>
      <div class="tabs" role="tablist" aria-label={t("browser.title")}>
        {#each browsers as b (b.browser)}
          {@const h = health(b)}
          <button
            type="button"
            role="tab"
            aria-selected={picked === b.browser}
            class="tab"
            class:on={picked === b.browser}
            title={healthLabel(h)}
            onclick={() => (picked = b.browser)}
          >
            <span class="dot {h}" aria-hidden="true"></span>
            {b.browser}
            <span class="sr-only">({healthLabel(h)})</span>
          </button>
        {/each}
      </div>

      {#if active}
        {@const h = health(active)}
        {#if h === "ok"}
          <div class="banner ok">
            <Icon name="circle-check" size={16} />
            <div>
              <strong>{t("browser.working", { browser: active.browser })}</strong>
              <span class="muted small">{t("browser.lastSeen", { when: timeAgo(active.seen!) })}</span>
              <p class="small muted">{t("browser.workingHelp")}</p>
            </div>
          </div>
        {:else if h === "broken"}
          <div class="banner warn" role="alert">
            <Icon name="wrench" size={16} />
            <div>
              <strong>{t("browser.broken", { browser: active.browser })}</strong>
              <p class="small">{t("browser.brokenHelp")}</p>
              <button class="btn btn-sm btn-primary" onclick={repair} disabled={preparing}>
                <Icon name="refresh-cw" size={12} />{preparing ? t("browser.connecting") : t("browser.repair")}
              </button>
            </div>
          </div>
        {:else}
          <ol class="steps">
            <li class:done={!!setup} class:current={!setup}>
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
            <li class:off={!setup} class:current={!!setup}>
              <strong>{t("browser.step.open", { browser: active.browser })}</strong>
              {#if active.canOpen}
                <button class="btn btn-sm" onclick={() => void openBrowser(active, "extensions")}>
                  <Icon name="external-link" size={12} />{t("browser.openPage", { browser: active.browser })}
                </button>
              {/if}
              <span class="small muted">{active.canOpen ? t("browser.orType") : t("browser.typeIn")}</span>
              <CopyCommand text={extensionsUrl(active)} />
            </li>
            <li class:off={!setup} class:current={!!setup}>
              {#if active.firefox}
                <strong>{t("browser.step.ff")}</strong>
                <span class="small muted">{t("browser.step.ffText")}</span>
              {:else}
                <strong>{t("browser.step.load")}</strong>
                <span class="small muted">{t("browser.step.loadText")}</span>
              {/if}
              {#if setup}
                <CopyCommand text={active.firefox ? `${setup.firefoxExtensionDir}/manifest.json` : setup.extensionDir} />
                <button class="link" onclick={() => void revealFolder(active.firefox)}><Icon name="folder-open" size={12} />{t("browser.showFolder")}</button>
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
    padding: 20px 28px 22px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    align-items: stretch;
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
    gap: 7px;
    padding: 5px 12px;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-soft);
    font: inherit;
    font-size: var(--fs-sm);
    cursor: pointer;
    transition:
      background 0.12s,
      border-color 0.12s;
  }
  .tab:hover {
    background: var(--accent-soft);
  }
  .tab.on {
    border-color: var(--text);
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 500;
  }
  .tab:focus-visible {
    outline: 2px solid var(--accent-ring);
    outline-offset: 2px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: none;
    /* Not set up: a hollow ring; filled once connected. */
    border: 1.5px solid var(--muted);
  }
  .dot.ok,
  .dot.broken {
    border: 0;
  }
  .dot.ok {
    background: var(--ok);
  }
  .dot.broken {
    background: var(--warn);
  }
  .banner {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius, 8px);
    background: var(--surface);
  }
  .banner > :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .banner > div {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-width: 0;
  }
  .banner .btn {
    margin-top: 6px;
  }
  .banner.ok {
    background: var(--ok-soft);
    border-color: transparent;
  }
  .banner.ok > :global(svg) {
    color: var(--ok);
  }
  .banner.warn {
    background: var(--warn-soft);
    border-color: var(--warn-border);
  }
  .banner.warn > :global(svg) {
    color: var(--warn);
  }
  .banner.off > :global(svg) {
    color: var(--muted);
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
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
    gap: 0;
  }
  .steps > li {
    counter-increment: step;
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 0 0 18px 36px;
    transition: opacity 0.15s;
  }
  .steps > li:last-child {
    padding-bottom: 0;
  }
  /* The rail joining the step numbers. */
  .steps > li:not(:last-child)::after {
    content: "";
    position: absolute;
    left: 11px;
    top: 26px;
    bottom: 4px;
    width: 1px;
    background: var(--border);
  }
  .steps > li.off {
    opacity: 0.5;
  }
  .steps > li::before {
    content: counter(step);
    position: absolute;
    left: 0;
    top: -1px;
    width: 23px;
    height: 23px;
    display: grid;
    place-items: center;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    background: var(--surface);
    color: var(--text-soft);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .steps > li.current::before {
    border-color: var(--text);
    background: var(--text);
    color: var(--surface);
  }
  .steps > li.done::before {
    content: "✓";
    border-color: transparent;
    background: var(--ok-soft);
    color: var(--ok);
  }
  .steps :global(.cmd) {
    width: min(460px, 100%);
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
    color: var(--ok);
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
