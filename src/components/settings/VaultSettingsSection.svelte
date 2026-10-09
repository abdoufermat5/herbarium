<script lang="ts">
  import { onMount } from "svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { api } from "../../lib/api";
  import { app, adoptVault, rescanVault, reloadPages, switchVault, toast, errorMessage } from "../../lib/state.svelte";
  import { navigate } from "../../lib/navigation.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { prefs, setPref } from "../../lib/prefs.svelte";
  import Icon from "../../lib/Icon.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";
  import Switch from "./Switch.svelte";
  import { publishSite } from "../../lib/exports";

  let scanning = $state(false);
  let busy = $state<"open" | "create" | null>(null);
  let newName = $state("Herbarium");
  let exporting = $state(false);

  let netDefault = $state(false);
  let netLoaded = $state(false);
  let netSaving = $state(false);
  let netError = $state("");

  const recent = $derived(app.config?.recentVaults ?? []);
  const current = $derived(app.config?.vaultPath ?? null);

  async function loadNetwork() {
    try {
      netDefault = (await api.networkSettings()).defaultAllowCdn;
    } catch (e) {
      console.error(e);
      netError = errorMessage(e);
    } finally {
      netLoaded = true;
    }
  }

  let reviewEdits = $state(false);
  let agentsLoaded = $state(false);
  let agentsSaving = $state(false);
  let agentsError = $state("");

  async function loadAgents() {
    try {
      reviewEdits = (await api.agentSettings()).reviewEdits;
    } catch (e) {
      console.error(e);
      agentsError = errorMessage(e);
    } finally {
      agentsLoaded = true;
    }
  }

  async function setReviewEdits(value: boolean) {
    if (agentsSaving) return;
    agentsSaving = true;
    try {
      reviewEdits = (await api.configureAgents({ reviewEdits: value })).reviewEdits;
      toast(value ? t("settings.reviewEditsOn") : t("settings.reviewEditsOff"), "success");
    } catch (e) {
      console.error(e);
      toast(t("settings.agentsSaveFailed", { detail: errorMessage(e) }), "error");
    } finally {
      agentsSaving = false;
    }
  }

  onMount(() => {
    void loadNetwork();
    void loadAgents();
  });

  async function rescan() {
    if (scanning) return;
    scanning = true;
    try {
      const report = await rescanVault();
      toast(t("settings.rescanned", { indexed: report.indexed, removed: report.removed }), "success");
    } catch (e) {
      console.error(e);
      toast(t("settings.rescanFailed"), "error");
    } finally {
      scanning = false;
    }
  }

  async function revealVaultNow() {
    try {
      await api.revealVault();
    } catch (e) {
      console.error(e);
      toast(t("settings.vaultRevealFailed", { detail: errorMessage(e) }), "error");
    }
  }

  /** Switch only after the shared leave guards allow it, and adopt on success. */
  async function openVault() {
    if (busy) return;
    try {
      const picked = await open({ directory: true, title: t("onboard.openDialog") });
      if (typeof picked !== "string") return;
      busy = "open";
      const changed = await navigate(async () => {
        const config = await api.setVault(picked);
        await adoptVault(config);
      });
      if (changed) toast(t("toast.vaultOpened"), "success");
    } catch (e) {
      console.error(e);
      toast(t("settings.vaultOpenFailed", { detail: errorMessage(e) }), "error");
    } finally {
      busy = null;
    }
  }

  async function openRecent(path: string) {
    if (busy || path === current) return;
    busy = "open";
    try {
      const changed = await switchVault(path);
      if (changed) toast(t("toast.vaultOpened"), "success");
    } catch (e) {
      console.error(e);
      toast(t("settings.vaultOpenFailed", { detail: errorMessage(e) }), "error");
    } finally {
      busy = null;
    }
  }

  async function createNew() {
    if (busy) return;
    try {
      const parent = await open({ directory: true, title: t("onboard.createDialog") });
      if (typeof parent !== "string") return;
      const name = newName.trim() || "Herbarium";
      busy = "create";
      const changed = await navigate(async () => {
        const config = await api.createVault(parent, name);
        await adoptVault(config);
      });
      if (changed) toast(t("toast.vaultCreated", { name }), "success");
    } catch (e) {
      console.error(e);
      toast(t("settings.vaultCreateFailed", { detail: errorMessage(e) }), "error");
    } finally {
      busy = null;
    }
  }

  async function removeRecent(path: string) {
    try {
      app.config = await api.removeRecentVault(path);
    } catch (e) {
      console.error(e);
      toast(t("settings.recentRemoveFailed", { detail: errorMessage(e) }), "error");
    }
  }

  async function exportVaultCopy() {
    if (exporting) return;
    try {
      const dest = await save({
        defaultPath: "Herbarium.zip",
        title: t("settings.exportDialog"),
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });
      if (typeof dest !== "string") return;
      exporting = true;
      await api.exportVault(dest);
      toast(t("settings.exported", { path: dest }), "success");
    } catch (e) {
      console.error(e);
      toast(t("settings.exportFailed", { detail: errorMessage(e) }), "error");
    } finally {
      exporting = false;
    }
  }

  async function addSampleVaultPages() {
    try {
      const result = await api.addSamples();
      await reloadPages(true);
      toast(result.added > 0 ? t("samples.added", { count: result.added }) : t("samples.already"), "success");
    } catch (e) {
      toast(`${t("samples.failed")}: ${errorMessage(e)}`, "error");
    }
  }

  async function setNetworkDefault(value: boolean) {
    if (netSaving) return;
    netSaving = true;
    try {
      const settings = await api.configureNetwork(value);
      netDefault = settings.defaultAllowCdn;
      toast(value ? t("settings.networkOn") : t("settings.networkOff"), "success");
    } catch (e) {
      console.error(e);
      toast(t("settings.networkSaveFailed", { detail: errorMessage(e) }), "error");
    } finally {
      netSaving = false;
    }
  }
