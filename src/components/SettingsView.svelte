<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
  import { app, setTheme, setLayout, setSort, toast, errorMessage, type SettingsTab, type ThemeChoice } from "../lib/state.svelte";
  import type { IconName } from "../lib/icons";
  import { navigate } from "../lib/navigation.svelte";
  import { api } from "../lib/api";
  import { confirmAction } from "../lib/confirm.svelte";
  import { t, i18n, setLocale, LOCALES } from "../lib/i18n.svelte";
  import { prefs, setPref } from "../lib/prefs.svelte";
  import { TEXT_SCALES, TYPE_STYLES } from "../lib/typography";
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
  import AiSettingsSection from "./settings/AiSettingsSection.svelte";
  import PublishSettingsSection from "./settings/PublishSettingsSection.svelte";

  const TABS: Array<{ id: SettingsTab; icon: IconName }> = [
    { id: "general", icon: "settings" },
    { id: "review", icon: "calendar-clock" },
    { id: "vault", icon: "folder" },
    { id: "capture", icon: "upload" },
    { id: "ai", icon: "sparkle" },
    { id: "about", icon: "info" },
  ];

  let scroller: HTMLDivElement | undefined = $state();
  let tabsEl: HTMLDivElement | undefined = $state();
  /** Position of the active tab, in the tabs' own scrolled coordinates. */
  let tabIndicator = $state({ x: 0, width: 0 });

  function measureTabs() {
    if (!tabsEl) return;
    const active = tabsEl.querySelector<HTMLElement>('[aria-selected="true"]');
    if (!active) return;
    tabIndicator = { x: active.offsetLeft, width: active.offsetWidth };
  }

  // Language and panel width both move the tabs; remeasure after either changes.
  $effect(() => {
    app.settingsTab;
    i18n.locale;
    requestAnimationFrame(measureTabs);
  });

  function pickTab(tab: SettingsTab) {
    app.settingsTab = tab;
    scroller?.scrollTo({ top: 0 });
  }

  onMount(() => {
    const ro = new ResizeObserver(() => measureTabs());
    if (tabsEl) ro.observe(tabsEl);
    // Geist may finish loading after mount, changing every label's width.
    void document.fonts?.ready.then(() => measureTabs());
    return () => ro.disconnect();
  });

  /** Arrow keys move between tabs, as in any tab list. */
  function onTabKey(e: KeyboardEvent) {
    const i = TABS.findIndex((x) => x.id === app.settingsTab);
    const next = e.key === "ArrowRight" ? i + 1 : e.key === "ArrowLeft" ? i - 1 : e.key === "Home" ? 0 : e.key === "End" ? TABS.length - 1 : null;
    if (next === null) return;
    e.preventDefault();
    const tab = TABS[(next + TABS.length) % TABS.length];
    pickTab(tab.id);
    document.getElementById(`settings-tab-${tab.id}`)?.focus();
  }

  function setDetailsOpen(open: boolean) {
    setPref("detailsOpen", open);
    app.inspectorOpen = open;
  }

  const themeOptions: SelectOption<ThemeChoice>[] = $derived([
    { value: "system", label: t("prefs.themeOptionSystem") },
    { value: "light", label: t("prefs.themeOptionLight"), icon: "sun" },
    { value: "dark", label: t("prefs.themeOptionDark"), icon: "moon" },
  ]);

  /* --------------------------------------------------------------- type style */

  const sizeOptions = $derived([
    { value: 0.92, label: t("settings.typeSizeSmall") },
    { value: 1, label: t("settings.typeSizeDefault") },
    { value: 1.08, label: t("settings.typeSizeLarge") },
    { value: 1.16, label: t("settings.typeSizeLarger") },
  ]);

  let specimens: HTMLDivElement | undefined = $state();

  /** Arrow keys walk the specimen radios and apply each one, as a radio group does. */
  function onSpecimenKey(e: KeyboardEvent) {
    const len = TYPE_STYLES.length;
    const i = TYPE_STYLES.findIndex((s) => s.id === prefs.typeStyle);
    const next =
      e.key === "Home"
        ? TYPE_STYLES[0]
        : e.key === "End"
          ? TYPE_STYLES[len - 1]
          : e.key === "ArrowRight" || e.key === "ArrowDown"
            ? TYPE_STYLES[(i + 1) % len]
            : e.key === "ArrowLeft" || e.key === "ArrowUp"
              ? TYPE_STYLES[(i - 1 + len) % len]
              : null;
    if (!next) return;
    e.preventDefault();
    setPref("typeStyle", next.id);
    requestAnimationFrame(() =>
      specimens?.querySelector<HTMLButtonElement>(`[data-style="${next.id}"]`)?.focus(),
    );
  }

  function onSizeKey(e: KeyboardEvent) {
    const len = TEXT_SCALES.length;
    const i = TEXT_SCALES.indexOf(prefs.textScale as (typeof TEXT_SCALES)[number]);
    const next =
      e.key === "Home"
        ? TEXT_SCALES[0]
        : e.key === "End"
          ? TEXT_SCALES[len - 1]
          : e.key === "ArrowRight" || e.key === "ArrowDown"
            ? TEXT_SCALES[(i + 1) % len]
            : e.key === "ArrowLeft" || e.key === "ArrowUp"
              ? TEXT_SCALES[(i - 1 + len) % len]
              : null;
    if (next === null) return;
    e.preventDefault();
    setPref("textScale", next);
    document.querySelector<HTMLButtonElement>(`[data-scale="${next}"]`)?.focus();
  }

  function resetType() {
    if (prefs.typeStyle !== "herbarium") setPref("typeStyle", "herbarium");
    if (prefs.textScale !== 1) setPref("textScale", 1);
  }

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

