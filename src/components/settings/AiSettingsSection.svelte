<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import { errorMessage, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { AiProvider, AiSettings } from "../../lib/types";
  import Icon from "../../lib/Icon.svelte";
  import Select, { type SelectOption } from "../Select.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";

  let settings = $state<AiSettings | null>(null);
  let modelDraft = $state("");
  let urlDraft = $state("");
  let keyDraft = $state("");
  let saving = $state(false);

  const current = $derived(settings?.providers.find((p) => p.id === settings?.provider) ?? null);
  const hasKey = $derived(!!settings && settings.keys.includes(settings.provider));
  /** Custom services need an address; Ollama may live on another machine. */
  const showUrl = $derived(!!current && (current.id === "custom" || current.id === "ollama"));
  /** Custom services may or may not want a key. */
  const showKey = $derived(!!current && (current.needsKey || current.id === "custom"));

  const providers = $derived<SelectOption<AiProvider>[]>(
    (settings?.providers ?? []).map((p) => ({
      value: p.id,
      label: p.label,
      hint: p.id === "claude-code" ? t("ai.providerCliHint") : settings?.keys.includes(p.id) ? t("ai.keySavedShort") : undefined,
    })),
  );

  function sync(next: AiSettings) {
    settings = next;
    modelDraft = next.model;
    urlDraft = next.baseUrl ?? "";
  }

  async function save(patch: { provider?: AiProvider; model?: string; baseUrl?: string | null; key?: string }) {
    if (!settings || saving) return;
    saving = true;
    try {
      const provider = patch.provider ?? settings.provider;
      // A new provider starts with its usual model and address.
      const switching = provider !== settings.provider;
      const info = settings.providers.find((p) => p.id === provider);
      const model = patch.model ?? (switching ? (info?.defaultModel ?? "") : settings.model);
      const baseUrl = patch.baseUrl !== undefined ? patch.baseUrl : switching ? null : settings.baseUrl;
      sync(await api.setAiSettings(provider, model, baseUrl, patch.key));
      if (patch.key !== undefined) {
        keyDraft = "";
        toast(patch.key ? t("ai.keySaved") : t("ai.keyRemoved"), "success");
      }
    } catch (e) {
      toast(`${t("ai.saveFailed")}: ${errorMessage(e)}`, "error");
      if (settings) sync(settings);
    } finally {
      saving = false;
    }
  }

  function commitModel() {
    const next = modelDraft.trim();
    if (next !== settings?.model) void save({ model: next });
  }

  function commitUrl() {
    const next = urlDraft.trim();
    if (next !== (settings?.baseUrl ?? "")) void save({ baseUrl: next || null });
  }

  onMount(async () => {
    try {
      sync(await api.aiSettings());
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
  {#if showUrl}
    <SettingRow
      title={t("ai.baseUrl")}
      hint={current?.baseUrl ? t("ai.baseUrlDefault", { url: current.baseUrl }) : t("ai.baseUrlHint")}
    >
      <input
        class="mono"
        type="url"
        bind:value={urlDraft}
        onchange={commitUrl}
        placeholder={current?.baseUrl ?? "https://…/v1"}
        aria-label={t("ai.baseUrl")}
        spellcheck="false"
        disabled={!settings || saving}
      />
    </SettingRow>
  {/if}
  {#if showKey && current}
    <SettingRow
      title={t("ai.keyFor", { provider: current.label })}
      hint={hasKey ? t("ai.keyIsSet") : current.needsKey ? t("ai.keyHint") : t("ai.keyOptional")}
    >
      <form class="inline" onsubmit={(e) => (e.preventDefault(), keyDraft.trim() && void save({ key: keyDraft }))}>
        <input
          class="mono"
          type="password"
          bind:value={keyDraft}
          placeholder={hasKey ? "••••••••" : t("ai.keyPlaceholder")}
          aria-label={t("ai.keyFor", { provider: current.label })}
          autocomplete="off"
          spellcheck="false"
          disabled={!settings || saving}
        />
        <button class="btn btn-sm" type="submit" disabled={!keyDraft.trim() || saving}>{t("ai.saveKey")}</button>
        {#if hasKey}
          <button class="btn btn-sm btn-ghost" type="button" onclick={() => void save({ key: "" })} disabled={saving}>
            {t("ai.removeKey")}
          </button>
        {:else if current.keyUrl}
          <button class="btn btn-sm btn-ghost" type="button" onclick={() => void api.openExternal(current.keyUrl!)}>
            <Icon name="external-link" size={12} />{t("ai.getKey")}
          </button>
        {/if}
      </form>
    </SettingRow>
  {/if}
  <SettingRow title={t("ai.model")} hint={t("ai.modelHint")}>
    <div class="inline">
      <input
        class="mono"
        type="text"
        bind:value={modelDraft}
        onchange={commitModel}
        placeholder={current?.defaultModel || t("ai.modelPlaceholder")}
        aria-label={t("ai.model")}
        spellcheck="false"
        disabled={!settings || saving}
      />
      {#if settings && current?.defaultModel && settings.model !== current.defaultModel}
        <button class="btn btn-sm btn-ghost" onclick={() => void save({ model: current.defaultModel })} disabled={saving}>
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
    flex-wrap: wrap;
  }
  .mono {
    width: 220px;
    max-width: 100%;
    font-family: var(--mono);
    font-size: var(--fs-sm);
  }
  .inline .btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
</style>
