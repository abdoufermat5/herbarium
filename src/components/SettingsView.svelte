<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
  import { app, setTheme, setLayout, setSort, goView, toast, errorMessage, type ThemeChoice } from "../lib/state.svelte";
  import { navigate } from "../lib/navigation.svelte";
  import { api } from "../lib/api";
  import { confirmAction } from "../lib/confirm.svelte";
  import { t, i18n, setLocale, LOCALES } from "../lib/i18n.svelte";
  import { prefs, setPref } from "../lib/prefs.svelte";
  import type { UpdateChannel, UpdateInfo } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import Select, { type SelectOption } from "./Select.svelte";
  import SettingsSection from "./settings/SettingsSection.svelte";
  import SettingRow from "./settings/SettingRow.svelte";
  import Switch from "./settings/Switch.svelte";
  import EditorSettingsSection from "./settings/EditorSettingsSection.svelte";
  import ReviewSettingsSection from "./settings/ReviewSettingsSection.svelte";
  import VaultSettingsSection from "./settings/VaultSettingsSection.svelte";
  import BrowserSettingsSection from "./settings/BrowserSettingsSection.svelte";
  import CaptureSettingsSection from "./settings/CaptureSettingsSection.svelte";

  function setDetailsOpen(open: boolean) {
    setPref("detailsOpen", open);
    app.inspectorOpen = open;
  }

  const themeOptions: SelectOption<ThemeChoice>[] = $derived([
    { value: "system", label: t("prefs.themeOptionSystem") },
    { value: "light", label: t("prefs.themeOptionLight"), icon: "sun" },
    { value: "dark", label: t("prefs.themeOptionDark"), icon: "moon" },
  ]);

  /* ------------------------------------------------------ desktop integration */

  type Permission = "checking" | "granted" | "denied" | "prompt" | "unsupported";

  let permission = $state<Permission>("checking");

  const permissionLabel = $derived(
    permission === "granted"
      ? t("settings.notifyGranted")
      : permission === "denied"
        ? t("settings.notifyDenied")
        : permission === "unsupported"
          ? t("settings.notifyUnsupported")
          : t("settings.notifyPrompt"),
  );

  async function askPermission() {
    try {
      const result = await requestPermission();
      permission = result === "granted" ? "granted" : result === "denied" ? "denied" : "prompt";
    } catch (e) {
      console.error(e);
      permission = "unsupported";
    }
  }

  async function setTray(enabled: boolean) {
    try {
      app.config = await api.setCloseToTray(enabled);
    } catch (e) {
      console.error(e);
      toast(t("settings.closeToTrayFailed", { detail: errorMessage(e) }), "error");
    }
  }

  /* ----------------------------------------------------------------- updates */

  let version = $state("");
  let update = $state<UpdateInfo | null>(null);
  let checking = $state(false);
  let upToDate = $state(false);
  let updateError = $state("");
  let installing = $state(false);
  // "snap" when the Snap Store owns updates; then the check/install button is
  // replaced by an explanatory row and no endpoint is ever contacted.
  let updateChannel = $state<UpdateChannel | null>(null);
  const snapManaged = $derived(updateChannel === "snap");

  async function initMeta() {
    try {
      version = await getVersion();
    } catch (e) {
      console.error(e);
    }
    try {
      permission = (await isPermissionGranted()) ? "granted" : "prompt";
    } catch (e) {
      console.error(e);
      permission = "unsupported";
    }
    try {
      updateChannel = await api.updateManagedBy();
    } catch (e) {
      console.error(e);
    }
  }

  onMount(() => void initMeta());

  async function checkForUpdates() {
    if (checking || snapManaged) return;
    checking = true;
    updateError = "";
    upToDate = false;
    update = null;
    try {
      const info = await api.checkUpdate();
      if (info) update = info;
      else upToDate = true;
    } catch (e) {
      console.error(e);
      updateError = errorMessage(e);
    } finally {
      checking = false;
    }
  }

  /** Confirm, then let the shared leave guards clear unsaved work before installing. */
  async function installUpdate() {
    const info = update;
    if (!info || installing || snapManaged) return;
    const confirmed = await confirmAction({
      title: t("settings.updateConfirmTitle", { version: info.version }),
      message: t("settings.updateConfirmMessage"),
      confirmLabel: t("settings.updateConfirm"),
    });
    if (!confirmed) return;
    installing = true;
    try {
      const changed = await navigate(() => api.installUpdate(info.version));
      if (changed) toast(t("settings.updateInstalled"), "success");
    } catch (e) {
      console.error(e);
      toast(t("settings.updateInstallFailed", { detail: errorMessage(e) }), "error");
    } finally {
      installing = false;
    }
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
      <button class="btn" onclick={() => void goView("list")}>
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
          value={app.themeChoice}
          ariaLabel={t("prefs.theme")}
          options={themeOptions}
          onchange={(v) => setTheme(v)}
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
            { value: "today", label: t("sidebar.today") },
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

    <BrowserSettingsSection />

    <CaptureSettingsSection />

    <SettingsSection id="settings-desktop" title={t("settings.desktop")}>
      <SettingRow title={t("settings.closeToTray")} hint={t("settings.closeToTrayHint")}>
        <Switch
          checked={app.config?.closeToTray ?? true}
          label={t("settings.closeToTray")}
          onchange={setTray}
        />
      </SettingRow>
      <SettingRow title={t("settings.notifyPermission")} hint={t("settings.notifyPermissionHint")}>
        <div class="perm">
          <span class="status" class:on={permission === "granted"}>{permissionLabel}</span>
          {#if permission === "prompt"}
            <button class="btn btn-sm" onclick={askPermission}>{t("settings.notifyRequest")}</button>
          {/if}
        </div>
      </SettingRow>
    </SettingsSection>

    <SettingsSection
      id="settings-updates"
      title={t("settings.updates")}
      note={snapManaged ? t("settings.updatesSnapHint") : t("settings.updatesHint")}
    >
      {#if snapManaged}
        <SettingRow
          stacked
          title={t("settings.updatesSnapTitle")}
          hint={version ? t("settings.updatesCurrentVersion", { version }) : undefined}
        >
          <span class="managed">
            <Icon name="circle-check" size={14} />
            {t("settings.updatesSnapManaged")}
          </span>
        </SettingRow>
      {:else}
        <SettingRow
          title={t("settings.checkUpdates")}
          hint={version ? t("settings.updatesCurrentVersion", { version }) : undefined}
        >
          <button class="btn" disabled={checking} onclick={checkForUpdates}>
            <Icon name="refresh-cw" size={13} />
            {checking ? t("settings.checking") : t("settings.checkUpdates")}
          </button>
        </SettingRow>
        {#if update}
          <div class="update">
            <p class="update-available">{t("settings.updateAvailable", { version: update.version })}</p>
            {#if update.notes}<pre class="notes">{update.notes}</pre>{/if}
            <button class="btn btn-primary" disabled={installing} onclick={installUpdate}>
              {installing ? t("settings.installing") : t("settings.updateInstall")}
            </button>
          </div>
        {:else if upToDate}
          <p class="update-ok"><Icon name="circle-check" size={14} />{t("settings.updateUpToDate")}</p>
        {:else if updateError}
          <p class="update-err">{t("settings.updateCheckFailed", { detail: updateError })}</p>
        {/if}
      {/if}
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

  .perm {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .status {
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .status.on {
    color: var(--leaf);
  }
  .managed {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--leaf);
    font-size: var(--fs-sm);
  }

  .update {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 20px 28px;
    align-items: flex-start;
  }
  .update-available {
    font-size: var(--fs-base);
  }
  .notes {
    margin: 0;
    max-height: 180px;
    overflow: auto;
    width: 100%;
    padding: 12px 14px;
    background: var(--sunken);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: var(--fs-xs);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--text-soft);
  }
  .update-ok,
  .update-err {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 18px 28px;
    font-size: var(--fs-sm);
  }
  .update-ok {
    color: var(--leaf);
  }
  .update-err {
    color: var(--danger);
    overflow-wrap: anywhere;
  }
</style>