<div class="settings" bind:this={scroller}>
  <div class="settings-inner">
    <header class="head">
      <h1 class="display">{t("settings.title")}</h1>
      <div class="tabs" role="tablist" aria-label={t("settings.title")} tabindex="-1" onkeydown={onTabKey} bind:this={tabsEl}>
        <span
          class="tab-indicator"
          aria-hidden="true"
          style={`--x: ${tabIndicator.x}px; --w: ${tabIndicator.width}`}
        ></span>
        {#each TABS as tab (tab.id)}
          <button
            id={`settings-tab-${tab.id}`}
            role="tab"
            class="tab"
            aria-selected={app.settingsTab === tab.id}
            aria-controls="settings-panel"
            tabindex={app.settingsTab === tab.id ? 0 : -1}
            onclick={() => pickTab(tab.id)}
          >
            <Icon name={tab.icon} size={14} />
            {t(`settings.tab.${tab.id}`)}
          </button>
        {/each}
      </div>
    </header>

    {#key app.settingsTab}
    <div id="settings-panel" class="panel" role="tabpanel" aria-labelledby={`settings-tab-${app.settingsTab}`}>
    {#if app.settingsTab === "general"}
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
      <SettingRow stacked title={t("settings.typeStyle")} hint={t("settings.typeStyleHint")}>
        <div
          class="specimens"
          role="radiogroup"
          aria-label={t("settings.typeStyle")}
          onkeydown={onSpecimenKey}
          bind:this={specimens}
        >
          {#each TYPE_STYLES as style (style.id)}
            <button
              type="button"
              role="radio"
              aria-checked={prefs.typeStyle === style.id}
              tabindex={prefs.typeStyle === style.id ? 0 : -1}
              class="specimen"
              data-type={style.id}
              data-style={style.id}
              onclick={() => setPref("typeStyle", style.id)}
            >
              <span class="specimen-aa" aria-hidden="true">Aa</span>
              <span class="specimen-name">{t(style.nameKey)}</span>
              <span class="specimen-sample" aria-hidden="true">{t("settings.typeSample")}</span>
              <span class="specimen-mono" aria-hidden="true">{t("settings.typeMonoSample")}</span>
              <span class="specimen-blurb">{t(style.blurbKey)}</span>
              {#if prefs.typeStyle === style.id}
                <span class="specimen-check" aria-hidden="true"><Icon name="check" size={12} /></span>
              {/if}
            </button>
          {/each}
        </div>
      </SettingRow>
      <SettingRow stacked title={t("settings.textSize")} hint={t("settings.textSizeHint")}>
        <div class="size-control">
          <div class="seg" role="radiogroup" aria-label={t("settings.textSize")} onkeydown={onSizeKey}>
            {#each sizeOptions as option (option.value)}
              <button
                type="button"
                role="radio"
                aria-checked={prefs.textScale === option.value}
                tabindex={prefs.textScale === option.value ? 0 : -1}
                class="seg-btn size-btn"
                class:active={prefs.textScale === option.value}
                data-scale={option.value}
                onclick={() => setPref("textScale", option.value)}
              >
                {option.label}
              </button>
            {/each}
          </div>
          {#if prefs.typeStyle !== "herbarium" || prefs.textScale !== 1}
            <button type="button" class="btn btn-ghost btn-sm" onclick={resetType}>
              {t("settings.typeReset")}
            </button>
          {/if}
        </div>
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
          value={app.sort === "relevance" ? "recent" : app.sort}
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

    {:else if app.settingsTab === "review"}
      <ReviewSettingsSection />
    {:else if app.settingsTab === "vault"}
      <VaultSettingsSection />
    {:else if app.settingsTab === "capture"}
      <BrowserSettingsSection />
      <CaptureSettingsSection />
    {:else if app.settingsTab === "ai"}
      <AiSettingsSection />
      <PublishSettingsSection />
    {:else}
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

    {/if}
    </div>
    {/key}

    <p class="save-hint"><Icon name="check" size={14} />{t("settings.saved")}</p>
  </div>
</div>

<style>
  .settings {
    height: 100%;
    overflow-x: hidden;
    overflow-y: auto;
    padding: 40px 40px 56px;
    /* lets rows restack when the pane is narrow, whatever the window or sidebar width */
    container: settings / inline-size;
  }
  .settings-inner {
    max-width: 720px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 28px;
    min-width: 0;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h1 {
    font-size: var(--fs-3xl);
  }
  .tabs {
    position: relative;
    display: flex;
    gap: 2px;
    overflow-x: auto;
    scrollbar-width: none;
    border-bottom: 1px solid var(--border);
    margin: 0 -4px;
    padding: 0 4px;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }
  /* One sliding element marks the active tab, instead of a border per button.
     A 1px line scaled to the tab's width keeps the motion on `transform`. */
  .tab-indicator {
    position: absolute;
    left: 0;
    bottom: -1px;
    width: 1px;
    height: 2px;
    background: var(--leaf);
    transform: translateX(var(--x, 0px)) scaleX(var(--w, 0));
    transform-origin: left center;
    transition: transform var(--t-med) var(--ease-out);
  }
  .tab {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 9px 12px 10px;
    margin-bottom: -1px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: var(--fs-sm);
    white-space: nowrap;
    cursor: pointer;
    transition:
      color var(--t-fast) var(--ease-out),
      border-color var(--t-fast) var(--ease-out);
  }
  .tab:hover {
    color: var(--text);
  }
  .tab[aria-selected="true"] {
    color: var(--text);
  }
  .tab:focus-visible {
    outline: 2px solid var(--leaf);
    outline-offset: -2px;
    border-radius: var(--radius-sm);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 36px;
    min-width: 0;
    animation: panel-in 180ms var(--ease-out);
  }
  @keyframes panel-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  .save-hint {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--muted);
    font-size: var(--fs-xs);
    padding-bottom: 8px;
  }

  @media (max-width: 760px) {
    .settings {
      padding: 24px 18px 40px;
    }
  }

  /* Every tab stays visible: icons go first, then padding. */
  @container settings (max-width: 720px) {
    .tab :global(svg) {
      display: none;
    }
    .tab {
      padding: 9px 9px 10px;
    }
  }

  @container settings (max-width: 560px) {
    .settings-inner {
      gap: 24px;
    }
    .panel {
      gap: 28px;
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

  /* Type style specimens. Each card carries its own data-type, so the
     [data-type] blocks in app.css give it that style's fonts while the rest
     of the app keeps the active one. */
  .specimens {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    width: 100%;
  }
  @container settings (min-width: 700px) {
    .specimens {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }
  .specimen {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    min-width: 0;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-family: var(--font);
    font-variation-settings: var(--font-ui-settings);
    text-align: left;
    cursor: pointer;
    transition:
      transform var(--t-fast) var(--ease-out),
      border-color var(--t-fast) var(--ease-out),
      background var(--t-fast) var(--ease-out);
  }
  .specimen:hover {
    transform: translateY(-2px);
    border-color: var(--border-hover);
  }
  .specimen[aria-checked="true"] {
    /* 1.5px, without the 1px of layout shift a thicker border would cause */
    border-width: 1.5px;
    border-color: var(--text);
    padding: calc(12px - 0.5px) calc(14px - 0.5px);
  }
  .specimen:focus-visible {
    outline: 2px solid var(--leaf);
    outline-offset: 2px;
  }
  .specimen-aa {
    font-family: var(--font-display);
    font-variation-settings: var(--font-display-settings);
    font-weight: var(--display-weight);
    font-size: calc(var(--fs-3xl) * var(--display-size-adjust));
    letter-spacing: var(--display-tracking);
    line-height: var(--display-leading);
    color: var(--text);
  }
  .specimen-name {
    margin-top: 2px;
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .specimen-sample {
    max-width: 100%;
    font-family: var(--font);
    font-size: var(--fs-sm);
    line-height: 1.35;
    color: var(--text-soft);
    overflow-wrap: anywhere;
  }
  .specimen-mono {
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .specimen-blurb {
    margin-top: 3px;
    font-size: var(--fs-xs);
    line-height: 1.4;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .specimen-check {
    position: absolute;
    top: 10px;
    right: 10px;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 9999px;
    background: var(--text);
    color: var(--surface);
  }

  .size-control {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .size-btn {
    width: auto;
    padding: 0 9px;
    font-size: var(--fs-xs);
  }
</style>
