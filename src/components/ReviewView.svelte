<script lang="ts">
  import { api } from "../lib/api";
  import { app, refreshAll, toast } from "../lib/state.svelte";
  import { dueInfo, fmtDate, plural, startOfToday } from "../lib/format";
  import type { PageMeta } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";

  const REVIEW_INTERVALS = [1, 3, 7, 30];
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

  async function reschedule(id: string, days: number) {
    if (busyId) return;
    busyId = id;
    try {
      await api.scheduleReview(id, days);
      due = due.filter((p) => p.id !== id);
      toast(t("toast.reviewScheduled", { days: plural(days, "day") }), "success");
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
  <header class="head">
    <div class="head-text">
      <h1>{t("review.title")}</h1>
      <p class="sub">
        {t("review.sub")}
      </p>
    </div>
    <button class="btn" onclick={() => (app.view = "list")}>
      <Icon name="files" size={14} />
      {t("review.allPages")}
    </button>
  </header>

  {#if loaded && !failed && due.length > 0}
    <div class="stats">
      {#if overdueCount > 0}
        <span class="chip chip-warn">
          <Icon name="calendar-clock" size={12} />
          {t("review.overdue", { pages: plural(overdueCount, "page") })}
        </span>
      {/if}
      <span class="chip">
        <Icon name="calendar-clock" size={12} />
        {t("review.toReview", { pages: plural(due.length, "page") })}
      </span>
      {#if reviewedToday > 0}
        <span class="chip chip-muted">
          <Icon name="check" size={12} />
          {t("review.reviewedToday", { count: reviewedToday })}
        </span>
      {/if}
    </div>
  {/if}

  {#if !loaded}
    <div class="list" role="status" aria-label={t("review.loading")}>
      {#each [0, 1, 2] as i (i)}
        <div class="skel-row skel" aria-hidden="true"></div>
      {/each}
    </div>
  {:else if failed}
    <div class="empty" role="alert">
      <span class="empty-icon"><Icon name="info" size={22} /></span>
      <strong>{t("review.loadFailed")}</strong>
      <span>{t("review.loadFailedHint")}</span>
      <button class="btn btn-sm" onclick={load}>{t("boot.retry")}</button>
    </div>
  {:else if due.length === 0}
    <div class="empty">
      <span class="empty-icon"><Icon name="circle-check" size={22} /></span>
      <strong>{t("review.caughtUp")}</strong>
      <span>{t("review.caughtUpHint")}</span>
      <button class="btn btn-sm" onclick={() => (app.view = "list")}>{t("review.browse")}</button>
    </div>
  {:else}
    <div class="list">
      {#each due as p (p.id)}
        {@const d = dueInfo(p.nextReview)}
        <article class="card row">
          <button class="row-open" onclick={() => open(p.id)}>
            <span class="thumb"><Icon name="leaf" size={18} /></span>
            <span class="body">
              <span class="title ellipsis">{p.title || t("common.untitled")}</span>
              <span class="meta">
                {#if p.folder}
                  <span class="loc"><Icon name="folder" size={12} />{p.folder}</span>
                {/if}
                {#each p.tags.slice(0, 3) as tag (tag)}
                  <span class="chip chip-muted">#{tag}</span>
                {/each}
              </span>
            </span>
          </button>
          <span class="side">
            {#if d}
              <span class="due" class:overdue={d.overdue}>{d.label}</span>
            {/if}
            <span class="date">{fmtDate(p.nextReview)}</span>
            <span class="resched" role="group" aria-label={t("review.reschedule")}>
              {#each REVIEW_INTERVALS as days (days)}
                <button
                  class="btn btn-xs"
                  title={t("review.againIn", { days: plural(days, "day") })}
                  disabled={busyId === p.id}
                  onclick={() => reschedule(p.id, days)}
                >
                  {t("review.daysShort", { n: days })}
                </button>
              {/each}
            </span>
            <button class="btn btn-sm btn-primary" onclick={() => open(p.id)}>{t("common.open")}</button>
          </span>
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px 20px 24px;
    height: 100%;
    overflow-y: auto;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    flex-wrap: wrap;
  }

  h1 {
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }

  .sub {
    margin-top: 3px;
    font-size: 13px;
    color: var(--muted);
    max-width: 560px;
  }

  .stats {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    transition: border-color var(--t-fast) var(--ease-out);
  }

  .row:hover {
    border-color: var(--border-hover);
  }

  .row-open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    text-align: left;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }

  .thumb {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
  }

  .title {
    display: block;
    max-width: 100%;
    font-size: 13.5px;
    font-weight: 600;
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

  .side {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .due {
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--warn-soft);
    color: var(--warn);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }

  .due.overdue {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .date {
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
  }

  .resched {
    display: flex;
    gap: 4px;
  }

  .skel-row {
    height: 66px;
    border-radius: var(--radius);
  }

  .skel {
    background: var(--sunken);
    animation: pulse 1.3s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
</style>
