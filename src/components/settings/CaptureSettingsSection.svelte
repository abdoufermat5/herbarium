<script lang="ts">
  import { api } from "../../lib/api";
  import { app, errorMessage, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";
  import Switch from "./Switch.svelte";

  const DEFAULT_SHORTCUT = "CommandOrControl+Alt+H";
  let saving = $state(false);
  let shortcutDraft = $state(app.config?.captureShortcut ?? "");

  /** How a shortcut reads on this system: Ctrl or ⌘ for CommandOrControl. */
  const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
  function pretty(shortcut: string | null | undefined): string {
    if (!shortcut) return t("capture.off");
    return shortcut
      .replace(/CommandOrControl|CmdOrCtrl/gi, isMac ? "⌘" : "Ctrl")
      .replace(/\bAlt\b/g, isMac ? "⌥" : "Alt")
      .replace(/\+/g, " + ");
  }

  async function update(patch: Partial<{ captureShortcut: string | null; watchDownloads: boolean; watchClipboard: boolean }>) {
    const cfg = app.config;
    if (!cfg || saving) return;
    saving = true;
    try {
      app.config = await api.setCapture({
        captureShortcut: cfg.captureShortcut,
        watchDownloads: cfg.watchDownloads,
        watchClipboard: cfg.watchClipboard,
        ...patch,
      });
      shortcutDraft = app.config.captureShortcut ?? "";
    } catch (e) {
      toast(`${t("capture.settingFailed")}: ${errorMessage(e)}`, "error");
      shortcutDraft = app.config?.captureShortcut ?? "";
    } finally {
      saving = false;
    }
  }

  function commitShortcut() {
    const next = shortcutDraft.trim();
    if (next === (app.config?.captureShortcut ?? "")) return;
    void update({ captureShortcut: next || null });
  }
</script>

<SettingsSection id="settings-capture" title={t("capture.title")} note={t("capture.note")}>
  <SettingRow title={t("capture.shortcut")} hint={t("capture.shortcutHint", { current: pretty(app.config?.captureShortcut) })}>
    <div class="shortcut">
      <input
        type="text"
        bind:value={shortcutDraft}
        onchange={commitShortcut}
        placeholder={DEFAULT_SHORTCUT}
        aria-label={t("capture.shortcut")}
        disabled={saving}
        spellcheck="false"
      />
      {#if (app.config?.captureShortcut ?? "") !== DEFAULT_SHORTCUT}
        <button class="btn btn-sm btn-ghost" onclick={() => void update({ captureShortcut: DEFAULT_SHORTCUT })} disabled={saving}>
          {t("capture.reset")}
        </button>
      {/if}
    </div>
  </SettingRow>
  <SettingRow title={t("capture.downloads")} hint={t("capture.downloadsHint")}>
    <Switch
      checked={app.config?.watchDownloads ?? true}
      disabled={saving || !app.config}
      label={t("capture.downloads")}
      onchange={(v) => void update({ watchDownloads: v })}
    />
  </SettingRow>
  <SettingRow title={t("capture.clipboard")} hint={t("capture.clipboardHint")}>
    <Switch
      checked={app.config?.watchClipboard ?? false}
      disabled={saving || !app.config}
      label={t("capture.clipboard")}
      onchange={(v) => void update({ watchClipboard: v })}
    />
  </SettingRow>
</SettingsSection>

<style>
  .shortcut {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .shortcut input {
    width: 220px;
    font-family: var(--mono);
    font-size: var(--fs-sm);
  }
</style>
