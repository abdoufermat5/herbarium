<script lang="ts" module>
  import type { AiSettings, ClaudeCodeStatus } from "../../lib/types";

  // What the AI service looked like last time: the panel opens on it at
  // once and checks again in the background.
  let cached: { settings: AiSettings; claude: ClaudeCodeStatus | null } | null = null;

  /** The AI settings changed: the next panel waits for the new ones. */
  export function forgetAiStatus() {
    cached = null;
  }
</script>

<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "../../lib/api";
  import { app, errorMessage, openSettings, toast, type AiStart } from "../../lib/state.svelte";
  import { confirmState } from "../../lib/confirm.svelte";
  import { folderPickerState } from "../../lib/folder-picker.svelte";
  import { i18n, t } from "../../lib/i18n.svelte";
  import type { RemixPreset } from "../../lib/types";
  import type { IconName } from "../../lib/icons";
  import Icon from "../../lib/Icon.svelte";
  import OrganizeFlow, { type OrganizeStep } from "./OrganizeFlow.svelte";

  /* The one place for everything AI: what it offers follows what is on
     screen (the page being read, and the library), so no feature needs a
     button of its own. Type what you want, or pick an action. */

  let { start }: { start: AiStart } = $props();

  type View = "home" | "translate" | "remixing" | "organize";
  interface Action {
    key: string;
    group: "page" | "library";
    icon: IconName;
    label: string;
    hint: string;
    /** Needs an AI service set up (importing a chat export does not). */
    needsAi: boolean;
    disabled?: boolean;
    run: () => void;
  }

  const PRESETS: { preset: RemixPreset; icon: IconName }[] = [
    { preset: "simplify", icon: "feather" },
    { preset: "deeper", icon: "lightbulb" },
    { preset: "quiz", icon: "question" },
    { preset: "cheatsheet", icon: "list-checks" },
    { preset: "modernize", icon: "wrench" },
  ];
  const LANGUAGES = ["en", "fr", "es", "de", "it", "pt", "ja", "zh"];

  // svelte-ignore state_referenced_locally
  let view = $state<View>(start === "organize" ? "organize" : "home");
  let q = $state("");
  let active = $state(-1);
  let error = $state<string | null>(null);
  let settings = $state<AiSettings | null>(cached?.settings ?? null);
  let claude = $state<ClaudeCodeStatus | null>(cached?.claude ?? null);
  let language = $state("");
  let organizeScope = $state<"unsorted" | "all">("unsorted");
  let organizeWishes = $state("");
  let organizeStep = $state<OrganizeStep>("options");
  let organizeAuto = $state(false);
  let remixing = $state<{ id: string; label: string } | null>(null);
  let chars = $state(0);
  let started = $state(0);
  let now = $state(0);
  let askEl: HTMLTextAreaElement | undefined = $state();
  let setupBtn: HTMLButtonElement | undefined = $state();
  let nudging = $state(false);
  let languageEl: HTMLInputElement | undefined = $state();
  let unlisten: UnlistenFn | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;
  let destroyed = false;
  const returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;

  /** The page in the reader, while it is still in the library. */
  const page = $derived(app.readId ? (app.library.find((p) => p.id === app.readId) ?? null) : null);
  const note = $derived(q.trim());
  const unsortedCount = $derived(app.library.filter((p) => !p.folder || p.folder.toLowerCase() === "inbox").length);

  const provider = $derived(settings?.providers.find((p) => p.id === settings?.provider) ?? null);
  const claudeMissing = $derived(settings?.provider === "claude-code" && !!claude && !claude.version);
  const needsKey = $derived(!!settings && !!provider?.needsKey && !settings.keys.includes(settings.provider));
  const needsUrl = $derived(settings?.provider === "custom" && !settings.baseUrl);
  const checking = $derived(!settings || (settings.provider === "claude-code" && !claude));
  const ready = $derived(!checking && !claudeMissing && !needsKey && !needsUrl);
  const setupText = $derived(
    claudeMissing
      ? t("remix.needsClaude")
      : needsUrl
        ? t("ai.needsUrl")
        : t("remix.needsKey", { provider: provider?.label ?? "" }),
  );
  const busy = $derived(view === "remixing" || (view === "organize" && (organizeStep === "working" || organizeStep === "applying")));
  const elapsed = $derived(remixing ? Math.max(0, Math.round((now - started) / 1000)) : 0);

  /** Language names in the app's language, for the translate shortcuts. */
  const languageNames = $derived.by(() => {
    try {
      const names = new Intl.DisplayNames([i18n.locale], { type: "language" });
      return LANGUAGES.map((code) => names.of(code) ?? code).map((n) => n.charAt(0).toUpperCase() + n.slice(1));
    } catch {
      return ["English", "French", "Spanish", "German", "Italian", "Portuguese", "Japanese", "Chinese"];
    }
  });

  const actions = $derived.by<Action[]>(() => {
    const list: Action[] = [];
    if (page) {
      if (note) {
        list.push({
          key: "custom",
          group: "page",
          icon: "chat-text",
          label: t("ai.useInstructions"),
          hint: note,
          needsAi: true,
          run: () => void remix("custom", note, t("remix.run")),
        });
      }
      for (const { preset, icon } of PRESETS) {
        list.push({
          key: preset,
          group: "page",
          icon,
          label: t(`remix.preset.${preset}`),
          hint: t(`remix.presetHint.${preset}`),
          needsAi: true,
          run: () => void remix(preset, note, t(`remix.preset.${preset}`)),
        });
      }
      list.push({
        key: "translate",
        group: "page",
        icon: "translate",
        label: `${t("remix.preset.translate")}…`,
        hint: t("remix.presetHint.translate"),
        needsAi: true,
        run: () => void openTranslate(),
      });
    } else if (note) {
      list.push({
        key: "organize-note",
        group: "library",
        icon: "chat-text",
        label: t("ai.organizeWithNote"),
        hint: note,
        needsAi: true,
        run: () => openOrganize("unsorted", false),
      });
    }
    list.push(
      {
        key: "organize-unsorted",
        group: "library",
        icon: "folders",
        label: t("ai.organizeUnsorted"),
        hint: unsortedCount > 0 ? t("organize.scopeUnsortedText", { count: unsortedCount }) : t("ai.nothingUnsorted"),
        needsAi: true,
        disabled: unsortedCount === 0,
        run: () => openOrganize("unsorted", true),
      },
      {
        key: "organize-all",
        group: "library",
        icon: "tree-structure",
        label: t("ai.organizeAll"),
        hint: t("organize.scopeAllText", { count: app.library.length }),
        needsAi: true,
        disabled: app.library.length === 0,
        run: () => openOrganize("all", true),
      },
      {
        key: "import-chats",
        group: "library",
        icon: "download-simple",
        label: t("aiImport.title"),
        hint: t("aiImport.paletteHint"),
        needsAi: false,
        run: () => {
          app.ai = null;
          app.aiImportOpen = true;
        },
      },
    );
    return list;
  });

  const usable = (a: Action) => !a.disabled;

  function runAction(a: Action | undefined) {
    if (!a || !usable(a)) return;
    if (a.needsAi && !ready) void nudgeSetup();
    else a.run();
  }

  /** An action that needs AI, before any is set up: point at the way to set it up. */
  async function nudgeSetup() {
    if (checking) return;
    nudging = false;
    await tick();
    nudging = true;
    setupBtn?.focus();
  }

  function onAskInput() {
    active = note ? 0 : -1;
    // Grow with the text, up to a few lines.
    if (askEl) {
      askEl.style.height = "auto";
      askEl.style.height = `${Math.min(askEl.scrollHeight, 120)}px`;
    }
  }

  function moveActive(step: 1 | -1) {
    const n = actions.length;
    if (n === 0) return;
    let i = active;
    for (let k = 0; k < n; k++) {
      i = i < 0 ? (step > 0 ? 0 : n - 1) : (i + step + n) % n;
      if (usable(actions[i])) break;
    }
    active = i;
    document.getElementById(`ai-action-${i}`)?.scrollIntoView({ block: "nearest" });
  }

  function onAskKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      moveActive(e.key === "ArrowDown" ? 1 : -1);
    } else if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      runAction(actions[active >= 0 ? active : note ? 0 : -1]);
    }
  }

  async function openTranslate() {
    view = "translate";
    error = null;
    await tick();
    languageEl?.focus();
  }

  function openOrganize(scope: "unsorted" | "all", auto: boolean) {
    organizeScope = scope;
    organizeWishes = note;
    organizeStep = "options";
    organizeAuto = auto;
    error = null;
    view = "organize";
  }

  async function remix(preset: RemixPreset, extra: string, label: string) {
    if (!ready || !page || remixing) return;
    const id = page.id;
    const from: View = view;
    remixing = { id, label };
    view = "remixing";
    error = null;
    chars = 0;
    started = now = Date.now();
    timer = setInterval(() => (now = Date.now()), 500);
    try {
      const full = preset === "translate" ? [`Target language: ${language.trim()}`, extra].filter(Boolean).join("\n") : extra;
      await api.remixPage(id, preset, full);
      if (destroyed) return;
      app.proposals = await api.listProposals();
      toast(t("remix.done"), "success");
      app.ai = null;
      if (app.readId === id) app.proposalOpen = true;
    } catch (e) {
      if (destroyed) return;
      const msg = errorMessage(e);
      error = msg === "cancelled" ? null : msg;
      view = from;
    } finally {
      remixing = null;
      if (timer) clearInterval(timer);
      timer = null;
    }
  }

  function cancelRemix() {
    if (remixing) void api.cancelRemix(remixing.id);
  }

  async function back() {
    if (busy) return;
    view = "home";
    error = null;
    await tick();
    askEl?.focus();
  }

  function close() {
    if (view === "organize" && organizeStep === "applying") return;
    cancelRemix();
    app.ai = null;
  }

  function toSettings() {
    close();
    void openSettings("ai");
  }

  // Capture phase: the app's own Escape would otherwise also leave the reader.
  function onKey(e: KeyboardEvent) {
    if (app.paletteOpen || confirmState.pending || folderPickerState.pending) return;
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === "Escape" || (mod && e.key.toLowerCase() === "j")) {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && view !== "home" && !busy) void back();
      else close();
    }
  }

  // Another page opened behind the panel: its remix is no longer wanted.
  $effect(() => {
    if (remixing && app.readId !== remixing.id) cancelRemix();
  });

  async function loadSettings() {
    try {
      const s = await api.aiSettings();
      const c = s.provider === "claude-code" ? await api.claudeCodeStatus() : null;
      cached = { settings: s, claude: c };
      if (destroyed) return;
      settings = s;
      claude = c;
    } catch (e) {
      if (!destroyed && !settings) error = errorMessage(e);
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey, true);
    askEl?.focus();
    void loadSettings();
    void listen<{ id: string; chars: number }>("remix-progress", (e) => {
      if (remixing && e.payload.id === remixing.id && e.payload.chars > 0) chars = e.payload.chars;
    }).then((stop) => {
      if (destroyed) stop();
      else unlisten = stop;
    });
  });

  onDestroy(() => {
    destroyed = true;
    cancelRemix();
    window.removeEventListener("keydown", onKey, true);
    unlisten?.();
    if (timer) clearInterval(timer);
    // Back to where the panel was opened from, once the window is live again.
    const el = returnFocus;
    void tick().then(() => {
      const lost = !document.activeElement || document.activeElement === document.body;
      if (lost && el?.isConnected && !el.closest("[inert]")) el.focus();
    });
  });
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && !busy && close()}>
  <div class="panel" role="dialog" aria-modal="true" aria-labelledby="ai-title" class:wide={view === "organize"}>
    <header class="head">
      {#if view === "home"}
        <span class="mark"><Icon name="sparkle" size={15} /></span>
      {:else}
        <button class="btn btn-ghost btn-icon" aria-label={t("common.back")} title={t("common.back")} onclick={back} disabled={busy}>
          <Icon name="arrow-left" size={15} />
        </button>
      {/if}
      <h2 id="ai-title" class="ellipsis">
        {#if view === "home"}{t("ai.panel")}
        {:else if view === "translate"}{t("remix.preset.translate")}
        {:else if view === "remixing"}{remixing?.label}
        {:else}{t("organize.title")}{/if}
      </h2>
      {#if (view === "translate" || view === "remixing") && page}
        <span class="context ellipsis">{page.title || t("common.untitled")}</span>
      {/if}
      <span class="grow"></span>
      <button class="btn btn-ghost btn-icon" aria-label={t("common.close")} title={t("common.close")} onclick={close} disabled={view === "organize" && organizeStep === "applying"}>
        <Icon name="x" size={15} />
      </button>
    </header>

    {#if view === "home"}
      <div class="ask">
        <textarea
          bind:this={askEl}
          bind:value={q}
          rows="1"
          oninput={onAskInput}
          onkeydown={onAskKey}
          placeholder={page ? t("ai.askPage") : t("ai.askLibrary")}
          aria-label={page ? t("ai.askPage") : t("ai.askLibrary")}
          role="combobox"
          aria-expanded="true"
          aria-controls="ai-actions"
          aria-activedescendant={active >= 0 ? `ai-action-${active}` : undefined}
          aria-autocomplete="list"
          spellcheck="true"
        ></textarea>
        <kbd class="kbd" class:on={!!note}>↵</kbd>
      </div>
      {#if note}<p class="ask-hint">{t("ai.askHint")}</p>{/if}
      {#if error}<p class="err" role="alert">{error}</p>{/if}

      <div class="ai-body list" id="ai-actions" role="listbox" aria-label={t("ai.panel")}>
        {#each actions as a, i (a.key)}
          {#if i === 0 || actions[i - 1].group !== a.group}
            <div class="group" role="presentation">
              {#if a.group === "page" && page}
                {t("ai.groupPage")}<span class="group-title ellipsis">{page.title || t("common.untitled")}</span>
              {:else}
                {t("ai.groupLibrary")}
              {/if}
            </div>
          {/if}
          <div
            id={`ai-action-${i}`}
            class="item"
            class:active={i === active}
            class:off={!usable(a)}
            role="option"
            tabindex={-1}
            aria-selected={i === active}
            aria-disabled={!usable(a)}
            onmousemove={() => usable(a) && (active = i)}
            onmousedown={(e) => e.preventDefault()}
            onclick={() => runAction(a)}
            onkeydown={(e) => e.key === "Enter" && runAction(a)}
          >
            <span class="icon"><Icon name={a.icon} size={16} /></span>
            <span class="text">
              <span class="label ellipsis">{a.label}</span>
              <span class="hint ellipsis">{a.hint}</span>
            </span>
            <span class="go"><Icon name="chevron-right" size={13} /></span>
          </div>
        {/each}
      </div>

      <footer class="ai-foot status" class:nudging onanimationend={() => (nudging = false)}>
        {#if checking && !error}
          <span class="muted small"><span class="spinner tiny"></span>{t("ai.checking")}</span>
        {:else if ready && settings}
          <span class="muted small ellipsis">
            <span class="dot ok"></span>{t("remix.via", { provider: provider?.label ?? settings.provider, model: settings.model || t("ai.autoShort") })}
          </span>
          <button class="btn btn-xs btn-ghost" onclick={toSettings}>{t("ai.change")}</button>
        {:else}
          <span class="setup small"><span class="dot warn"></span>{setupText}</span>
          <button bind:this={setupBtn} class="btn btn-sm btn-primary" onclick={toSettings}>{t("ai.setUp")}</button>
        {/if}
      </footer>
    {:else if view === "translate"}
      <div class="ai-body">
        <label class="field">
          {t("remix.language")}
          <input
            bind:this={languageEl}
            type="text"
            bind:value={language}
            placeholder={t("remix.languagePlaceholder")}
            onkeydown={(e) => e.key === "Enter" && language.trim() && void remix("translate", note, t("remix.preset.translate"))}
          />
        </label>
        <div class="langs" role="group" aria-label={t("ai.languages")}>
          {#each languageNames as name (name)}
            <button type="button" class="lang" class:on={language === name} onclick={() => (language = name)}>{name}</button>
          {/each}
        </div>
        {#if note}<p class="muted small">{t("ai.withNote", { note })}</p>{/if}
        {#if error}<p class="err" role="alert">{error}</p>{/if}
      </div>
      <footer class="ai-foot">
        <span class="muted small">{t("ai.reviewFirst")}</span>
        <button class="btn btn-sm btn-primary" disabled={!ready || !language.trim()} onclick={() => void remix("translate", note, t("remix.preset.translate"))}>
          <Icon name="translate" size={13} />{t("remix.preset.translate")}
        </button>
      </footer>
    {:else if view === "remixing"}
      <div class="ai-body">
        <div class="working" role="status" aria-live="polite">
          <span class="spinner"></span>
          <div>
            <strong>{t("ai.remixing")}</strong>
            <span class="muted small">
              {chars > 0 ? t("remix.writing", { chars: chars.toLocaleString(), seconds: elapsed }) : t("remix.thinking", { seconds: elapsed })}
            </span>
          </div>
        </div>
        <div class="bar" aria-hidden="true"><span></span></div>
      </div>
      <footer class="ai-foot">
        <span class="muted small">{t("ai.reviewFirst")}</span>
        <button class="btn btn-sm" onclick={cancelRemix}>{t("common.cancel")}</button>
      </footer>
    {:else}
      <OrganizeFlow
        bind:scope={organizeScope}
        bind:wishes={organizeWishes}
        bind:step={organizeStep}
        autostart={organizeAuto}
        {ready}
        onClose={close}
      />
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 11vh 24px 24px;
    background: var(--scrim);
    animation: fade-in var(--t-fast) var(--ease-out);
  }
  .panel {
    width: 600px;
    max-width: 100%;
    max-height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    animation: pop-in var(--t-med) var(--ease-out);
  }
  .panel.wide {
    width: 680px;
  }

  .head {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 10px 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .head .btn-icon:first-child {
    margin-left: -6px;
  }
  .mark {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: var(--radius-sm);
    background: var(--leaf-soft);
    color: var(--leaf);
  }
  h2 {
    margin: 0;
    min-width: 0;
    font-size: var(--fs-md);
    font-weight: 500;
    color: var(--text);
  }
  .context {
    min-width: 0;
    flex: 0 1 auto;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .context::before {
    content: "·";
    margin-right: 8px;
  }
  .grow {
    flex: 1;
  }

  .ask {
    flex: none;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 14px 16px 12px;
  }
  .ask textarea {
    flex: 1;
    min-height: 24px;
    max-height: 120px;
    resize: none;
    border: none;
    padding: 2px 0;
    background: transparent;
    box-shadow: none;
    outline: none;
    font: inherit;
    font-size: var(--fs-md);
    line-height: 1.45;
    color: var(--text);
  }
  .ask .kbd {
    margin-top: 2px;
    opacity: 0.45;
    transition: opacity var(--t-fast) var(--ease-out);
  }
  .ask .kbd.on {
    opacity: 1;
  }
  .ask-hint {
    margin: -6px 16px 6px;
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .err {
    margin: 0 16px 8px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--fs-sm);
    overflow-wrap: anywhere;
  }
  .ai-body .err {
    margin: 0;
  }

  /* Shared with the flows shown inside the panel. */
  .panel :global(.ai-body) {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 18px;
  }
  .panel :global(.ai-foot) {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 48px;
    padding: 8px 12px 8px 16px;
    border-top: 1px solid var(--border);
    background: var(--raised);
  }
  .panel :global(.ai-foot .btn) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .panel .list {
    gap: 0;
    padding: 2px 6px 8px;
    border-top: 1px solid var(--border);
  }
  .group {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    padding: 12px 10px 4px;
    font-size: var(--fs-2xs);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  .group-title {
    min-width: 0;
    text-transform: none;
    letter-spacing: 0;
    font-size: var(--fs-xs);
    font-weight: 400;
  }
  .group-title::before {
    content: "· ";
  }
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-soft);
  }
  .item.active {
    background: var(--sunken);
    color: var(--accent-strong);
  }
  .item.off {
    opacity: 0.5;
    cursor: default;
  }
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    background: var(--raised);
    border: 1px solid var(--border);
    color: var(--text-soft);
  }
  .item.active .icon {
    background: var(--surface);
    color: var(--leaf);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .label {
    font-size: var(--fs-base);
    font-weight: 500;
  }
  .hint {
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .go {
    flex: none;
    color: var(--muted);
    opacity: 0;
  }
  .item.active .go {
    opacity: 1;
  }

  .muted {
    margin: 0;
    color: var(--muted);
  }
  .small {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font-size: var(--fs-xs);
  }
  .setup {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    color: var(--text-soft);
    font-size: var(--fs-xs);
    line-height: 1.45;
  }
  .dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .setup .dot {
    margin-top: 5px;
  }
  .dot.ok {
    background: var(--ok);
  }
  .dot.warn {
    background: var(--warn);
  }
  .status.nudging {
    animation: nudge 0.9s var(--ease-out);
  }
  @keyframes nudge {
    0%,
    100% {
      background: var(--raised);
    }
    30% {
      background: var(--warn-soft);
    }
  }
  .spinner.tiny {
    width: 11px;
    height: 11px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: var(--fs-sm);
  }
  .langs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .lang {
    padding: 4px 10px;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-soft);
    font-size: var(--fs-xs);
    cursor: pointer;
  }
  .lang:hover {
    background: var(--sunken);
  }
  .lang.on {
    border-color: var(--leaf);
    color: var(--leaf);
    background: var(--leaf-soft);
  }

  .working {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 4px 4px;
  }
  .working div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .bar {
    position: relative;
    height: 3px;
    border-radius: 999px;
    background: var(--sunken);
    overflow: hidden;
  }
  .bar span {
    position: absolute;
    inset: 0 auto 0 0;
    width: 30%;
    border-radius: inherit;
    background: var(--leaf);
    animation: slide 1.4s var(--ease-out) infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bar span {
      animation: none;
      width: 100%;
      opacity: 0.4;
    }
  }
  @media (max-height: 640px) {
    .overlay {
      padding-top: 4vh;
    }
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
  @keyframes pop-in {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }
</style>
