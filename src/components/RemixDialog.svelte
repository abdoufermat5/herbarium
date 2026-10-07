<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "../lib/api";
  import { app, errorMessage, openSettings, toast } from "../lib/state.svelte";
  import { t } from "../lib/i18n.svelte";
  import type { AiSettings, RemixPreset } from "../lib/types";
  import Icon from "../lib/Icon.svelte";

  let {
    pageId,
    onDone,
    onClose,
  }: {
    pageId: string;
    /** The remix is waiting as a proposal. */
    onDone: () => void;
    onClose: () => void;
  } = $props();

  const PRESETS: RemixPreset[] = ["simplify", "deeper", "quiz", "translate", "cheatsheet", "modernize", "custom"];

  let settings = $state<AiSettings | null>(null);
  let preset = $state<RemixPreset>("simplify");
  let language = $state("");
  let instructions = $state("");
  let busy = $state(false);
  let chars = $state(0);
  let started = $state(0);
  let now = $state(0);
  let error = $state<string | null>(null);
  let closeBtn: HTMLButtonElement | undefined = $state();
  let unlisten: UnlistenFn | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;

  const provider = $derived(settings?.providers.find((p) => p.id === settings?.provider) ?? null);
  const needsKey = $derived(!!settings && !!provider?.needsKey && !settings.keys.includes(settings.provider));
  const extra = $derived(preset === "translate" ? language.trim() : instructions.trim());
  const ready = $derived(
    !!settings && !needsKey && !busy && (preset === "translate" || preset === "custom" ? extra.length > 0 : true),
  );
  const elapsed = $derived(busy ? Math.max(0, Math.round((now - started) / 1000)) : 0);

  async function run() {
    if (!ready) return;
    busy = true;
    error = null;
    chars = 0;
    started = now = Date.now();
    timer = setInterval(() => (now = Date.now()), 500);
    try {
      const full = preset === "translate" ? `Target language: ${extra}` : extra;
      await api.remixPage(pageId, preset, full);
      app.proposals = await api.listProposals();
      toast(t("remix.done"), "success");
      onDone();
    } catch (e) {
      const msg = errorMessage(e);
      if (msg !== "cancelled") error = msg;
    } finally {
      busy = false;
      if (timer) clearInterval(timer);
      timer = null;
    }
  }

  function close() {
    if (busy) void api.cancelRemix();
    onClose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void run();
    }
  }

  async function toSettings() {
    onClose();
    await openSettings("ai");
  }

  onMount(async () => {
    // Capture phase: the app's own Escape would otherwise close the reader.
    window.addEventListener("keydown", onKey, true);
    closeBtn?.focus();
    unlisten = await listen<{ id: string; chars: number }>("remix-progress", (e) => {
      if (e.payload.id === pageId && e.payload.chars > 0) chars = e.payload.chars;
    });
    try {
      settings = await api.aiSettings();
    } catch (e) {
      error = errorMessage(e);
    }
  });

  onDestroy(() => {
    window.removeEventListener("keydown", onKey, true);
    unlisten?.();
    if (timer) clearInterval(timer);
  });
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && !busy && close()}>
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="remix-title">
    <header class="head">
      <h2 id="remix-title"><Icon name="sparkle" size={16} />{t("remix.title")}</h2>
      <button bind:this={closeBtn} class="btn btn-ghost btn-icon" aria-label={t("common.close")} onclick={close}>
        <Icon name="x" size={15} />
      </button>
    </header>
    <p class="muted">{t("remix.intro")}</p>

    {#if needsKey}
      <div class="notice">
        <span>{t("remix.needsKey", { provider: provider?.label ?? "" })}</span>
        <button class="btn btn-sm" onclick={toSettings}>{t("remix.openSettings")}</button>
      </div>
    {/if}

    <div class="presets" role="radiogroup" aria-label={t("remix.presets")}>
      {#each PRESETS as p (p)}
        <button
          role="radio"
          aria-checked={preset === p}
          class="preset"
          class:on={preset === p}
          disabled={busy}
          onclick={() => (preset = p)}
        >
          <strong>{t(`remix.preset.${p}`)}</strong>
          <span>{t(`remix.presetHint.${p}`)}</span>
        </button>
      {/each}
    </div>

    {#if preset === "translate"}
      <label class="field">
        <span>{t("remix.language")}</span>
        <input type="text" bind:value={language} placeholder={t("remix.languagePlaceholder")} disabled={busy} />
      </label>
    {:else}
      <label class="field">
        <span>{preset === "custom" ? t("remix.instructions") : t("remix.extra")}</span>
        <textarea
          rows="3"
          bind:value={instructions}
          placeholder={preset === "custom" ? t("remix.customPlaceholder") : t("remix.extraPlaceholder")}
          disabled={busy}
        ></textarea>
      </label>
    {/if}

    {#if error}
      <p class="err" role="alert">{error}</p>
    {/if}

    <footer class="foot">
      <span class="muted small">
        {#if busy}
          <span class="spinner"></span>
          {chars > 0 ? t("remix.writing", { chars: chars.toLocaleString(), seconds: elapsed }) : t("remix.thinking", { seconds: elapsed })}
        {:else if settings}
          {t("remix.via", { provider: provider?.label ?? settings.provider, model: settings.model })}
        {/if}
      </span>
      <div class="buttons">
        <button class="btn btn-sm" onclick={close}>{t("common.cancel")}</button>
        <button class="btn btn-sm btn-primary" onclick={run} disabled={!ready}>
          <Icon name="sparkle" size={13} />{t("remix.run")}
        </button>
      </div>
    </footer>
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
    padding: 10vh 16px 16px;
    background: var(--scrim);
  }
  .dialog {
    width: 620px;
    max-width: 100%;
    max-height: 84vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 22px 24px;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 500;
  }
  .muted {
    margin: 0;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .small {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-xs);
  }
  .notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--sunken);
    font-size: var(--fs-sm);
  }
  .presets {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 8px;
  }
  .preset {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .preset:hover:not(:disabled) {
    background: var(--sunken);
  }
  .preset.on {
    border-color: var(--leaf);
    box-shadow: inset 0 0 0 1px var(--leaf);
  }
  .preset strong {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .preset span {
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: var(--fs-sm);
  }
  .field textarea {
    resize: vertical;
    font: inherit;
  }
  .err {
    margin: 0;
    color: var(--danger);
    font-size: var(--fs-sm);
  }
  /* The actions stay on screen however short the window. */
  .foot {
    position: sticky;
    bottom: -22px;
    margin: 0 -24px -22px;
    padding: 12px 24px 16px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  @media (max-height: 640px) {
    .overlay {
      padding-top: 4vh;
    }
    .dialog {
      max-height: 92vh;
    }
    .preset span {
      display: none;
    }
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .buttons .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
