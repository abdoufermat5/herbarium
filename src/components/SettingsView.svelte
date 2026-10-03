<script lang="ts">
  import { app, setTheme, setLayout, setSort } from "../lib/state.svelte";
  import { t, i18n, setLocale, LOCALES } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { reveal } from "../lib/reveal";
  import Select from "./Select.svelte";
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

    <section aria-labelledby="settings-appearance" use:reveal>
      <h2 id="settings-appearance" class="eyebrow">{t("settings.appearance")}</h2>
      <div class="card settings-card">
        <div class="setting-row">
          <div class="setting-copy">
            <h3>{t("prefs.theme")}</h3>
            <p>{t("settings.themeHint")}</p>
          </div>
          <Select
            value={app.theme}
            ariaLabel={t("prefs.theme")}
            size="md"
            align="right"
            options={[
              { value: "light", label: t("prefs.themeOptionLight"), icon: "sun" },
              { value: "dark", label: t("prefs.themeOptionDark"), icon: "moon" },
            ]}
            onchange={setTheme}
          />
        </div>
        <div class="setting-row">
          <div class="setting-copy">
            <h3>{t("prefs.language")}</h3>
            <p>{t("settings.languageHint")}</p>
          </div>
          <Select
            value={i18n.locale}
            ariaLabel={t("prefs.language")}
            size="md"
            align="right"
            options={[
              { value: "en", label: LOCALES.en.name },
              { value: "fr", label: LOCALES.fr.name },
            ]}
            onchange={setLocale}
          />
        </div>
      </div>
    </section>

    <section aria-labelledby="settings-library" use:reveal>
      <h2 id="settings-library" class="eyebrow">{t("settings.library")}</h2>
      <div class="card settings-card">
        <div class="setting-row">
          <div class="setting-copy">
            <h3>{t("list.layout")}</h3>
            <p>{t("settings.layoutHint")}</p>
          </div>
          <Select
            value={app.layout}
            ariaLabel={t("list.layout")}
            size="md"
            align="right"
            options={[
              { value: "grid", label: t("list.grid"), icon: "layout-grid" },
              { value: "list", label: t("list.list"), icon: "rows-3" },
            ]}
            onchange={setLayout}
          />
        </div>
        <div class="setting-row">
          <div class="setting-copy">
            <h3>{t("list.sort")}</h3>
            <p>{t("settings.sortHint")}</p>
          </div>
          <Select
            value={app.sort}
            ariaLabel={t("list.sort")}
            size="md"
            align="right"
            options={[
              { value: "recent", label: t("list.sortRecent") },
              { value: "title", label: t("list.sortTitle") },
              { value: "review", label: t("list.sortReview") },
            ]}
            onchange={setSort}
          />
        </div>
      </div>
    </section>

    <section aria-labelledby="settings-vault" use:reveal>
      <h2 id="settings-vault" class="eyebrow">{t("settings.vault")}</h2>
      <div class="card vault-card">
        <p>{t("settings.vaultHint")}</p>
        <div class="vault-path">
          <span>{t("settings.vaultPath")}</span>
          <code>{app.config?.vaultPath}</code>
        </div>
      </div>
    </section>

    <p class="save-hint"><Icon name="check" size={14} />{t("settings.saved")}</p>
  </div>
</div>

<style>
  .settings {
    height: 100%;
    overflow-y: auto;
    padding: 48px 40px 72px;
  }
  .settings-inner {
    max-width: 720px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 40px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 24px;
  }
  .head-text {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h1 {
    font-size: 40px;
  }
  .sub,
  .setting-copy p,
  .vault-card p {
    color: var(--muted);
    font-size: 13px;
  }
  .sub {
    font-size: 14px;
  }
  h2 {
    margin-bottom: 12px;
  }
  .settings-card {
    padding: 0 28px;
  }
  .setting-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 180px;
    align-items: center;
    gap: 24px;
    padding: 24px 0;
  }
  .setting-row + .setting-row {
    border-top: 1px solid var(--border);
  }
  h3 {
    font-size: 14px;
    font-weight: 500;
    margin-bottom: 2px;
  }
  .vault-card {
    padding: 24px 28px;
  }
  .vault-path {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 18px;
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
  @media (max-width: 900px) {
    .settings {
      padding: 32px 24px;
    }
    .setting-row {
      grid-template-columns: minmax(0, 1fr);
      gap: 12px;
    }
    .head {
      flex-wrap: wrap;
    }
  }
</style>
