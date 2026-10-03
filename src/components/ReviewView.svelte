<script lang="ts">
  import { api } from "../lib/api";
  import { app, refreshAll, toast } from "../lib/state.svelte";
  import { dueInfo, fmtDate, fmtDateTime, fmtDuration, plural, startOfToday } from "../lib/format";
  import type { PageMeta } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { reveal } from "../lib/reveal";
  import PresetButtons from "./PresetButtons.svelte";
  import { t } from "../lib/i18n.svelte";

  // Preset intervals come from the vault's review settings (`app.review`).
  const DAY = 86400000;

  let due = $state<PageMeta[]>([]);
  let loaded = $state(false);
  let failed = $state(false);
  let busyId = $state<string | null>(null);
  let loadToken = 0;

  const now = Date.now();
  const overdueCount = $derived(
    due.filter((p) => p.nextReview && now - p.nextReview >= DAY).length,
  );
  const reviewedToday = $derived(
    app.pages.filter((p) => p.lastReview && p.lastReview >= startOfToday()).length,
  );

  function load() {
    const token = ++loadToken;
    loaded = false;
    failed = false;
    api
      .reviewToday()
      .then((rows) => {
        if (token !== loadToken) return;
        due = [...rows].sort((a, b) => (a.nextReview ?? 0) - (b.nextReview ?? 0));
        loaded = true;
      })
      .catch((e) => {
        if (token !== loadToken) return;
        console.error(e);
        due = [];
        failed = true;
        loaded = true;
      });
  }

  $effect(() => {
    if (app.view !== "review") return;
    load();
  });

  async function reschedule(id: string, minutes: number) {
    if (busyId) return;
    busyId = id;
    try {
      await api.scheduleReview(id, minutes);
      due = due.filter((p) => p.id !== id);
      toast(t("toast.reviewScheduled", { when: fmtDuration(minutes) }), "success");
      void refreshAll();
    } catch {
      toast(t("review.rescheduleFailed"), "error");
    } finally {
      busyId = null;
    }
  }

  async function complete(id: string) {
    if (busyId) return;
    busyId = id;
    try {
      const next = await api.completeReview(id);
      due = due.filter((p) => p.id !== id);
      toast(t("toast.reviewed", { when: fmtDuration(next.intervalMinutes ?? 1) }), "success");
      void refreshAll();
    } catch {
      toast(t("review.rescheduleFailed"), "error");
    } finally {
      busyId = null;
    }
  }

  function open(id: string) {
    app.readId = id;
  }
</script>

<div class="pane">
  <div class="inner">
    <header class="head">
      <div class="head-text">
        <span class="eyebrow">{fmtDate(now)}</span>
        <h1 class="display">{t("review.title")}</h1>
        <p class="sub">{t("review.sub")}</p>
      </div>
      <button class="btn" onclick={() => (app.view = "list")}>
        <Icon name="files" size={14} />
        {t("review.allPages")}
      </button>
    </header>

    {#if loaded && !failed && due.length > 0}
      <div class="stats">
        {#if overdueCount > 0}
          <span class="chip chip-danger">
            {t("review.overdue", { pages: plural(overdueCount, "page") })}
          </span>
        {/if}
        <span class="chip chip-warn">
          {t("review.toReview", { pages: plural(due.length, "page") })}
        </span>
        {#if reviewedToday > 0}
          <span class="chip chip-ok">
            {t("review.reviewedToday", { count: reviewedToday })}
          </span>
        {/if}
      </div>
    {/if}

    {#if !loaded}
      <div class="list" role="status" aria-label={t("review.loading")}>
        {#each [0, 1, 2] as i (i)}
          <div class="skel-row" aria-hidden="true"></div>
        {/each}
      </div>
    {:else if failed}
      <div class="empty" role="alert">
        <span class="empty-icon"><Icon name="info" size={20} /></span>
        <strong>{t("review.loadFailed")}</strong>
        <span>{t("review.loadFailedHint")}</span>
        <button class="btn btn-sm" onclick={load}>{t("boot.retry")}</button>
      </div>
    {:else if due.length === 0}
      <div class="empty">
        <span class="empty-icon"><Icon name="circle-check" size={20} /></span>
        <strong>{t("review.caughtUp")}</strong>
        <span>{t("review.caughtUpHint")}</span>
        <button class="btn btn-sm" onclick={() => (app.view = "list")}>{t("review.browse")}</button>
      </div>
    {:else}
      <div class="list">
        {#each due as p (p.id)}
          {@const d = dueInfo(p.nextReview)}
          <article class="row" use:reveal>
            <button class="row-open" onclick={() => open(p.id)}>
              <span class="thumb"><Icon name="leaf" size={15} /></span>
              <span class="body">
                <span class="title ellipsis">{p.title || t("common.untitled")}</span>
                <span class="meta">
                  {#if p.folder}
                    <span class="loc"><Icon name="folder" size={11} />{p.folder}</span>
                  {/if}
                  {#each p.tags.slice(0, 3) as tag (tag)}
                    <span class="chip chip-muted">{tag}</span>
                  {/each}
                  <span class="date">{fmtDateTime(p.nextReview)}</span>
                </span>
              </span>
            </button>
            <span class="side">
              {#if d}
                <span class="chip" class:chip-danger={d.overdue} class:chip-warn={!d.overdue}>{d.label}</span>
              {/if}
              <span class="resched" role="group" aria-label={t("review.reschedule")}>
                <PresetButtons onpick={(m) => reschedule(p.id, m)} disabled={busyId === p.id} />
              </span>
              <button
                class="btn btn-sm"
                disabled={busyId === p.id}
                title={t("review.doneHint")}
                onclick={() => complete(p.id)}
              >
                <Icon name="check" size={13} />{t("review.done")}
              </button>
              <button class="btn btn-sm btn-primary" onclick={() => open(p.id)}>{t("common.open")}</button>
            </span>
          </article>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .pane {
    height: 100%;
    overflow-y: auto;
  }
  .inner {
    max-width: var(--content-w);
    margin: 0 auto;
    padding: 48px 40px 72px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 24px;
    flex-wrap: wrap;
  }
  .head-text {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  h1 {
    font-size: 40px;
  }
  .sub {
    font-size: 14px;
    color: var(--muted);
    max-width: 560px;
  }

  .stats {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .list {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 8px 0 0;
    border-bottom: 1px solid var(--border);
    transition: background var(--t-med) var(--ease-out);
  }
  .row:hover {
    background: var(--surface);
  }

  .row-open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 8px;
    text-align: left;
    border-radius: var(--radius-sm);
  }

  .thumb {
    flex: none;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm);
    background: var(--leaf-soft);
    color: var(--leaf);
  }

  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .title {
    font-size: 14px;
    font-weight: 500;
    color: var(--text);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .loc {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-size: 12px;
  }
  .date {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--muted);
  }

  .side {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .resched {
    display: flex;
    gap: 4px;
  }

  .skel-row {
    height: 68px;
    border-bottom: 1px solid var(--border);
    animation: pulse 1.6s ease-in-out infinite;
    background: var(--surface);
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
</style>
