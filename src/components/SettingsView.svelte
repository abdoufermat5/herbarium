<script lang="ts">
  import { app, setTheme, setLayout, setSort } from "../lib/state.svelte";
  import { t, i18n, setLocale, LOCALES } from "../lib/i18n.svelte";
  import { prefs, setPref } from "../lib/prefs.svelte";
  import Icon from "../lib/Icon.svelte";
  import Select from "./Select.svelte";
  import SettingsSection from "./settings/SettingsSection.svelte";
  import SettingRow from "./settings/SettingRow.svelte";
  import Switch from "./settings/Switch.svelte";
  import EditorSettingsSection from "./settings/EditorSettingsSection.svelte";
  import ReviewSettingsSection from "./settings/ReviewSettingsSection.svelte";
  import VaultSettingsSection from "./settings/VaultSettingsSection.svelte";

  function setDetailsOpen(open: boolean) {
    setPref("detailsOpen", open);
    app.inspectorOpen = open;
  }
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
      <SettingRow title={t("settings.startView")} hint={t("settings.startViewHint")}>
        <Select
          fill
          size="md"
          align="right"
          value={prefs.startView}
          ariaLabel={t("settings.startView")}
          options={[
            { value: "list", label: t("sidebar.all") },
            { value: "review", label: t("sidebar.review") },
          ]}
          onchange={(v) => setPref("startView", v)}
        />
      </SettingRow>
    </SettingsSection>

    <SettingsSection id="settings-import" title={t("settings.import")}>
      <SettingRow title={t("settings.importTarget")} hint={t("settings.importTargetHint")}>
        <Select
          fill
          size="md"
          align="right"
          value={prefs.importTarget}
          ariaLabel={t("settings.importTarget")}
          options={[
            { value: "browsing", label: t("settings.importBrowsing") },
            { value: "last", label: t("settings.importLast") },
            { value: "root", label: t("import.rootFolder") },
          ]}
          onchange={(v) => setPref("importTarget", v)}
        />
      </SettingRow>
    </SettingsSection>

    <SettingsSection id="settings-reader" title={t("settings.reader")}>
      <SettingRow title={t("settings.detailsOpen")} hint={t("settings.detailsOpenHint")}>
        <Switch checked={prefs.detailsOpen} label={t("settings.detailsOpen")} onchange={setDetailsOpen} />
      </SettingRow>
    </SettingsSection>

    <EditorSettingsSection />

    <ReviewSettingsSection />

    <VaultSettingsSection />

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
    font-size: var(--fs-4xl);
  }
  .sub {
    color: var(--muted);
    font-size: var(--fs-base);
  }
  .save-hint {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--muted);
    font-size: var(--fs-xs);
    padding-bottom: 8px;
  }

  @container settings (max-width: 560px) {
    .settings-inner {
      gap: 32px;
    }
  }
</style>
