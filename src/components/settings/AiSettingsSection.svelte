<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../../lib/api";
  import { errorMessage, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { AiModel, AiProvider, AiSettings, ClaudeCodeStatus } from "../../lib/types";
  import Icon from "../../lib/Icon.svelte";
  import Select, { type SelectOption } from "../Select.svelte";
  import CopyCommand from "./CopyCommand.svelte";
  import { forgetAiStatus } from "../ai/AiPanel.svelte";
  import SettingsSection from "./SettingsSection.svelte";

  /** The Select value that reveals a field to type a model name. */
  const OTHER = "\u0000other";
  /** The services reached with an API key, in the order they are offered. */
  const KEY_SERVICES = ["anthropic", "openai", "gemini", "deepseek", "mistral", "openrouter", "custom"];
  const isWindows = typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent);
  const isMac = typeof navigator !== "undefined" && /mac os/i.test(navigator.userAgent);

  type Way = "claude-code" | "ollama" | "key";

  let settings = $state<AiSettings | null>(null);
  let urlDraft = $state("");
  let keyDraft = $state("");
  let otherDraft = $state("");
  let typingOther = $state(false);
  let saving = $state(false);
  let showAddress = $state(false);

  /** What the service said: its models, or why they could not be listed. */
  let models = $state<AiModel[]>([]);
  let recommended = $state<string | null>(null);
  let listing = $state(false);
  let listError = $state<string | null>(null);
  let listSeq = 0;

  let claude = $state<ClaudeCodeStatus | null>(null);
  let checkingClaude = $state(false);

  const current = $derived(settings?.providers.find((p) => p.id === settings?.provider) ?? null);
  const way = $derived<Way>(
    settings?.provider === "claude-code" ? "claude-code" : settings?.provider === "ollama" ? "ollama" : "key",
  );
  const hasKey = $derived(!!settings && settings.keys.includes(settings.provider));
  const ready = $derived.by(() => {
    if (!current) return false;
    if (current.id === "claude-code") return !!claude?.version;
    if (current.id === "custom") return !!settings?.baseUrl;
    return !current.needsKey || hasKey;
  });
  /** Ready and the service answered: remixing will work. */
  const working = $derived(
    ready && (current?.id === "claude-code" || (!listing && !listError && models.length > 0)),
  );

  const nameOf = (id: string) => models.find((m) => m.id === id)?.name ?? id;
  const keyServices = $derived((settings?.providers ?? []).filter((p) => KEY_SERVICES.includes(p.id)));
  const siteOf = (url: string | null) => {
    try {
      return url ? new URL(url).hostname.replace(/^www\./, "") : "";
    } catch {
      return "";
    }
  };

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

  async function checkClaude() {
    checkingClaude = true;
    try {
      claude = await api.claudeCodeStatus();
      forgetAiStatus();
      if (claude.version) await loadModels(false);
    } catch (e) {
      claude = { path: null, version: null, error: errorMessage(e), chosen: false };
    } finally {
      checkingClaude = false;
    }
  }

  async function chooseClaude() {
    const picked = await openDialog({ title: t("ai.cc.chooseTitle"), multiple: false, directory: false });
    if (typeof picked !== "string") return;
    checkingClaude = true;
    try {
      claude = await api.setClaudeCodePath(picked);
      forgetAiStatus();
      if (claude.version) {
        toast(t("ai.cc.found", { version: claude.version.replace(/\s*\(Claude Code\)\s*$/i, "") }), "success");
        await loadModels(false);
      }
    } catch (e) {
      toast(`${t("ai.cc.notClaude")}: ${errorMessage(e)}`, "error");
    } finally {
      checkingClaude = false;
    }
  }

  async function forgetClaudePath() {
    checkingClaude = true;
    try {
      claude = await api.setClaudeCodePath(null);
      forgetAiStatus();
    } finally {
      checkingClaude = false;
    }
  }

  function sync(next: AiSettings) {
    forgetAiStatus();
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
      if (switching && provider === "claude-code") await checkClaude();
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

  /** Pick one of the three ways; "with a key" keeps a key service already chosen. */
  function pickWay(next: Way) {
    if (next === way || !settings) return;
    if (next === "key") {
      const withKey = KEY_SERVICES.find((id) => settings!.keys.includes(id)) ?? "anthropic";
      void save({ provider: withKey });
    } else {
      void save({ provider: next });
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

  /** Ollama is installed and running when it answers with its model list. */
  const ollamaEmpty = $derived(way === "ollama" && !listError && !listing && models.length === 0 && ready);

  onMount(async () => {
    try {
      sync(await api.aiSettings());
      if (settings?.provider === "claude-code") await checkClaude();
      await loadModels(false);
    } catch (e) {
      console.error(e);
    }
  });
</script>

<SettingsSection id="settings-ai" title={t("ai.title")} note={t("ai.note")}>
  <div class="ways-wrap">
    <p class="ask">{t("ai.ask")}</p>
    <div class="ways" role="radiogroup" aria-label={t("ai.ask")}>
      {#each [
        { id: "claude-code", icon: "sparkle", title: t("ai.way.cc"), text: t("ai.way.ccText"), badge: t("ai.way.ccBadge") },
        { id: "ollama", icon: "leaf", title: t("ai.way.local"), text: t("ai.way.localText"), badge: t("ai.way.localBadge") },
        { id: "key", icon: "settings", title: t("ai.way.key"), text: t("ai.way.keyText"), badge: t("ai.way.keyBadge") },
      ] as w (w.id)}
        <button
          type="button"
          role="radio"
          aria-checked={way === w.id}
          class="way"
          class:on={way === w.id}
          disabled={!settings || saving}
          onclick={() => pickWay(w.id as Way)}
        >
          <span class="badge">{w.badge}</span>
          <strong>{w.title}</strong>
          <span class="text">{w.text}</span>
        </button>
      {/each}
    </div>
  </div>

  {#if settings && current}
    <div class="setup">
      {#if way === "claude-code"}
        <!-- Claude Code: found, or how to get it. -->
        {#if checkingClaude && !claude}
          <p class="state muted"><span class="spinner small"></span>{t("ai.cc.looking")}</p>
        {:else if claude?.version}
          <p class="state good"><Icon name="circle-check" size={15} />{t("ai.cc.ready", { version: claude.version.replace(/\s*\(Claude Code\)\s*$/i, "") })}</p>
          <p class="small muted mono-path" title={claude.path ?? ""}>{claude.path}</p>
          <p class="small muted">{t("ai.cc.signInHint")}</p>
          <div class="actions">
            <button class="btn btn-sm" onclick={checkClaude} disabled={checkingClaude}>
              <Icon name="refresh-cw" size={12} />{t("ai.checkAgain")}
            </button>
            {#if claude.chosen}
              <button class="btn btn-sm btn-ghost" onclick={forgetClaudePath} disabled={checkingClaude}>{t("ai.cc.auto")}</button>
            {/if}
          </div>
        {:else}
          <p class="state bad"><Icon name="info" size={15} />{claude?.path ? (claude.error ?? t("ai.cc.missing")) : t("ai.cc.missing")}</p>
          <ol class="steps">
            <li>
              <strong>{t("ai.cc.step1")}</strong>
              <span class="small muted">{isWindows ? t("ai.cc.step1Windows") : t("ai.cc.step1Unix")}</span>
              <CopyCommand
                text={isWindows ? "irm https://claude.ai/install.ps1 | iex" : "curl -fsSL https://claude.ai/install.sh | bash"}
              />
              <button class="link" onclick={() => void api.openExternal("https://code.claude.com/docs/en/setup")}>
                {t("ai.cc.otherWays")} <Icon name="external-link" size={11} />
              </button>
            </li>
            <li>
              <strong>{t("ai.cc.step2")}</strong>
              <span class="small muted">{t("ai.cc.step2Text")}</span>
              <CopyCommand text="claude" />
            </li>
            <li>
              <strong>{t("ai.cc.step3")}</strong>
              <div class="actions">
                <button class="btn btn-sm btn-primary" onclick={checkClaude} disabled={checkingClaude}>
                  {#if checkingClaude}<span class="spinner small"></span>{:else}<Icon name="refresh-cw" size={12} />{/if}
                  {t("ai.checkAgain")}
                </button>
                <button class="btn btn-sm btn-ghost" onclick={chooseClaude} disabled={checkingClaude}>
                  <Icon name="folder-open" size={12} />{t("ai.cc.choose")}
                </button>
              </div>
            </li>
          </ol>
        {/if}
      {:else if way === "ollama"}
        <!-- Ollama: running, empty, or how to get it. -->
        {#if listing}
          <p class="state muted"><span class="spinner small"></span>{t("ai.ol.looking")}</p>
        {:else if working}
          <p class="state good">
            <Icon name="circle-check" size={15} />{t("ai.ol.ready", { count: models.length })}
          </p>
        {:else}
          <p class="state bad"><Icon name="info" size={15} />{ollamaEmpty ? t("ai.ol.noModel") : t("ai.ol.missing")}</p>
          <ol class="steps">
            {#if !ollamaEmpty}
              <li>
                <strong>{t("ai.ol.step1")}</strong>
                {#if isWindows || isMac}
                  <button class="btn btn-sm" onclick={() => void api.openExternal("https://ollama.com/download")}>
                    <Icon name="external-link" size={12} />{t("ai.ol.download")}
                  </button>
                {:else}
                  <span class="small muted">{t("ai.ol.step1Linux")}</span>
                  <CopyCommand text="curl -fsSL https://ollama.com/install.sh | sh" />
                {/if}
              </li>
            {/if}
            <li>
              <strong>{t("ai.ol.step2")}</strong>
              <span class="small muted">{t("ai.ol.step2Text")}</span>
              <CopyCommand text="ollama pull llama3.2" />
            </li>
            <li>
              <strong>{t("ai.cc.step3")}</strong>
              <div class="actions">
                <button class="btn btn-sm btn-primary" onclick={() => void loadModels(true)}>
                  <Icon name="refresh-cw" size={12} />{t("ai.checkAgain")}
                </button>
              </div>
            </li>
          </ol>
        {/if}
        <button class="link" onclick={() => (showAddress = !showAddress)}>{t("ai.ol.elsewhere")}</button>
        {#if showAddress || settings.baseUrl}
          <input
            class="field mono wide"
            type="url"
            bind:value={urlDraft}
            onchange={commitUrl}
            placeholder={current.baseUrl ?? ""}
            aria-label={t("ai.baseUrl")}
            spellcheck="false"
            disabled={saving}
          />
        {/if}
      {:else}
        <!-- A service reached with an API key. -->
        <div class="services" role="radiogroup" aria-label={t("ai.provider")}>
          {#each keyServices as p (p.id)}
            <button
              type="button"
              role="radio"
              aria-checked={settings.provider === p.id}
              class="service"
              class:on={settings.provider === p.id}
              disabled={saving}
              onclick={() => void save({ provider: p.id })}
            >
              {p.id === "custom" ? t("ai.otherService") : p.label.replace(/ \(.*\)$/, "")}
              {#if settings.keys.includes(p.id)}<Icon name="check" size={11} />{/if}
            </button>
          {/each}
        </div>

        {#if current.id === "custom"}
          <label class="field-label" for="ai-url">{t("ai.baseUrl")}</label>
          <input
            id="ai-url"
            class="field mono wide"
            type="url"
            bind:value={urlDraft}
            onchange={commitUrl}
            placeholder="https://…/v1"
            spellcheck="false"
            disabled={saving}
          />
          <span class="small muted">{t("ai.baseUrlHint")}</span>
        {/if}

        <ol class="steps">
          {#if current.keyUrl}
            <li>
              <strong>{t("ai.key.step1", { site: siteOf(current.keyUrl) })}</strong>
              <span class="small muted">{t("ai.key.step1Text")}</span>
              <button class="btn btn-sm" onclick={() => void api.openExternal(current.keyUrl!)}>
                <Icon name="external-link" size={12} />{t("ai.key.open", { site: siteOf(current.keyUrl) })}
              </button>
            </li>
          {/if}
          <li>
            <strong>{current.id === "custom" ? t("ai.key.optional") : t("ai.key.step2")}</strong>
            <span class="small muted">{hasKey ? t("ai.keyIsSet") : t("ai.key.step2Text")}</span>
            <form class="inline" onsubmit={(e) => (e.preventDefault(), keyDraft.trim() && void save({ key: keyDraft }))}>
              <input
                class="field mono"
                type="password"
                bind:value={keyDraft}
                placeholder={hasKey ? "••••••••" : t("ai.keyPlaceholder")}
                aria-label={t("ai.keyFor", { provider: current.label })}
                autocomplete="off"
                spellcheck="false"
                disabled={saving}
              />
              <button class="btn btn-sm btn-primary" type="submit" disabled={!keyDraft.trim() || saving}>
                {saving && keyDraft ? t("ai.checking") : t("ai.saveKey")}
              </button>
              {#if hasKey}
                <button class="btn btn-sm btn-ghost" type="button" onclick={() => void save({ key: "" })} disabled={saving}>
                  {t("ai.removeKey")}
                </button>
              {/if}
            </form>
          </li>
        </ol>

        <div class="status" role="status" aria-live="polite">
          {#if !ready}
            <span class="muted"><Icon name="info" size={13} />{current.id === "custom" ? t("ai.statusNeedsUrl") : t("ai.statusNeedsKey", { provider: current.label })}</span>
          {:else if listing}
            <span class="muted"><span class="spinner small"></span>{t("ai.statusChecking", { provider: current.label })}</span>
          {:else if listError}
            <span class="bad"><Icon name="x" size={13} />{listError}</span>
            <button class="btn btn-xs btn-ghost" onclick={() => void loadModels(true)}>{t("ai.retry")}</button>
          {:else if models.length > 0}
            <span class="good"><Icon name="circle-check" size={13} />{t("ai.statusReady", { provider: current.label, count: models.length })}</span>
          {/if}
        </div>
      {/if}

      {#if working && models.length > 0}
        <div class="model-row">
          <span class="field-label">{t("ai.model")}</span>
          <div class="model">
            <Select
              value={typingOther ? OTHER : (settings.model ?? "")}
              options={modelOptions}
              ariaLabel={t("ai.model")}
              disabled={saving}
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
          <span class="small muted">{t("ai.modelHint")}</span>
        </div>
      {/if}
    </div>
  {/if}
</SettingsSection>

<style>
  .ways-wrap,
  .setup {
    padding: 20px 28px;
  }
  .ask {
    margin: 0 0 12px;
    font-weight: 500;
  }
  .ways {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
    gap: 10px;
  }
  .way {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 14px 14px 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .way:hover:not(:disabled) {
    border-color: var(--border-hover);
  }
  .way.on {
    border-color: var(--leaf);
    box-shadow: 0 0 0 1px var(--leaf);
    background: var(--leaf-soft);
  }
  .way strong {
    font-size: var(--fs-base);
    font-weight: 600;
  }
  .way .text {
    color: var(--muted);
    font-size: var(--fs-sm);
    line-height: 1.4;
  }
  .badge {
    font-size: var(--fs-2xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--leaf);
  }

  .setup {
    display: flex;
    flex-direction: column;
    gap: 12px;
    align-items: flex-start;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-weight: 500;
  }
  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
    counter-reset: step;
    display: flex;
    flex-direction: column;
    gap: 14px;
    width: 100%;
  }
  .steps > li {
    counter-increment: step;
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding-left: 34px;
  }
  .steps > li::before {
    content: counter(step);
    position: absolute;
    left: 0;
    top: -1px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--text);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .steps :global(.cmd) {
    width: min(460px, 100%);
  }
  .services {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .service {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-soft);
    font: inherit;
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .service.on {
    border-color: var(--leaf);
    color: var(--text);
    background: var(--leaf-soft);
    font-weight: 500;
  }
  .actions,
  .inline {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .actions .btn,
  .inline .btn,
  .steps .btn,
  .status .btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .field {
    width: 260px;
    max-width: 100%;
  }
  .field.wide {
    width: min(420px, 100%);
  }
  .field-label {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .mono {
    font-family: var(--mono);
    font-size: var(--fs-sm);
  }
  .mono-path {
    margin: -6px 0 0;
    font-family: var(--mono);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    font-size: var(--fs-sm);
    margin: 0;
  }
  .link {
    border: 0;
    padding: 0;
    background: none;
    color: var(--text-soft);
    font: inherit;
    font-size: var(--fs-sm);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
    font-size: var(--fs-sm);
  }
  .status span {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .model-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
    width: 100%;
  }
  .model {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
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
