<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "../lib/api";
  import { app, errorMessage, openSettings, reloadPages, toast } from "../lib/state.svelte";
  import { confirmState } from "../lib/confirm.svelte";
  import { folderPickerState } from "../lib/folder-picker.svelte";
  import { t } from "../lib/i18n.svelte";
  import type { AiSettings, ClaudeCodeStatus, OrganizeMove, OrganizePlan } from "../lib/types";
  import Icon from "../lib/Icon.svelte";

  const KEY = "organize:library";
  type Step = "options" | "working" | "review" | "applying";

  let step = $state<Step>("options");
  let scope = $state<"unsorted" | "all">("unsorted");
  let wishes = $state("");
  let plan = $state<OrganizePlan | null>(null);
  let picked = $state<Set<string>>(new Set());
  let error = $state<string | null>(null);
  let settings = $state<AiSettings | null>(null);
  let claude = $state<ClaudeCodeStatus | null>(null);
  let chars = $state(0);
  let started = $state(0);
  let now = $state(0);
  let closeBtn: HTMLButtonElement | undefined = $state();
  let unlisten: UnlistenFn | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;
  let destroyed = false;

  const provider = $derived(settings?.providers.find((p) => p.id === settings?.provider) ?? null);
  const needsKey = $derived(!!settings && !!provider?.needsKey && !settings.keys.includes(settings.provider));
  const claudeMissing = $derived(settings?.provider === "claude-code" && !!claude && !claude.version);
  const needsSetup = $derived(needsKey || claudeMissing);
  const unsortedCount = $derived(app.library.filter((p) => !p.folder || p.folder.toLowerCase() === "inbox").length);
  const elapsed = $derived(step === "working" ? Math.max(0, Math.round((now - started) / 1000)) : 0);

  /** The moves grouped by where they go, biggest group first. */
  const groups = $derived.by(() => {
    const by = new Map<string, { to: string | null; isNew: boolean; moves: OrganizeMove[] }>();
    for (const m of plan?.moves ?? []) {
      const key = m.to ?? "";
      const g = by.get(key) ?? { to: m.to, isNew: m.newFolder, moves: [] };
      g.moves.push(m);
      by.set(key, g);
    }
    return [...by.values()].sort((a, b) => b.moves.length - a.moves.length);
  });
  const chosen = $derived((plan?.moves ?? []).filter((m) => picked.has(m.id)));
  const folderCount = $derived(new Set(chosen.filter((m) => m.newFolder && m.to).map((m) => m.to)).size);

  async function run() {
    if (needsSetup || step !== "options") return;
    step = "working";
    error = null;
    chars = 0;
    started = now = Date.now();
    timer = setInterval(() => (now = Date.now()), 500);
    try {
      const result = await api.organizePlan(scope, wishes);
      plan = result;
      picked = new Set(result.moves.map((m) => m.id));
      step = "review";
    } catch (e) {
      const msg = errorMessage(e);
      error = msg === "cancelled" ? null : msg;
      step = "options";
    } finally {
      if (timer) clearInterval(timer);
      timer = null;
    }
  }

  function close() {
    if (step === "applying") return;
    if (step === "working") void api.cancelOrganize();
    app.organizeOpen = false;
  }

  function toggle(id: string) {
    const next = new Set(picked);
    if (!next.delete(id)) next.add(id);
    picked = next;
  }

  function toggleGroup(moves: OrganizeMove[]) {
    const next = new Set(picked);
    const all = moves.every((m) => next.has(m.id));
    for (const m of moves) {
      if (all) next.delete(m.id);
      else next.add(m.id);
    }
    picked = next;
  }

  /** Move and tag the chosen pages, grouped so each call does one kind of change. */
  async function apply() {
    if (chosen.length === 0 || step !== "review") return;
    step = "applying";
    const moves = chosen;
    const done: OrganizeMove[] = [];
    let failed = 0;
    try {
      const batches = new Map<string, OrganizeMove[]>();
      for (const m of moves) {
        const key = `${m.to === m.from ? "-" : (m.to ?? "")}|${m.addTags.join(",")}`;
        batches.set(key, [...(batches.get(key) ?? []), m]);
      }
      for (const batch of batches.values()) {
        const first = batch[0];
        const result = await api.bulkUpdate(
          batch.map((m) => m.id),
          { ...(first.to !== first.from ? { folder: first.to ?? null } : {}), addTags: first.addTags },
        );
        const ok = new Set(result.updated.map((m) => m.id));
        for (const m of batch) {
          if (ok.has(m.id)) done.push(m);
          else failed++;
        }
      }
    } catch (e) {
      console.error(e);
      toast(errorMessage(e), "error");
    }
    await reloadPages(true);
    app.organizeOpen = false;
    if (done.length > 0) {
      toast(
        failed > 0
          ? t("organize.appliedPartial", { count: done.length, failed })
          : t("organize.applied", { count: done.length }),
        failed > 0 ? "info" : "success",
        15000,
        { label: t("organize.undo"), run: () => void undo(done) },
      );
    }
  }

  /** Put the pages back where they were and take the added tags off again. */
  async function undo(moves: OrganizeMove[]) {
    try {
      const batches = new Map<string, OrganizeMove[]>();
      for (const m of moves) {
        const key = `${m.to === m.from ? "-" : (m.from ?? "")}|${m.addTags.join(",")}`;
        batches.set(key, [...(batches.get(key) ?? []), m]);
      }
      for (const batch of batches.values()) {
        const first = batch[0];
        await api.bulkUpdate(
          batch.map((m) => m.id),
          { ...(first.to !== first.from ? { folder: first.from ?? null } : {}), removeTags: first.addTags },
        );
      }
      await reloadPages(true);
      toast(t("organize.undone"), "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  async function toSettings() {
    app.organizeOpen = false;
    await openSettings("ai");
  }

  // Capture phase, as the other dialogs: Escape closes only this one.
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !app.paletteOpen && !confirmState.pending && !folderPickerState.pending) {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  onMount(async () => {
    window.addEventListener("keydown", onKey, true);
    closeBtn?.focus();
    const stop = await listen<{ id: string; chars: number }>("remix-progress", (e) => {
      if (e.payload.id === KEY && e.payload.chars > 0) chars = e.payload.chars;
    });
    if (destroyed) stop();
    else unlisten = stop;
    try {
      settings = await api.aiSettings();
      if (settings.provider === "claude-code") claude = await api.claudeCodeStatus();
    } catch (e) {
      error = errorMessage(e);
    }
  });

  onDestroy(() => {
    destroyed = true;
    if (step === "working") void api.cancelOrganize();
    window.removeEventListener("keydown", onKey, true);
    unlisten?.();
    if (timer) clearInterval(timer);
  });
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && step === "options" && close()}>
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="organize-title">
    <header class="head">
      <h2 id="organize-title"><Icon name="sparkle" size={16} />{t("organize.title")}</h2>
      <button bind:this={closeBtn} class="btn btn-ghost btn-icon" aria-label={t("common.close")} onclick={close} disabled={step === "applying"}>
        <Icon name="x" size={15} />
      </button>
    </header>

    {#if step === "options"}
      <p class="muted">{t("organize.intro")}</p>

      {#if needsSetup}
        <div class="notice">
          <span>{claudeMissing ? t("remix.needsClaude") : t("remix.needsKey", { provider: provider?.label ?? "" })}</span>
          <button class="btn btn-sm" onclick={toSettings}>{t("remix.openSettings")}</button>
        </div>
      {/if}

      <div class="scopes" role="radiogroup" aria-label={t("organize.scope")}>
        <button type="button" role="radio" aria-checked={scope === "unsorted"} class="scope" class:on={scope === "unsorted"} onclick={() => (scope = "unsorted")}>
          <strong>{t("organize.scopeUnsorted")}</strong>
          <span>{t("organize.scopeUnsortedText", { count: unsortedCount })}</span>
        </button>
        <button type="button" role="radio" aria-checked={scope === "all"} class="scope" class:on={scope === "all"} onclick={() => (scope = "all")}>
          <strong>{t("organize.scopeAll")}</strong>
          <span>{t("organize.scopeAllText", { count: app.library.length })}</span>
        </button>
      </div>

      <label class="field">
        {t("organize.wishes")}
        <textarea rows="3" bind:value={wishes} placeholder={t("organize.wishesPlaceholder")}></textarea>
      </label>
      <p class="muted small">{t("organize.privacy")}</p>
      {#if error}<p class="err" role="alert">{error}</p>{/if}
    {:else if step === "working"}
      <div class="working" role="status" aria-live="polite">
        <span class="spinner"></span>
        <div>
          <strong>{t("organize.working")}</strong>
          <span class="muted small">{chars > 0 ? t("organize.writing", { chars, seconds: elapsed }) : t("organize.reading", { seconds: elapsed })}</span>
        </div>
      </div>
    {:else if plan}
      <p class="summary">{plan.summary || t("organize.summaryNone")}</p>
      {#if plan.leftOut > 0}
        <p class="muted small"><Icon name="info" size={12} />{t("organize.leftOut", { count: plan.leftOut })}</p>
      {/if}
      {#if plan.moves.length === 0}
        <div class="empty">
          <Icon name="circle-check" size={22} />
          <strong>{t("organize.inOrder")}</strong>
          <span class="muted small">{t("organize.inOrderText", { count: plan.considered })}</span>
        </div>
      {:else}
        <div class="review">
          {#each groups as g (g.to ?? "")}
            <section class="group">
              <header>
                <label class="check">
                  <input
                    type="checkbox"
                    checked={g.moves.every((m) => picked.has(m.id))}
                    indeterminate={g.moves.some((m) => picked.has(m.id)) && !g.moves.every((m) => picked.has(m.id))}
                    onchange={() => toggleGroup(g.moves)}
                  />
                  <Icon name={g.isNew ? "folder-plus" : "folder"} size={14} />
                  <strong>{g.to ?? t("organize.topLevel")}</strong>
                </label>
                {#if g.isNew}<span class="chip-new">{t("organize.newFolder")}</span>{/if}
                <span class="count">{g.moves.length}</span>
              </header>
              <ul>
                {#each g.moves as m (m.id)}
                  <li>
                    <label class="check row">
                      <input type="checkbox" checked={picked.has(m.id)} onchange={() => toggle(m.id)} />
                      <span class="what">
                        <span class="title">{m.title}</span>
                        <span class="meta">
                          {#if m.to !== m.from}{t("organize.from", { folder: m.from ?? t("organize.topLevel") })}{/if}
                          {#each m.addTags as tag (tag)}<span class="tag">+ {tag}</span>{/each}
                          {#if m.reason}<span class="reason">{m.reason}</span>{/if}
                        </span>
                      </span>
                    </label>
                  </li>
                {/each}
              </ul>
            </section>
          {/each}
        </div>
      {/if}
    {/if}

    <footer class="foot">
      {#if step === "review" && plan && plan.moves.length > 0}
        <span class="muted small">
          {t("organize.chosen", { count: chosen.length })}{folderCount > 0 ? ` · ${t("organize.newFolders", { count: folderCount })}` : ""}
        </span>
      {:else}
        <span></span>
      {/if}
      <div class="buttons">
        {#if step === "options"}
          <button class="btn btn-sm" onclick={close}>{t("common.cancel")}</button>
          <button class="btn btn-sm btn-primary" onclick={run} disabled={needsSetup || !settings}>
            <Icon name="sparkle" size={13} />{t("organize.suggest")}
          </button>
        {:else if step === "working"}
          <button class="btn btn-sm" onclick={close}>{t("common.cancel")}</button>
        {:else if step === "review"}
          <button class="btn btn-sm" onclick={close}>{plan && plan.moves.length > 0 ? t("organize.discard") : t("common.close")}</button>
          {#if plan && plan.moves.length > 0}
            <button class="btn btn-sm" onclick={() => ((step = "options"), (plan = null))}>{t("organize.again")}</button>
            <button class="btn btn-sm btn-primary" onclick={apply} disabled={chosen.length === 0}>
              {t("organize.apply", { count: chosen.length })}
            </button>
          {/if}
        {:else}
          <button class="btn btn-sm btn-primary" disabled><span class="spinner small"></span>{t("organize.applying")}</button>
        {/if}
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
    padding: 8vh 16px 16px;
    background: var(--scrim);
  }
  .dialog {
    width: 680px;
    max-width: 100%;
    max-height: 86vh;
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
  .scopes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 8px;
  }
  .scope {
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
  .scope:hover {
    background: var(--sunken);
  }
  .scope.on {
    border-color: var(--leaf);
    box-shadow: inset 0 0 0 1px var(--leaf);
  }
  .scope strong {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .scope span {
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
  .working {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 28px 8px;
  }
  .working div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .summary {
    margin: 0;
    font-family: var(--font-display);
    font-size: 17px;
    line-height: 1.4;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 28px 8px;
    color: var(--leaf);
    text-align: center;
  }
  .review {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .group {
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .group > header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--sunken);
  }
  .group > header .check {
    flex: 1;
    min-width: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    font-size: var(--fs-sm);
  }
  .chip-new {
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--leaf-soft);
    color: var(--leaf);
    font-size: var(--fs-2xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .count {
    color: var(--muted);
    font-size: var(--fs-xs);
    font-variant-numeric: tabular-nums;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li + li {
    border-top: 1px solid var(--border);
  }
  .row {
    align-items: flex-start;
    padding: 7px 12px;
  }
  .row:hover {
    background: var(--sunken);
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .tag {
    color: var(--leaf);
  }
  .reason {
    font-style: italic;
  }
  /* The actions stay on screen however long the plan. */
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
  .buttons {
    display: flex;
    gap: 8px;
  }
  .buttons .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .spinner.small {
    width: 12px;
    height: 12px;
  }
  @media (max-height: 640px) {
    .overlay {
      padding-top: 4vh;
    }
    .dialog {
      max-height: 92vh;
    }
  }
</style>
