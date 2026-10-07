<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import { errorMessage, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Icon from "../../lib/Icon.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";

  let browsers = $state<Array<{ browser: string; connected: boolean }>>([]);
  let loaded = $state(false);
  let busy = $state(false);

  async function load() {
    try {
      browsers = await api.browserStatus();
    } catch (e) {
      console.error(e);
    } finally {
      loaded = true;
    }
  }

  async function connect() {
    busy = true;
    try {
      const done = await api.connectBrowsers();
      toast(t("browser.connected", { count: done.length }), "success");
      await load();
    } catch (e) {
      toast(`${t("browser.connectFailed")}: ${errorMessage(e)}`, "error");
    } finally {
      busy = false;
    }
  }

  async function revealExtension() {
    try {
      const path = await api.revealExtension();
      toast(t("browser.extensionRevealed", { path }), "info", 12000);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  onMount(() => void load());
</script>

<SettingsSection id="settings-browser" title={t("browser.title")} note={t("browser.note")}>
  <SettingRow title={t("browser.connect")} hint={t("browser.connectHint")} stacked>
    <div class="row">
      {#if loaded}
        {#if browsers.length === 0}
          <span class="muted">{t("browser.none")}</span>
        {:else}
          <ul class="browsers">
            {#each browsers as b (b.browser)}
              <li class:on={b.connected}>
                <Icon name={b.connected ? "circle-check" : "info"} size={13} />
                {b.browser} · {b.connected ? t("browser.isConnected") : t("browser.notConnected")}
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
      <button class="btn" onclick={connect} disabled={busy || (loaded && browsers.length === 0)}>
        {busy ? t("browser.connecting") : t("browser.connectAction")}
      </button>
    </div>
  </SettingRow>
  <SettingRow title={t("browser.extension")} hint={t("browser.extensionHint")}>
    <button class="btn" onclick={revealExtension}>
      <Icon name="folder-open" size={13} />
      {t("browser.extensionAction")}
    </button>
  </SettingRow>
</SettingsSection>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .browsers {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .browsers li {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .browsers li.on {
    color: var(--leaf);
  }
  .muted {
    color: var(--muted);
    font-size: var(--fs-sm);
  }
</style>
