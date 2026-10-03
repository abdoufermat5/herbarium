<script lang="ts">
  import { app, setTheme, setLayout, setSort } from "../lib/state.svelte";
  import { t, i18n, setLocale, LOCALES } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import Select from "./Select.svelte";
</script>

<div class="settings">
  <div class="settings-inner">
    <header class="head">
      <div>
        <h1>{t("settings.title")}</h1>
        <p class="sub">{t("settings.sub")}</p>
      </div>
      <button class="btn" onclick={() => (app.view = "list")}>
        <Icon name="arrow-left" size={14} />
        {t("sidebar.all")}
      </button>
    </header>

    <section aria-labelledby="settings-appearance">
      <h2 id="settings-appearance"><Icon name="sun" size={16} />{t("settings.appearance")}</h2>
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

    <section aria-labelledby="settings-library">
      <h2 id="settings-library"><Icon name="files" size={16} />{t("settings.library")}</h2>
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

    <section aria-labelledby="settings-vault">
      <h2 id="settings-vault"><Icon name="folder-open" size={16} />{t("settings.vault")}</h2>
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
    padding: 28px 32px;
  }
  .settings-inner {
    max-width: 760px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 28px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 20px;
  }
  h1 {
    font-family: var(--font-display);
    font-size: 28px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .sub, .setting-copy p, .vault-card p {
    color: var(--muted);
    font-size: 13px;
  }
  .sub {
    margin-top: 4px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
    color: var(--accent-strong);
    font-size: 13px;
    font-weight: 600;
  }
  .settings-card {
    padding: 0 20px;
  }
  .setting-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 180px;
    align-items: center;
    gap: 24px;
    padding: 20px 0;
  }
  .setting-row + .setting-row {
    border-top: 1px solid var(--border);
  }
  h3 {
    font-size: 14px;
    font-weight: 600;
    margin-bottom: 4px;
  }
  .vault-card {
    padding: 20px;
  }
  .vault-path {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 16px;
    padding: 12px 14px;
    background: var(--sunken);
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
      padding: 24px 20px;
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