</script>

<SettingsSection id="settings-vault" title={t("settings.vault")} note={t("settings.vaultHint")}>
  <div class="vault-path">
    <span>{t("settings.vaultPath")}</span>
    <code>{current ?? "—"}</code>
  </div>
  <SettingRow title={t("settings.vaultReveal")} hint={t("settings.vaultRevealHint")}>
    <button class="btn" onclick={revealVaultNow}>
      <Icon name="external-link" size={13} />
      {t("settings.vaultRevealAction")}
    </button>
  </SettingRow>
  <SettingRow title={t("settings.export")} hint={t("settings.exportHint")}>
    <button class="btn" disabled={exporting} onclick={exportVaultCopy}>
      <Icon name="upload" size={13} />
      {exporting ? t("settings.exporting") : t("settings.exportAction")}
    </button>
  </SettingRow>
  <SettingRow title={t("health.settingTitle")} hint={t("health.settingHint")}>
    <button class="btn" onclick={() => (app.healthOpen = true)}>
      <Icon name="circle-check" size={13} />
      {t("health.check")}
    </button>
  </SettingRow>
  <SettingRow title={t("samples.title")} hint={t("samples.hint")}>
    <button class="btn" onclick={addSampleVaultPages}>
      <Icon name="file-plus" size={13} />
      {t("samples.action")}
    </button>
  </SettingRow>
  <SettingRow title={t("export.site")} hint={t("export.siteHint")}>
    <button class="btn" onclick={() => void publishSite(null)}>
      <Icon name="upload" size={13} />
      {t("export.siteAction")}
    </button>
  </SettingRow>
  <SettingRow title={t("settings.refreshOnFocus")} hint={t("settings.refreshOnFocusHint")}>
    <Switch
      checked={prefs.refreshOnFocus}
      label={t("settings.refreshOnFocus")}
      onchange={(v) => setPref("refreshOnFocus", v)}
    />
  </SettingRow>
  <SettingRow title={t("settings.rescan")} hint={t("settings.rescanHint")}>
    <button class="btn rescan" disabled={scanning} onclick={rescan}>
      {scanning ? t("settings.rescanning") : t("settings.rescanNow")}
    </button>
  </SettingRow>
</SettingsSection>

<SettingsSection
  id="settings-vaults"
  title={t("settings.vaultSwitcher")}
  note={t("settings.recentVaultsHint")}
