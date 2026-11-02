<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api } from "../lib/api";
  import {
    app,
    errorMessage,
    fmtDay,
    goAll,
    nextDueLine,
    openPage,
    refreshAll,
    session,
    startReviewSession,
    toast,
  } from "../lib/state.svelte";
  import { dueInfo, fmtDate, fmtDateTime, fmtDuration, plural } from "../lib/format";
  import type { PageMeta, ReviewGrade, ReviewStats } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import Illustration from "../lib/Illustration.svelte";
  import { reducedMotion } from "../lib/motion";
  import { reveal } from "../lib/reveal";
  import PresetButtons from "./PresetButtons.svelte";
  import SessionComplete from "./SessionComplete.svelte";
  import VitalityLeaf from "./VitalityLeaf.svelte";
  import { t } from "../lib/i18n.svelte";

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

  /** A number that counts up to its value (500ms); instant under reduced motion. */
  function countUp(get: () => number) {
    let shown = $state(0);
    $effect(() => {
      const to = get();
      const from = untrack(() => shown);
      if (from === to || reducedMotion()) {
        shown = to;
        return;
      }
      const start = performance.now();
      let frame: number = requestAnimationFrame(function step(now) {
        const k = Math.min(1, (now - start) / 500);
        shown = from + (to - from) * (1 - Math.pow(1 - k, 3));
        if (k < 1) frame = requestAnimationFrame(step);
      });
      return () => cancelAnimationFrame(frame);
    });
    return {
      get value() {
        return Math.round(shown);
      },
    };
  }

  const nDue = countUp(() => shownStats?.dueTotal ?? 0);
  const nOverdue = countUp(() => shownStats?.overdue ?? 0);
  const nToday = countUp(() => shownStats?.reviewedToday ?? 0);
  const nTotal = countUp(() => shownStats?.totalReviews ?? 0);

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

  /** Activity heatmap ("the garden"): columns are weeks (Monday first, UTC), oldest left. */
  const activity = $derived(shownStats?.activity ?? []);
  const activityTotal = $derived(activity.reduce((sum, d) => sum + d.count, 0));
  const activityMax = $derived(Math.max(0, ...activity.map((d) => d.count)));
  type Cell = { day: string; count: number; i: number };
  const activityWeeks = $derived.by(() => {
    if (activity.length === 0) return [] as Array<Array<Cell | null>>;
    const [y, m, d] = activity[0].day.split("-").map(Number);
    // getUTCDay: 0 = Sunday; shift so Monday is row 0.
    const lead = (new Date(Date.UTC(y, m - 1, d)).getUTCDay() + 6) % 7;
    const cells: Array<Cell | null> = [...Array(lead).fill(null), ...activity.map((a, i) => ({ ...a, i }))];
    const weeks = [];
    for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
    return weeks;
  });

  /** 0 for no reviews, else 1..4 by share of the busiest day. */
  function level(count: number): number {
    if (count === 0 || activityMax === 0) return 0;
    return Math.min(4, Math.max(1, Math.ceil((count / activityMax) * 4)));
  }

  // One roving tab stop for the whole garden; arrows move between days.
  let heatEl = $state<HTMLDivElement | null>(null);
  let heatWrap = $state<HTMLDivElement | null>(null);
  let heatFocus = $state(-1);
  const heatActive = $derived(heatFocus >= 0 ? Math.min(heatFocus, activity.length - 1) : activity.length - 1);
  let tip = $state<{ text: string; x: number; y: number } | null>(null);

  function showTip(el: HTMLElement, cell: Cell) {
    const wrap = heatWrap?.getBoundingClientRect();
    if (!wrap) return;
    const box = el.getBoundingClientRect();
    tip = {
      text: `${fmtDay(cell.day)} · ${plural(cell.count, "review")}`,
      x: box.left - wrap.left + box.width / 2,
      y: box.top - wrap.top,
    };
  }

  function onCellKey(e: KeyboardEvent, i: number) {
    const step =
      e.key === "ArrowLeft" ? -7 : e.key === "ArrowRight" ? 7 : e.key === "ArrowUp" ? -1 : e.key === "ArrowDown" ? 1
      : e.key === "Home" ? -i : e.key === "End" ? activity.length - 1 - i : 0;
    if (!step) return;
    e.preventDefault();
    const next = Math.max(0, Math.min(activity.length - 1, i + step));
    heatFocus = next;
    heatEl?.querySelector<HTMLElement>(`[data-i="${next}"]`)?.focus();
  }

  const utcToday = new Date().toISOString().slice(0, 10);

  // R starts a session, as on the Today board.
  function onKey(e: KeyboardEvent) {
    if (e.key.toLowerCase() !== "r" || e.repeat || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
    if (app.view !== "review" || app.readId || !queueLoaded || due.length === 0) return;
    if (app.paletteOpen || app.importOpen || document.querySelector('[aria-modal="true"]')) return;
    const el = e.target as HTMLElement | null;
    if (el?.closest("input, textarea, select, [contenteditable='true']")) return;
    e.preventDefault();
    void startReviewSession();
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  function barHeight(count: number): string {
    if (count <= 0) return "0%";
    return `${Math.max(6, Math.round((count / forecastMax) * 100))}%`;
  }
</script>

{#if session.summary}
  <SessionComplete />
{:else}
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
            <span class="kbd">R</span>
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
          <dd class="stat-n" class:zero={shownStats.dueTotal === 0}>{nDue.value}</dd>
        </div>
        <div class="stat">
          <dt class="stat-l">{t("review.stat.overdue")}</dt>
          <dd class="stat-n" class:hot={shownStats.overdue > 0} class:zero={shownStats.overdue === 0}>{nOverdue.value}</dd>
        </div>
        <div class="stat">
          <dt class="stat-l">{t("review.stat.today")}</dt>
          <dd class="stat-n" class:zero={shownStats.reviewedToday === 0}>{nToday.value}</dd>
        </div>
        <div class="stat">
          <dt class="stat-l">{t("review.stat.total")}</dt>
          <dd class="stat-n" class:zero={shownStats.totalReviews === 0}>{nTotal.value}</dd>
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
      <div class="empty caught">
        <Illustration name="watering-can" size={120} animate />
        <strong>{t("review.caughtUp")}</strong>
        <span>{t("review.caughtUpHint")}</span>
        {#if shownStats}<span class="caught-next">{nextDueLine(shownStats)}</span>{/if}
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
                <span class="thumb"><VitalityLeaf meta={p} size={18} /></span>
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
            {#each upcoming as day, k (day.day)}
              <div
                class="bar-col"
                class:today={day.day === utcToday}
                role="listitem"
                aria-label={`${fmtDay(day.day)}: ${plural(day.count, "page")}`}
              >
                <span class="bar-count" aria-hidden="true">{day.count}</span>
                <span class="bar-track" aria-hidden="true">
                  <span class="bar" style:height={barHeight(day.count)} style:--k={k}></span>
                </span>
                <span class="bar-day" aria-hidden="true">
                  {day.day === utcToday ? t("review.forecast.todayTag") : fmtDay(day.day)}
                </span>
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
        <div class="fc-side">
          {#if shownStats.streak > 0}
            <p class="streak-callout">
              <Icon name="leaf" size={13} />
              {t("review.streak.line", { count: shownStats.streak })}
              {#if shownStats.streak > 1 && shownStats.streak === shownStats.longestStreak}
                <span class="best">{t("review.streak.best")}</span>
              {/if}
            </p>
          {/if}
          <dl class="streaks">
            <div><dt>{t("review.activity.longest")}</dt><dd>{plural(shownStats.longestStreak, "day")}</dd></div>
          </dl>
        </div>
      </header>
      {#if activityTotal === 0}
        <p class="fc-empty muted">{t("review.activity.empty")}</p>
      {:else}
        <div class="heat-wrap" bind:this={heatWrap}>
          <div
            class="heatmap"
            role="group"
            aria-label={t("review.activity.label", {
              total: activityTotal,
              streak: shownStats.streak,
              longest: shownStats.longestStreak,
            })}
            bind:this={heatEl}
            onscroll={() => (tip = null)}
          >
            {#each activityWeeks as week, w (w)}
              <div class="hm-week">
                {#each week as cell, i (i)}
                  {#if cell}
                    <button
                      class="hm-cell l{level(cell.count)}"
                      class:today={cell.day === utcToday}
                      data-i={cell.i}
                      tabindex={cell.i === heatActive ? 0 : -1}
                      aria-label={`${fmtDay(cell.day)}: ${plural(cell.count, "review")}`}
                      onmouseenter={(e) => showTip(e.currentTarget, cell)}
                      onmouseleave={() => (tip = null)}
                      onfocus={(e) => {
                        heatFocus = cell.i;
                        showTip(e.currentTarget, cell);
                      }}
                      onblur={() => (tip = null)}
                      onkeydown={(e) => onCellKey(e, cell.i)}
                    ></button>
                  {:else}
                    <span class="hm-cell blank"></span>
                  {/if}
                {/each}
              </div>
            {/each}
          </div>
          {#if tip}
            <div class="hm-tip" role="tooltip" style:left={`${tip.x}px`} style:top={`${tip.y}px`}>{tip.text}</div>
          {/if}
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
{/if}

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
    font-size: var(--fs-3xl);
  }
  .sub {
    font-size: var(--fs-base);
    color: var(--muted);
    max-width: 560px;
  }

  /* ------------------------------------------------------------- statistics */

  /* One strip, hairlines between the figures. */
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
    margin: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    overflow: hidden;
  }
  .stat {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 6px;
    padding: 12px 18px;
    box-shadow: -1px 0 0 var(--border);
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
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-2xl);
    font-weight: 500;
    line-height: 1;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .stat-n.zero {
    color: var(--faint, var(--muted));
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
    background: var(--sunken);
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
    font-variation-settings: var(--font-mono-settings);
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
    font-variation-settings: var(--font-mono-settings);
  }
  .hist-more {
    font-size: var(--fs-2xs);
    color: var(--muted);
    margin: 0;
  }

  .err-detail {
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
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
    font-variation-settings: var(--font-mono-settings);
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
    background: color-mix(in oklab, var(--c-sage) 45%, var(--surface));
    transform-origin: bottom;
    /* Staggered rise, one bar after the next. */
    animation: bar-rise 420ms var(--ease-out) calc(var(--k) * 35ms) backwards;
  }
  .bar-col.today .bar {
    background: var(--c-sage);
  }
  .bar-col.today .bar-day {
    color: var(--text);
    font-weight: 500;
  }
  @keyframes bar-rise {
    from {
      transform: scaleY(0);
    }
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
  .fc-side {
    display: flex;
    align-items: flex-start;
    gap: 20px;
    flex-wrap: wrap;
  }
  .streak-callout {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    background: var(--c-sage-soft);
    color: var(--c-sage);
    font-size: var(--fs-xs);
    font-weight: 500;
  }
  .streak-callout .best {
    font-weight: 400;
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
  .heat-wrap {
    position: relative;
  }
  .heatmap {
    display: flex;
    gap: 3px;
    overflow-x: auto;
    padding: 2px 2px 4px;
  }
  .hm-week {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .hm-cell {
    display: block;
    width: 11px;
    height: 11px;
    padding: 0;
    border: 0;
    border-radius: 2px;
    background: var(--sunken);
    flex: none;
  }
  button.hm-cell {
    cursor: default;
    transition: transform var(--t-fast) var(--ease-out);
  }
  button.hm-cell:hover,
  button.hm-cell:focus-visible {
    transform: scale(1.35);
  }
  button.hm-cell:focus-visible {
    outline: 2px solid var(--accent-strong);
    outline-offset: 1px;
    border-radius: 2px;
  }
  .hm-cell.today {
    box-shadow: 0 0 0 1px var(--text-soft);
  }
  .hm-cell.blank {
    background: transparent;
  }
  /* One hue, stepped from bare soil to full green: the share of the busiest day. */
  .hm-cell.l1 {
    background: color-mix(in oklab, var(--c-sage) 28%, var(--sunken));
  }
  .hm-cell.l2 {
    background: color-mix(in oklab, var(--c-sage) 50%, var(--sunken));
  }
  .hm-cell.l3 {
    background: color-mix(in oklab, var(--c-sage) 75%, var(--sunken));
  }
  .hm-cell.l4 {
    background: var(--c-sage);
  }
  .hm-tip {
    position: absolute;
    z-index: 2;
    transform: translate(-50%, calc(-100% - 8px));
    padding: 4px 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--raised);
    box-shadow: var(--shadow);
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-2xs);
    color: var(--text);
    white-space: nowrap;
    pointer-events: none;
    animation: tip-in var(--t-fast) var(--ease-out);
  }
  @keyframes tip-in {
    from {
      opacity: 0;
    }
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

  .caught {
    padding: 48px 20px 56px;
  }
  .caught :global(svg) {
    margin-bottom: 8px;
  }
  .caught-next {
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-xs);
    color: var(--muted);
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
    font-variation-settings: var(--font-mono-settings);
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
