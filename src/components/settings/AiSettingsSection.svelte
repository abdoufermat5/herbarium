<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import { errorMessage, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { AiProvider, AiSettings } from "../../lib/types";
  import Select, { type SelectOption } from "../Select.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";

  const DEFAULT_MODEL = "claude-opus-5-5";

  let settings = $state<AiSettings | null>(null);
  let modelDraft = $state(DEFAULT_MODEL);
  let keyDraft = $state("");
  let saving = $state(false);

  const providers = $derived<SelectOption<AiProvider>[]>([
    { value: "anthropic", label: t("ai.providerApi"), hint: t("ai.providerApiHint") },
    { value: "claude-code", label: t("ai.providerCli"), hint: t("ai.providerCliHint") },
  ]);

  async function save(patch: { provider?: AiProvider; model?: string; key?: string }) {
    if (!settings || saving) return;
    saving = true;
    try {
      settings = await api.setAiSettings(
        patch.provider ?? settings.provider,
        patch.model ?? settings.model,
        patch.key,
      );
      modelDraft = settings.model;
      if (patch.key !== undefined) {
        keyDraft = "";
        toast(patch.key ? t("ai.keySaved") : t("ai.keyRemoved"), "success");
      }
    } catch (e) {
      toast(`${t("ai.saveFailed")}: ${errorMessage(e)}`, "error");
      modelDraft = settings?.model ?? DEFAULT_MODEL;
    } finally {
      saving = false;
    }
  }

  function commitModel() {
    const next = modelDraft.trim();
    if (next && next !== settings?.model) void save({ model: next });
  }

  onMount(async () => {
    try {
      settings = await api.aiSettings();
      modelDraft = settings.model;
    } catch (e) {
      console.error(e);
    }
  });
</script>

<SettingsSection id="settings-ai" title={t("ai.title")} note={t("ai.note")}>
  <SettingRow title={t("ai.provider")} hint={t("ai.providerHint")}>
    <Select
      value={settings?.provider ?? "anthropic"}
      options={providers}
      ariaLabel={t("ai.provider")}
      disabled={!settings || saving}
      onchange={(v) => void save({ provider: v })}
    />
  </SettingRow>
  {#if settings?.provider !== "claude-code"}
    <SettingRow
      title={t("ai.key")}
      hint={settings?.hasKey ? t("ai.keyIsSet") : t("ai.keyHint")}
    >
      <form class="inline" onsubmit={(e) => (e.preventDefault(), keyDraft.trim() && void save({ key: keyDraft }))}>
        <input
          type="password"
          bind:value={keyDraft}
          placeholder={settings?.hasKey ? "••••••••" : "sk-ant-…"}
          aria-label={t("ai.key")}
          autocomplete="off"
          spellcheck="false"
          disabled={!settings || saving}
        />
        <button class="btn btn-sm" type="submit" disabled={!keyDraft.trim() || saving}>{t("ai.saveKey")}</button>
        {#if settings?.hasKey}
          <button class="btn btn-sm btn-ghost" type="button" onclick={() => void save({ key: "" })} disabled={saving}>
            {t("ai.removeKey")}
          </button>
        {/if}
      </form>
    </SettingRow>
  {/if}
  <SettingRow title={t("ai.model")} hint={t("ai.modelHint")}>
    <div class="inline">
      <input
        type="text"
        bind:value={modelDraft}
        onchange={commitModel}
        placeholder={DEFAULT_MODEL}
        aria-label={t("ai.model")}
        spellcheck="false"
        disabled={!settings || saving}
      />
      {#if settings && settings.model !== DEFAULT_MODEL}
        <button class="btn btn-sm btn-ghost" onclick={() => void save({ model: DEFAULT_MODEL })} disabled={saving}>
          {t("capture.reset")}
        </button>
      {/if}
    </div>
  </SettingRow>
</SettingsSection>

<style>
  .inline {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .inline input {
    width: 220px;
    font-family: var(--mono);
    font-size: var(--fs-sm);
  }
</style>
