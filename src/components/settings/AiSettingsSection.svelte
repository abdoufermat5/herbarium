<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import { errorMessage, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { AiModel, AiProvider, AiSettings } from "../../lib/types";
  import Icon from "../../lib/Icon.svelte";
  import Select, { type SelectOption } from "../Select.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";

  /** The Select value that reveals a field to type a model name. */
  const OTHER = "\u0000other";

  let settings = $state<AiSettings | null>(null);
  let urlDraft = $state("");
  let keyDraft = $state("");
  let otherDraft = $state("");
  let typingOther = $state(false);
  let saving = $state(false);

  /** What the service said: its models, or why they could not be listed. */
  let models = $state<AiModel[]>([]);
  let recommended = $state<string | null>(null);
  let listing = $state(false);
  let listError = $state<string | null>(null);
  let listSeq = 0;

  const current = $derived(settings?.providers.find((p) => p.id === settings?.provider) ?? null);
  const hasKey = $derived(!!settings && settings.keys.includes(settings.provider));
  /** Custom services need an address; Ollama may live on another machine. */
  const showUrl = $derived(!!current && (current.id === "custom" || current.id === "ollama"));
  /** Custom services may or may not want a key. */
  const showKey = $derived(!!current && (current.needsKey || current.id === "custom"));
  const ready = $derived(!!current && (!current.needsKey || hasKey) && (current.id !== "custom" || !!settings?.baseUrl));

  const nameOf = (id: string) => models.find((m) => m.id === id)?.name ?? id;

  const providerOptions = $derived<SelectOption<AiProvider>[]>(
    (settings?.providers ?? []).map((p) => ({
      value: p.id,
      label: p.label,
      hint:
        p.id === "claude-code"
          ? t("ai.providerCliHint")
          : p.id === "ollama"
            ? t("ai.providerLocalHint")
            : settings?.keys.includes(p.id)
              ? t("ai.keySavedShort")
              : undefined,
    })),
  );

  /** "Automatic" first, then what the service offers, then a typed name. */
  const modelOptions = $derived.by<SelectOption<string>[]>(() => {
    const auto = recommended ? t("ai.autoWith", { model: nameOf(recommended) }) : t("ai.auto");
    const list: SelectOption<string>[] = [{ value: "", label: auto, hint: t("ai.autoHint") }];
    for (const m of models) list.push({ value: m.id, label: m.name, hint: m.name !== m.id ? m.id : undefined });
    const chosen = settings?.model ?? "";
    if (chosen && !models.some((m) => m.id === chosen)) list.push({ value: chosen, label: chosen });
    list.push({ value: OTHER, label: t("ai.modelOther") });
    return list;
  });

  async function loadModels(refresh: boolean) {
    if (!settings || !ready) {
      models = [];
      recommended = null;
      listError = null;
      return;
    }
    const seq = ++listSeq;
    const provider = settings.provider;
    listing = true;
    listError = null;
    try {
      const res = await api.aiModels(provider, refresh);
      if (seq !== listSeq) return;
      models = res.models;
      recommended = res.recommended;
    } catch (e) {
      if (seq !== listSeq) return;
      models = [];
      recommended = null;
      listError = errorMessage(e);
    } finally {
      if (seq === listSeq) listing = false;
    }
  }

  function sync(next: AiSettings) {
    settings = next;
    urlDraft = next.baseUrl ?? "";
  }

  async function save(patch: { provider?: AiProvider; model?: string; baseUrl?: string | null; key?: string }) {
    if (!settings || saving) return;
    saving = true;
    try {
      const provider = patch.provider ?? settings.provider;
      const switching = provider !== settings.provider;
      // A new service starts on Automatic, at its usual address.
      const model = patch.model ?? (switching ? "" : settings.model);
      const baseUrl = patch.baseUrl !== undefined ? patch.baseUrl : switching ? null : settings.baseUrl;
      sync(await api.setAiSettings(provider, model, baseUrl, patch.key));
      if (patch.key !== undefined) keyDraft = "";
      if (patch.model === undefined) {
        typingOther = false;
        // A new service, key or address: ask the service again. This also
        // tells the user straight away whether the key works.
        await loadModels(true);
        if (patch.key && !listError) toast(t("ai.keyWorks"), "success");
        else if (patch.key === "") toast(t("ai.keyRemoved"), "success");
      }
    } catch (e) {
      toast(`${t("ai.saveFailed")}: ${errorMessage(e)}`, "error");
      if (settings) sync(settings);
    } finally {
      saving = false;
    }
  }

  function pickModel(value: string) {
    if (value === OTHER) {
      typingOther = true;
      otherDraft = settings?.model ?? "";
      return;
    }
    typingOther = false;
    void save({ model: value });
  }

  function commitOther() {
    const next = otherDraft.trim();
    if (next) void save({ model: next });
  }

  function commitUrl() {
    const next = urlDraft.trim();
    if (next !== (settings?.baseUrl ?? "")) void save({ baseUrl: next || null });
  }

  onMount(async () => {
    try {
      sync(await api.aiSettings());
      await loadModels(false);
    } catch (e) {
      console.error(e);
    }
  });
</script>

<SettingsSection id="settings-ai" title={t("ai.title")} note={t("ai.note")}>
  <SettingRow title={t("ai.provider")} hint={t("ai.providerHint")}>
    <Select
      value={settings?.provider ?? "anthropic"}
      options={providerOptions}
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
        class="field mono"
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
          class="field mono"
          type="password"
          bind:value={keyDraft}
          placeholder={hasKey ? "••••••••" : t("ai.keyPlaceholder")}
          aria-label={t("ai.keyFor", { provider: current.label })}
          autocomplete="off"
          spellcheck="false"
          disabled={!settings || saving}
        />
        <button class="btn btn-sm" type="submit" disabled={!keyDraft.trim() || saving}>
          {saving && keyDraft ? t("ai.checking") : t("ai.saveKey")}
        </button>
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
    <div class="model">
      <Select
        value={typingOther ? OTHER : (settings?.model ?? "")}
        options={modelOptions}
        ariaLabel={t("ai.model")}
        disabled={!settings || saving}
        onchange={pickModel}
      />
      {#if typingOther}
        <form class="inline" onsubmit={(e) => (e.preventDefault(), commitOther())}>
          <input
            class="field mono"
            type="text"
            bind:value={otherDraft}
            placeholder={t("ai.modelPlaceholder")}
            aria-label={t("ai.modelOther")}
            spellcheck="false"
          />
          <button class="btn btn-sm" type="submit" disabled={!otherDraft.trim() || saving}>{t("ai.useModel")}</button>
        </form>
      {/if}
    </div>
  </SettingRow>

  <div class="status" role="status" aria-live="polite">
    {#if !current}
      <span></span>
    {:else if !ready}
      <span class="muted"><Icon name="info" size={13} />{current.id === "custom" ? t("ai.statusNeedsUrl") : t("ai.statusNeedsKey", { provider: current.label })}</span>
    {:else if listing}
      <span class="muted"><span class="spinner small"></span>{t("ai.statusChecking", { provider: current.label })}</span>
    {:else if listError}
      <span class="bad"><Icon name="x" size={13} />{listError}</span>
      <button class="btn btn-xs btn-ghost" onclick={() => void loadModels(true)}>{t("ai.retry")}</button>
    {:else if models.length > 0}
      <span class="good"><Icon name="circle-check" size={13} />{t("ai.statusReady", { provider: current.label, count: models.length })}</span>
      <button class="btn btn-xs btn-ghost" onclick={() => void loadModels(true)} title={t("ai.refreshHint")}>
        <Icon name="refresh-cw" size={12} />{t("ai.refresh")}
      </button>
    {/if}
  </div>
</SettingsSection>

<style>
  .inline {
    display: flex;
    gap: 6px;
    align-items: center;
    justify-content: flex-end;
  }
  .model {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
  }
  .field {
    width: 220px;
    min-width: 0;
    flex: 0 1 auto;
  }
  .mono {
    font-family: var(--mono);
    font-size: var(--fs-sm);
  }
  .inline .btn,
  .status .btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 8px 28px 12px;
    font-size: var(--fs-sm);
  }
  .status span {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .muted {
    color: var(--muted);
  }
  .good {
    color: var(--ok, var(--leaf));
  }
  .bad {
    color: var(--danger);
  }
  .spinner.small {
    width: 12px;
    height: 12px;
  }
</style>