>
  <div class="recent">
    {#if recent.length === 0}
      <p class="recent-empty">{t("settings.recentEmpty")}</p>
    {:else}
      <ul class="recent-list">
        {#each recent as path (path)}
          <li class="recent-item">
            <button
              class="recent-open"
              disabled={path === current || !!busy}
              onclick={() => openRecent(path)}
              title={path}
            >
              <Icon name="folder" size={13} />
              <span class="recent-path">{path}</span>
              {#if path === current}<span class="badge">{t("settings.recentCurrent")}</span>{/if}
            </button>
            {#if path !== current}
              <button
                class="btn btn-icon btn-sm"
                aria-label={t("settings.recentRemove", { path })}
                title={t("settings.recentRemove", { path })}
                onclick={() => removeRecent(path)}
              >
                <Icon name="x" size={12} />
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  <SettingRow title={t("settings.vaultOpen")} hint={t("settings.vaultOpenHint")}>
    <button class="btn" disabled={busy === "open"} onclick={openVault}>
      <Icon name="folder-open" size={13} />
      {busy === "open" ? t("settings.vaultOpening") : t("settings.vaultOpenAction")}
    </button>
  </SettingRow>
  <SettingRow title={t("settings.vaultCreate")} hint={t("settings.vaultCreateHint")} stacked>
    <div class="create">
      <input
        type="text"
        bind:value={newName}
        aria-label={t("settings.vaultNameLabel")}
        placeholder={t("settings.vaultNameLabel")}
        spellcheck="false"
        autocomplete="off"
      />
      <button class="btn btn-primary" disabled={busy === "create"} onclick={createNew}>
        <Icon name="plus" size={13} />
        {busy === "create" ? t("settings.vaultCreating") : t("settings.vaultCreateAction")}
      </button>
    </div>
  </SettingRow>
</SettingsSection>

<SettingsSection id="settings-network" title={t("settings.network")} note={t("settings.networkWarning")}>
  <SettingRow title={t("settings.networkDefault")} hint={t("settings.networkDefaultHint")}>
    <Switch
      checked={netDefault}
      disabled={!netLoaded || netSaving || !!netError}
      label={t("settings.networkDefault")}
      onchange={setNetworkDefault}
    />
  </SettingRow>
  {#if netError}
    <p class="net-error">{t("settings.networkReadFailed", { detail: netError })}</p>
  {/if}
</SettingsSection>

<SettingsSection id="settings-agents" title={t("settings.agents")} note={t("settings.agentsNote")}>
  <SettingRow title={t("settings.reviewEdits")} hint={t("settings.reviewEditsHint")}>
    <Switch
      checked={reviewEdits}
      disabled={!agentsLoaded || agentsSaving || !!agentsError}
      label={t("settings.reviewEdits")}
      onchange={setReviewEdits}
    />
  </SettingRow>
  {#if agentsError}
    <p class="net-error">{t("settings.agentsReadFailed", { detail: agentsError })}</p>
  {/if}
</SettingsSection>

<style>
  .vault-path {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 20px 28px;
  }
  .vault-path span {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .vault-path code {
    font-family: var(--mono);
    font-size: var(--fs-xs);
    overflow-wrap: anywhere;
  }
  .rescan {
    width: 100%;
    height: 36px;
  }

  .recent {
    padding: 16px 28px;
  }
  .recent-empty {
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .recent-list {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .recent-item {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .recent-open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-soft);
    font: inherit;
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease-out);
  }
  .recent-open:hover:not(:disabled) {
    background: var(--sunken);
  }
  .recent-open:disabled {
    cursor: default;
    color: var(--muted);
  }
  .recent-path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: var(--fs-xs);
  }
  .badge {
    flex: none;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--leaf-soft);
    color: var(--leaf);
    font-size: var(--fs-2xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .create {
    display: flex;
    gap: 8px;
    width: 100%;
    max-width: 420px;
  }
  .create input {
    flex: 1;
    min-width: 0;
    background: var(--surface);
  }

  .net-error {
    padding: 14px 28px;
    color: var(--danger);
    font-size: var(--fs-sm);
    overflow-wrap: anywhere;
  }
</style>
