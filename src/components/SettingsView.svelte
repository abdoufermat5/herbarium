<script lang="ts">
  import { app, setTheme, setLayout, setSort } from "../lib/state.svelte";
  import { t, i18n, setLocale, LOCALES } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import Select from "./Select.svelte";
  import SettingsSection from "./settings/SettingsSection.svelte";
  import SettingRow from "./settings/SettingRow.svelte";
  import ReviewSettingsSection from "./settings/ReviewSettingsSection.svelte";
</script>

<div class="settings">
  <div class="settings-inner">
    <header class="head">
      <div class="head-text">
        <span class="eyebrow">Herbarium</span>
        <h1 class="display">{t("settings.title")}</h1>
        <p class="sub">{t("settings.sub")}</p>
      </div>
      <button class="btn" onclick={() => (app.view = "list")}>
        <Icon name="arrow-left" size={14} />
        {t("sidebar.all")}
      </button>
    </header>

    <SettingsSection id="settings-appearance" title={t("settings.appearance")}>
      <SettingRow title={t("prefs.theme")} hint={t("settings.themeHint")}>
        <Select
          fill
          size="md"
          align="right"
          value={app.theme}
          ariaLabel={t("prefs.theme")}
          options={[
            { value: "light", label: t("prefs.themeOptionLight"), icon: "sun" },
            { value: "dark", label: t("prefs.themeOptionDark"), icon: "moon" },
          ]}
          onchange={setTheme}
        />
      </SettingRow>
      <SettingRow title={t("prefs.language")} hint={t("settings.languageHint")}>
        <Select
          fill
          size="md"
          align="right"
          value={i18n.locale}
          ariaLabel={t("prefs.language")}
          options={[
            { value: "en", label: LOCALES.en.name },
            { value: "fr", label: LOCALES.fr.name },
          ]}
          onchange={setLocale}
        />
      </SettingRow>
    </SettingsSection>

    <SettingsSection id="settings-library" title={t("settings.library")}>
      <SettingRow title={t("list.layout")} hint={t("settings.layoutHint")}>
        <Select
          fill
          size="md"
          align="right"
          value={app.layout}
          ariaLabel={t("list.layout")}
          options={[
            { value: "grid", label: t("list.grid"), icon: "layout-grid" },
            { value: "list", label: t("list.list"), icon: "rows-3" },
          ]}
          onchange={setLayout}
        />
      </SettingRow>
      <SettingRow title={t("list.sort")} hint={t("settings.sortHint")}>
        <Select
          fill
          size="md"
          align="right"
          value={app.sort}
          ariaLabel={t("list.sort")}
          options={[
            { value: "recent", label: t("list.sortRecent") },
            { value: "title", label: t("list.sortTitle") },
            { value: "review", label: t("list.sortReview") },
          ]}
          onchange={setSort}
        />
      </SettingRow>
    </SettingsSection>

    <ReviewSettingsSection />

    <SettingsSection id="settings-vault" title={t("settings.vault")}>
      <div class="vault">
        <p>{t("settings.vaultHint")}</p>
        <div class="vault-path">
          <span>{t("settings.vaultPath")}</span>
          <code>{app.config?.vaultPath}</code>
        </div>
      </div>
    </SettingsSection>

    <p class="save-hint"><Icon name="check" size={14} />{t("settings.saved")}</p>
  </div>
</div>

<style>
  .settings {
    height: 100%;
    overflow-x: hidden;
    overflow-y: auto;
    padding: 48px 40px 72px;
    /* lets rows restack when the pane is narrow, whatever the window or sidebar width */
    container: settings / inline-size;
  }
  .settings-inner {
    max-width: 720px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 40px;
    min-width: 0;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    flex-wrap: wrap;
    gap: 16px 24px;
  }
  .head-text {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  h1 {
    font-size: 40px;
  }
  .sub {
    color: var(--muted);
    font-size: 14px;
  }
  .vault {
    padding: 20px 28px 24px;
  }
  .vault p {
    color: var(--muted);
    font-size: 13px;
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
    font-size: 11px;
    color: var(--muted);
  }
  .vault-path code {
    font-family: var(--mono);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .save-hint {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--muted);
    font-size: 12px;
    padding-bottom: 8px;
  }

  @container settings (max-width: 560px) {
    .settings-inner {
      gap: 32px;
    }
    .vault {
      padding: 18px 20px 20px;
    }
  }
</style>
