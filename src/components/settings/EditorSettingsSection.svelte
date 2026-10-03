<script lang="ts">
  import { t } from "../../lib/i18n.svelte";
  import {
    prefs,
    setPref,
    EDITOR_FONT_SIZES,
    EDITOR_LINE_HEIGHTS,
    EDITOR_TAB_SIZES,
    type EditorFont,
    type PreviewLayout,
  } from "../../lib/prefs.svelte";
  import Select, { type SelectOption } from "../Select.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";
  import Switch from "./Switch.svelte";

  // Real tabs, so the sample shows the effect of the tab width.
  const SAMPLE = [
    "<section class=\"card\">",
    "\t<h1>Hello, Herbarium</h1>",
    "\t<p>Long lines only wrap when wrapping is on, so this sentence is deliberately a little too long.</p>",
    "</section>",
  ].join("\n");

  const fonts = $derived<SelectOption<EditorFont>[]>([
    { value: "app", label: t("settings.editor.fontApp") },
    { value: "system", label: t("settings.editor.fontSystem") },
  ]);
  const sizes = $derived<SelectOption<number>[]>(
    EDITOR_FONT_SIZES.map((n) => ({ value: n, label: t("settings.editor.px", { n }) })),
  );
  const spacings = $derived<SelectOption<number>[]>(
    EDITOR_LINE_HEIGHTS.map((n, i) => ({
      value: n,
      label: t((["settings.editor.lhCompact", "settings.editor.lhNormal", "settings.editor.lhRelaxed"] as const)[i]),
    })),
  );
  const tabSizes = $derived<SelectOption<number>[]>(
    EDITOR_TAB_SIZES.map((n) => ({ value: n, label: t("settings.editor.spaces", { n }) })),
  );
  const previews = $derived<SelectOption<PreviewLayout>[]>([
    { value: "right", label: t("settings.editor.previewRight") },
    { value: "below", label: t("settings.editor.previewBelow") },
    { value: "off", label: t("settings.editor.previewOff") },
  ]);
</script>

<SettingsSection id="settings-editor" title={t("settings.editor")}>
  <div class="sample-row">
    <pre
      class="sample"
      aria-hidden="true"
      style:font-family={prefs.editorFont === "system"
        ? "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
        : "var(--mono)"}
      style:font-size="{prefs.editorFontSize}px"
      style:line-height={prefs.editorLineHeight}
      style:tab-size={prefs.editorTabSize}
      style:white-space={prefs.editorWrap ? "pre-wrap" : "pre"}>{SAMPLE}</pre>
  </div>

  <SettingRow title={t("settings.editor.font")} hint={t("settings.editor.fontHint")}>
    <Select
      fill
      size="md"
      align="right"
      value={prefs.editorFont}
      ariaLabel={t("settings.editor.font")}
      options={fonts}
      onchange={(v) => setPref("editorFont", v)}
    />
  </SettingRow>
  <SettingRow title={t("settings.editor.size")} hint={t("settings.editor.sizeHint")}>
    <Select
      fill
      size="md"
      align="right"
      value={prefs.editorFontSize}
      ariaLabel={t("settings.editor.size")}
      options={sizes}
      onchange={(v) => setPref("editorFontSize", v)}
    />
  </SettingRow>
  <SettingRow title={t("settings.editor.spacing")} hint={t("settings.editor.spacingHint")}>
    <Select
      fill
      size="md"
      align="right"
      value={prefs.editorLineHeight}
      ariaLabel={t("settings.editor.spacing")}
      options={spacings}
      onchange={(v) => setPref("editorLineHeight", v)}
    />
  </SettingRow>
  <SettingRow title={t("settings.editor.wrap")} hint={t("settings.editor.wrapHint")}>
    <Switch
      checked={prefs.editorWrap}
      label={t("settings.editor.wrap")}
      onchange={(v) => setPref("editorWrap", v)}
    />
  </SettingRow>
  <SettingRow title={t("settings.editor.tabSize")} hint={t("settings.editor.tabSizeHint")}>
    <Select
      fill
      size="md"
      align="right"
      value={prefs.editorTabSize}
      ariaLabel={t("settings.editor.tabSize")}
      options={tabSizes}
      onchange={(v) => setPref("editorTabSize", v)}
    />
  </SettingRow>
  <SettingRow title={t("settings.editor.tabIndents")} hint={t("settings.editor.tabIndentsHint")}>
    <Switch
      checked={prefs.editorTabIndents}
      label={t("settings.editor.tabIndents")}
      onchange={(v) => setPref("editorTabIndents", v)}
    />
  </SettingRow>
  <SettingRow title={t("settings.editor.spellcheck")} hint={t("settings.editor.spellcheckHint")}>
    <Switch
      checked={prefs.editorSpellcheck}
      label={t("settings.editor.spellcheck")}
      onchange={(v) => setPref("editorSpellcheck", v)}
    />
  </SettingRow>
  <SettingRow title={t("settings.editor.preview")} hint={t("settings.editor.previewHint")}>
    <Select
      fill
      size="md"
      align="right"
      value={prefs.editorPreview}
      ariaLabel={t("settings.editor.preview")}
      options={previews}
      onchange={(v) => setPref("editorPreview", v)}
    />
  </SettingRow>
</SettingsSection>

<style>
  .sample-row {
    padding: 20px 28px;
  }
  .sample {
    margin: 0;
    padding: 14px 16px;
    max-width: 100%;
    overflow-x: auto;
    background: var(--raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
    overflow-wrap: anywhere;
  }

  @container settings (max-width: 560px) {
    .sample-row {
      padding: 18px 20px;
    }
  }
</style>
