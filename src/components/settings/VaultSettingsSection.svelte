<script lang="ts">
  import { app, rescanVault, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { prefs, setPref } from "../../lib/prefs.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";
  import Switch from "./Switch.svelte";

  let scanning = $state(false);

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
</script>

<SettingsSection id="settings-vault" title={t("settings.vault")}>
  <div class="vault">
    <p>{t("settings.vaultHint")}</p>
    <div class="vault-path">
      <span>{t("settings.vaultPath")}</span>
      <code>{app.config?.vaultPath}</code>
    </div>
  </div>
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

<style>
  .vault {
    padding: 20px 28px 24px;
  }
  .vault p {
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .vault-path {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 16px;
    padding: 14px 16px;
    background: var(--raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
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

  @container settings (max-width: 560px) {
    .vault {
      padding: 18px 20px 20px;
    }
  }
</style>
