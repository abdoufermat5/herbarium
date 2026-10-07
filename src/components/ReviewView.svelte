<script lang="ts">
  import { api } from "../lib/api";
  import {
    app,
    errorMessage,
    goAll,
    openPage,
    refreshAll,
    startReviewSession,
    toast,
  } from "../lib/state.svelte";
  import { dueInfo, fmtDate, fmtDateTime, fmtDuration, plural } from "../lib/format";
  import type { PageMeta, ReviewGrade, ReviewStats } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { reveal } from "../lib/reveal";
  import PresetButtons from "./PresetButtons.svelte";
  import { i18n, LOCALES, t } from "../lib/i18n.svelte";

  const now = Date.now();

  let due = $state<PageMeta[]>([]);
  let queueLoaded = $state(false);
  let queueError = $state<string | null>(null);
  let stats = $state<ReviewStats | null>(null);
  let statsError = $state<string | null>(null);
  let busyId = $state<string | null>(null);
  let loadToken = 0;

  /** Freshest statistics we have: this view's fetch, else the shared poll. */
  const shownStats = $derived(stats ?? app.reviewStats);
  const upcoming = $derived(shownStats?.upcoming ?? []);
  const forecastMax = $derived(Math.max(1, ...upcoming.map((d) => d.count)));
  const forecastTotal = $derived(upcoming.reduce((sum, d) => sum + d.count, 0));

  let active = $state(0);
  let listEl = $state<HTMLDivElement | null>(null);
  let expanded = $state<Record<string, boolean>>({});
  let forecastView = $state<"chart" | "table">("chart");

  /** Load the (capped) due queue and the uncapped statistics independently.
   *  `queueLoaded` stays true across reloads so the list never flashes back to
   *  the skeleton; it only starts false on the first load. */
  function load() {
    const token = ++loadToken;
    queueError = null;
    statsError = null;

    void api
      .reviewToday()
      .then((rows) => {
        if (token !== loadToken) return;
        due = [...rows].sort((a, b) => (a.nextReview ?? 0) - (b.nextReview ?? 0));
        active = Math.max(0, Math.min(active, due.length - 1));
        queueLoaded = true;
      })
      .catch((e) => {
        if (token !== loadToken) return;
        console.error(e);
        due = [];
        queueError = errorMessage(e);
        queueLoaded = true;
      });

    void api
      .reviewStats()
      .then((value) => {
        if (token !== loadToken) return;
        stats = value;
        statsError = null;
      })
      .catch((e) => {
        if (token !== loadToken) return;
        console.error(e);
        statsError = errorMessage(e);
      });
  }

  $effect(() => {
    if (app.view !== "review") return;
    // Re-read on every rescan so external changes are reflected.
    void app.vaultRevision;
    load();
  });

  // Keep the roving tabindex in range as the queue shrinks.
  $effect(() => {
    if (active >= due.length) active = Math.max(0, due.length - 1);
  });

  async function reschedule(id: string, minutes: number) {
    if (busyId) return;
    busyId = id;
    try {
      await api.scheduleReview(id, minutes);
      due = due.filter((p) => p.id !== id);
      active = Math.max(0, Math.min(active, due.length - 1));
      toast(t("toast.reviewScheduled", { when: fmtDuration(minutes) }), "success");
      // Re-sync the queue (a capped list may uncover the next page) and the stats.
      load();
      void refreshAll();
    } catch (e) {
      console.error(e);
      toast(t("review.rescheduleFailed"), "error");
    } finally {
      busyId = null;
    }
  }

  function focusRow(index: number) {
    const rows = listEl?.querySelectorAll<HTMLButtonElement>("button.row-open");
    rows?.[index]?.focus();
  }

  function moveRow(index: number) {
    if (due.length === 0) return;
    const next = Math.max(0, Math.min(due.length - 1, index));
    active = next;
    focusRow(next);
  }

  function onRowKey(e: KeyboardEvent, index: number) {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        moveRow(index + 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        moveRow(index - 1);
        break;
      case "Home":
        e.preventDefault();
        moveRow(0);
        break;
      case "End":
        e.preventDefault();
        moveRow(due.length - 1);
        break;
    }
  }

  function onTabKey(e: KeyboardEvent) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    forecastView = forecastView === "chart" ? "table" : "chart";
    requestAnimationFrame(() => document.getElementById(`forecast-tab-${forecastView}`)?.focus());
  }

  function toggleHistory(id: string) {
    expanded = { ...expanded, [id]: !expanded[id] };
  }

  function gradeLabel(grade: ReviewGrade): string {
    return t(`review.grade.${grade}`);
  }

  /** Format a UTC `YYYY-MM-DD` day as a locale date without timezone drift. */
  /** Activity heatmap: columns are weeks (Monday first, UTC), oldest left. */
  const activity = $derived(shownStats?.activity ?? []);
  const activityTotal = $derived(activity.reduce((sum, d) => sum + d.count, 0));
  const activityMax = $derived(Math.max(0, ...activity.map((d) => d.count)));
  const activityWeeks = $derived.by(() => {
    if (activity.length === 0) return [] as Array<Array<(typeof activity)[number] | null>>;
    const [y, m, d] = activity[0].day.split("-").map(Number);
    // getUTCDay: 0 = Sunday; shift so Monday is row 0.
    const lead = (new Date(Date.UTC(y, m - 1, d)).getUTCDay() + 6) % 7;
    const cells: Array<(typeof activity)[number] | null> = [...Array(lead).fill(null), ...activity];
    const weeks = [];
    for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
    return weeks;
  });

  /** 0 for no reviews, else 1..4 by share of the busiest day. */
  function level(count: number): number {
    if (count === 0 || activityMax === 0) return 0;
    return Math.min(4, Math.max(1, Math.ceil((count / activityMax) * 4)));
  }

  function fmtDay(day: string): string {
    const [y, m, d] = day.split("-").map(Number);
    if (!y || !m || !d) return day;
    return new Date(Date.UTC(y, m - 1, d)).toLocaleDateString(LOCALES[i18n.locale].bcp47, {
      month: "short",
      day: "numeric",
      timeZone: "UTC",
    });
  }

  function barHeight(count: number): string {
    if (count <= 0) return "0%";
    return `${Math.max(6, Math.round((count / forecastMax) * 100))}%`;
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
      <div class="head-actions">
        {#if queueLoaded && due.length > 0}
          <button
            class="btn btn-primary"
            title={t("review.startHint")}
            onclick={() => void startReviewSession()}
          >
            <Icon name="calendar-clock" size={14} />
            {t("review.start")}
          </button>
        {/if}
        <button class="btn" onclick={() => void goAll()}>
          <Icon name="files" size={14} />
          {t("review.allPages")}
        </button>
      </div>
    </header>

    {#if statsError && !shownStats}
      <div class="notice notice-error" role="alert">
        <Icon name="info" size={16} />
        <span>{t("review.statsUnavailable", { error: statsError })}</span>
        <button class="btn btn-sm" onclick={load}>{t("boot.retry")}</button>
      </div>
    {:else if shownStats}
      <dl class="stats">
        <div class="stat">
          <dt class="stat-l">{t("review.stat.due")}</dt>
          <dd class="stat-n">{shownStats.dueTotal}</dd>
        </div>
        <div class="stat">
          <dt class="stat-l">{t("review.stat.overdue")}</dt>
          <dd class="stat-n" class:hot={shownStats.overdue > 0}>{shownStats.overdue}</dd>
        </div>
        <div class="stat">
          <dt class="stat-l">{t("review.stat.today")}</dt>
          <dd class="stat-n">{shownStats.reviewedToday}</dd>
        </div>
        <div class="stat">
          <dt class="stat-l">{t("review.stat.total")}</dt>
          <dd class="stat-n">{shownStats.totalReviews}</dd>
        </div>
      </dl>
      {#if statsError}
        <div class="notice notice-warn" role="status">
          <span>{t("review.statsUnavailable", { error: statsError })}</span>
          <button class="btn btn-xs" onclick={load}>{t("boot.retry")}</button>
        </div>
      {/if}
    {/if}

    {#if !queueLoaded}
      <div class="list" role="status" aria-label={t("review.loading")}>
        {#each [0, 1, 2] as i (i)}
          <div class="skel-row" aria-hidden="true"></div>
        {/each}
      </div>
    {:else if queueError}
      <div class="empty" role="alert">
        <span class="empty-icon"><Icon name="info" size={20} /></span>
        <strong>{t("review.loadFailed")}</strong>
        <span>{t("review.loadFailedHint")}</span>
        <span class="err-detail">{queueError}</span>
        <button class="btn btn-sm" onclick={load}>{t("boot.retry")}</button>
      </div>
    {:else if due.length === 0}
      <div class="empty">
        <span class="empty-icon"><Icon name="circle-check" size={20} /></span>
        <strong>{t("review.caughtUp")}</strong>
        <span>{t("review.caughtUpHint")}</span>
        <button class="btn btn-sm" onclick={() => void goAll()}>{t("review.browse")}</button>
      </div>
    {:else}
      {#if shownStats && due.length < shownStats.dueTotal}
        <p class="capped">
          {t("review.queueCapped", { shown: due.length, total: shownStats.dueTotal })}
        </p>
      {/if}
      <p id="queue-keys" class="sr-only">{t("review.queueKeys")}</p>
      <div
        class="list"
        role="list"
        aria-label={t("review.queueLabel")}
        aria-describedby="queue-keys"
        bind:this={listEl}
      >
        {#each due as p, i (p.id)}
          {@const d = dueInfo(p.nextReview)}
          {@const hist = p.ext?.review}
          <article class="row" use:reveal role="listitem">
            <div class="row-main">
              <button
                class="row-open"
                tabindex={i === active ? 0 : -1}
                onfocus={() => (active = i)}
                onkeydown={(e) => onRowKey(e, i)}
                onclick={() => void openPage(p.id)}
              >
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
                  <span class="chip" class:chip-danger={d.overdue} class:chip-warn={!d.overdue}
                    >{d.label}</span
                  >
                {/if}
                {#if hist && hist.count > 0}
                  <button
                    class="btn btn-xs"
                    aria-expanded={!!expanded[p.id]}
                    aria-controls={`hist-${p.id}`}
                    onclick={() => toggleHistory(p.id)}
                  >
                    {expanded[p.id] ? t("review.historyHide") : t("review.historyShow")}
                  </button>
                {/if}
                <span class="resched" role="group" aria-label={t("review.reschedule")}>
                  <PresetButtons onpick={(m) => reschedule(p.id, m)} disabled={busyId === p.id} />
                </span>
                <button class="btn btn-sm btn-primary" onclick={() => void openPage(p.id)}>
                  {t("common.open")}
                </button>
              </span>
            </div>
            {#if expanded[p.id]}
              <div
                class="hist"
                id={`hist-${p.id}`}
                role="region"
                aria-label={t("review.historyTitle", {
                  title: p.title || t("common.untitled"),
                })}
              >
                {#if hist && hist.log.length > 0}
                  <p class="hist-count">{t("review.historyCount", { count: hist.count })}</p>
                  <ul class="hist-log">
                    {#each [...hist.log].reverse() as entry, k (k)}
                      <li>
                        {t("review.logEntry", {
                          when: fmtDateTime(entry.at),
                          grade: gradeLabel(entry.grade),
                          interval: fmtDuration(entry.intervalMinutes),
                        })}
                      </li>
                    {/each}
                  </ul>
                  {#if hist.count > hist.log.length}
                    <p class="hist-more">
                      {t("review.historyLimit", {
                        shown: hist.log.length,
                        count: hist.count,
                      })}
                    </p>
                  {/if}
                {:else}
                  <p class="hist-count muted">{t("review.noHistory")}</p>
                {/if}
              </div>
            {/if}
          </article>
        {/each}
      </div>
    {/if}

    {#if shownStats}
    <section class="forecast card">
      <header class="fc-head">
        <div class="fc-text">
          <h2 class="fc-title">{t("review.forecast.title")}</h2>
          <p class="fc-hint">{t("review.forecast.hint")}</p>
        </div>
        <div
          class="tabs"
          role="tablist"
          aria-label={t("review.forecast.title")}
          tabindex="-1"
          onkeydown={onTabKey}
        >
          <button
            id="forecast-tab-chart"
            class="tab"
            class:active={forecastView === "chart"}
            role="tab"
            aria-selected={forecastView === "chart"}
            aria-controls="forecast-panel"
            tabindex={forecastView === "chart" ? 0 : -1}
            onclick={() => (forecastView = "chart")}
          >
            {t("review.forecast.chart")}
          </button>
          <button
            id="forecast-tab-table"
            class="tab"
            class:active={forecastView === "table"}
            role="tab"
            aria-selected={forecastView === "table"}
            aria-controls="forecast-panel"
            tabindex={forecastView === "table" ? 0 : -1}
            onclick={() => (forecastView = "table")}
          >
            {t("review.forecast.table")}
          </button>
        </div>
      </header>
      <div
        id="forecast-panel"
        class="fc-body"
        role="tabpanel"
        aria-labelledby={`forecast-tab-${forecastView}`}
      >
        {#if upcoming.length === 0}
          <p class="fc-empty muted">{t("review.forecast.empty")}</p>
        {:else if forecastView === "chart"}
          <div
            class="chart"
            role="list"
            aria-label={t("review.forecast.chartLabel", { total: forecastTotal })}
          >
            {#each upcoming as day (day.day)}
              <div
                class="bar-col"
                role="listitem"
                aria-label={`${fmtDay(day.day)}: ${plural(day.count, "page")}`}
              >
                <span class="bar-count" aria-hidden="true">{day.count}</span>
                <span class="bar-track" aria-hidden="true">
                  <span class="bar" style={`height:${barHeight(day.count)}`}></span>
                </span>
                <span class="bar-day" aria-hidden="true">{fmtDay(day.day)}</span>
              </div>
            {/each}
          </div>
        {:else}
          <table class="fc-table">
            <caption class="sr-only">{t("review.forecast.title")}</caption>
            <thead>
              <tr>
                <th scope="col">{t("review.forecast.day")}</th>
                <th scope="col">{t("review.forecast.pages")}</th>
              </tr>
            </thead>
            <tbody>
              {#each upcoming as day (day.day)}
                <tr>
                  <th scope="row">{fmtDay(day.day)}</th>
                  <td>{day.count}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    </section>

    <section class="activity card">
      <header class="fc-head">
        <div class="fc-text">
          <h2 class="fc-title">{t("review.activity.title")}</h2>
          <p class="fc-hint">{t("review.activity.hint", { total: activityTotal })}</p>
        </div>
        <dl class="streaks">
          <div><dt>{t("review.activity.streak")}</dt><dd>{plural(shownStats.streak, "day")}</dd></div>
          <div><dt>{t("review.activity.longest")}</dt><dd>{plural(shownStats.longestStreak, "day")}</dd></div>
        </dl>
      </header>
      {#if activityTotal === 0}
        <p class="fc-empty muted">{t("review.activity.empty")}</p>
      {:else}
        <div
          class="heatmap"
          role="img"
          aria-label={t("review.activity.label", {
            total: activityTotal,
            streak: shownStats.streak,
            longest: shownStats.longestStreak,
          })}
        >
          {#each activityWeeks as week, w (w)}
            <div class="hm-week">
              {#each week as cell, i (i)}
                {#if cell}
                  <span
                    class="hm-cell l{level(cell.count)}"
                    title={`${fmtDay(cell.day)}: ${plural(cell.count, "review")}`}
                  ></span>
                {:else}
                  <span class="hm-cell blank"></span>
                {/if}
              {/each}
            </div>
          {/each}
        </div>
        <div class="hm-legend" aria-hidden="true">
          <span>{t("review.activity.less")}</span>
          {#each [0, 1, 2, 3, 4] as l (l)}<span class="hm-cell l{l}"></span>{/each}
          <span>{t("review.activity.more")}</span>
        </div>
      {/if}
    </section>
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
  .head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  h1 {
    font-size: var(--fs-4xl);
  }
  .sub {
    font-size: var(--fs-base);
    color: var(--muted);
    max-width: 560px;
  }

  /* ------------------------------------------------------------- statistics */

  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 12px;
    margin: 0;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .stat-l {
    font-size: var(--fs-2xs);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .stat-n {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-3xl);
    font-weight: 500;
    line-height: 1;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .stat-n.hot {
    color: var(--danger);
  }

  .notice {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-size: var(--fs-sm);
  }
  .notice-error {
    border-color: var(--danger-border);
    background: var(--danger-soft);
    color: var(--danger);
  }
  .notice-warn {
    border-color: var(--warn-border);
    background: var(--warn-soft);
    color: var(--warn);
  }
  .notice > span {
    flex: 1;
    min-width: 0;
  }

  .capped {
    font-size: var(--fs-xs);
    color: var(--muted);
    margin: -6px 0 0;
  }

  /* ------------------------------------------------------------------ queue */

  .list {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
  }

  .row {
    border-bottom: 1px solid var(--border);
    transition: background var(--t-med) var(--ease-out);
  }
  .row:hover {
    background: var(--surface);
  }
  .row-main {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-right: 8px;
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
  .row-open:focus-visible {
    outline: 2px solid var(--accent-strong);
    outline-offset: -2px;
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
    font-size: var(--fs-base);
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
    font-size: var(--fs-xs);
  }
  .date {
    font-family: var(--mono);
    font-size: var(--fs-2xs);
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

  /* ---------------------------------------------------------------- history */

  .hist {
    padding: 4px 8px 16px 52px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .hist-count {
    font-size: var(--fs-xs);
    color: var(--muted);
    margin: 0;
  }
  .hist-log {
    margin: 0;
    padding-left: 16px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--fs-xs);
    color: var(--muted);
    font-family: var(--mono);
  }
  .hist-more {
    font-size: var(--fs-2xs);
    color: var(--muted);
    margin: 0;
  }

  .err-detail {
    font-family: var(--mono);
    font-size: var(--fs-xs);
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  /* --------------------------------------------------------------- forecast */

  .forecast {
    padding: 20px 22px 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .fc-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  .fc-text {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .fc-title {
    font-size: var(--fs-lg);
    font-weight: 500;
    margin: 0;
  }
  .fc-hint {
    font-size: var(--fs-xs);
    color: var(--muted);
    margin: 0;
  }

  .tabs {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--raised);
  }
  .tab {
    padding: 4px 12px;
    border-radius: var(--radius-xs);
    font-size: var(--fs-xs);
    color: var(--muted);
    transition:
      background var(--t-fast) var(--ease-out),
      color var(--t-fast) var(--ease-out);
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.active {
    background: var(--surface);
    color: var(--accent-strong);
    box-shadow: 0 0 0 1px var(--border);
  }

  .chart {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    min-height: 132px;
  }
  .bar-col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .bar-count {
    font-family: var(--mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .bar-track {
    width: 100%;
    height: 92px;
    display: flex;
    align-items: flex-end;
  }
  .bar {
    width: 100%;
    min-height: 0;
    border-radius: var(--radius-xs) var(--radius-xs) 0 0;
    background: var(--leaf);
    transition: height var(--t-med) var(--ease-out);
  }
  .bar-day {
    font-size: var(--fs-2xs);
    color: var(--muted);
    white-space: nowrap;
  }

  .activity {
    padding: 20px 22px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .streaks {
    display: flex;
    gap: 20px;
    margin: 0;
  }
  .streaks div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .streaks dt {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .streaks dd {
    margin: 0;
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--text);
  }
  .heatmap {
    display: flex;
    gap: 2px;
    overflow-x: auto;
    padding-bottom: 2px;
  }
  .hm-week {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hm-cell {
    display: block;
    width: 11px;
    height: 11px;
    border-radius: 2px;
    background: var(--sunken);
    flex: none;
  }
  .hm-cell.blank {
    background: transparent;
  }
  /* One hue, light to dark: the share of the busiest day. */
  .hm-cell.l1 {
    background: color-mix(in oklab, var(--leaf) 30%, var(--surface));
  }
  .hm-cell.l2 {
    background: color-mix(in oklab, var(--leaf) 55%, var(--surface));
  }
  .hm-cell.l3 {
    background: color-mix(in oklab, var(--leaf) 78%, var(--surface));
  }
  .hm-cell.l4 {
    background: var(--leaf);
  }
  .hm-legend {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .hm-legend span:first-child {
    margin-right: 4px;
  }
  .hm-legend span:last-child {
    margin-left: 4px;
  }

  .fc-empty {
    font-size: var(--fs-sm);
    margin: 0;
  }

  .fc-table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--fs-sm);
  }
  .fc-table th,
  .fc-table td {
    text-align: left;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  .fc-table thead th {
    font-size: var(--fs-2xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
    font-weight: 500;
  }
  .fc-table tbody th {
    font-weight: 400;
    color: var(--text);
  }
  .fc-table td {
    font-family: var(--mono);
    color: var(--muted);
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
