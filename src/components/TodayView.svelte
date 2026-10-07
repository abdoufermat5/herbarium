<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import {
    app,
    errorMessage,
    goView,
    openInPath,
    openPage,
    reloadPages,
    startReviewSession,
    toast,
  } from "../lib/state.svelte";
  import { fmtDate, plural, timeAgo } from "../lib/format";
  import { i18n, t, LOCALES } from "../lib/i18n.svelte";
  import type { PageMeta, TodaySummary } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import PageIcon from "./PageIcon.svelte";

  let summary = $state<TodaySummary | null>(null);
  let error = $state<string | null>(null);

  const now = new Date();
  const hour = now.getHours();
  const greeting = $derived(
    hour < 5 ? t("today.evening") : hour < 12 ? t("today.morning") : hour < 18 ? t("today.afternoon") : t("today.evening"),
  );
  const dateLabel = $derived(
    now.toLocaleDateString(LOCALES[i18n.locale].bcp47, { weekday: "long", month: "long", day: "numeric" }),
  );

  async function load() {
    try {
      summary = await api.today();
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  // Refresh when the vault changes (saves, rescans, reviews elsewhere).
  $effect(() => {
    void app.vaultRevision;
    void app.library.length;
    void app.dueCount;
    void load();
  });

  async function addSamples() {
    try {
      await api.addSamples();
      await reloadPages(true);
      await load();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  function yearsAgo(p: PageMeta): string {
    const then = new Date(p.createdAt);
    const years = now.getFullYear() - then.getFullYear();
    if (years >= 1 && then.getMonth() === now.getMonth()) return t("today.yearsAgo", { count: years });
    return fmtDate(p.createdAt);
  }

  onMount(() => void load());
</script>

<div class="pane">
  <div class="inner">
    <header class="head">
      <p class="eyebrow">{dateLabel}</p>
      <h1 class="display">{greeting}</h1>
      {#if summary && summary.totalPages > 0}
        <p class="sub">{t("today.subtitle", { pages: plural(summary.totalPages, "page") })}</p>
      {/if}
    </header>

    {#if error}
      <div class="notice notice-error" role="alert">
        <Icon name="info" size={16} />
        <span>{error}</span>
        <button class="btn btn-sm" onclick={load}>{t("boot.retry")}</button>
      </div>
    {:else if summary && summary.totalPages === 0}
      <section class="card empty">
        <h2>{t("today.emptyTitle")}</h2>
        <p>{t("today.emptyText")}</p>
        <div class="ctas">
          <button class="cta" onclick={() => (app.aiImportOpen = true)}>
            <Icon name="files" size={18} />
            <span><strong>{t("today.ctaExport")}</strong><small>{t("today.ctaExportHint")}</small></span>
          </button>
          <button class="cta" onclick={() => void goView("settings")}>
            <Icon name="external-link" size={18} />
            <span><strong>{t("today.ctaBrowser")}</strong><small>{t("today.ctaBrowserHint")}</small></span>
          </button>
          <button class="cta" onclick={() => (app.importOpen = true)}>
            <Icon name="upload" size={18} />
            <span><strong>{t("today.ctaFiles")}</strong><small>{t("today.ctaFilesHint")}</small></span>
          </button>
          <button class="cta" onclick={addSamples}>
            <Icon name="leaf" size={18} />
            <span><strong>{t("today.ctaSamples")}</strong><small>{t("today.ctaSamplesHint")}</small></span>
          </button>
        </div>
      </section>
    {:else if summary}
      <div class="grid">
        <section class="card review">
          <h2 class="eyebrow">{t("sidebar.review")}</h2>
          {#if summary.dueTotal > 0}
            <p class="big"><span class="n">{summary.dueTotal}</span> {t("today.dueLabel", { count: summary.dueTotal })}</p>
            <button class="btn btn-primary" onclick={() => void startReviewSession()}>
              <Icon name="refresh-cw" size={13} />{t("review.start")}
            </button>
            <ul class="list">
              {#each summary.due as p (p.id)}
                <li><button class="row" onclick={() => void openPage(p.id)}><PageIcon page={p} /><span class="ellipsis">{p.title}</span></button></li>
              {/each}
            </ul>
          {:else}
            <p class="big calm"><Icon name="circle-check" size={18} /> {t("today.caughtUp")}</p>
          {/if}
          {#if summary.streak > 0}
            <p class="streak">{t("today.streak", { days: plural(summary.streak, "day") })}</p>
          {/if}
        </section>

        {#if summary.continue.length > 0}
          <section class="card">
            <h2 class="eyebrow">{t("today.continue")}</h2>
            <ul class="list">
              {#each summary.continue as c (c.pathId)}
                <li>
                  <button class="row two" onclick={() => void openInPath(c.pathId, c.page.id)}>
                    <span class="line"><PageIcon page={c.page} /><span class="ellipsis">{c.page.title}</span></span>
                    <span class="meta">
                      <span class="ellipsis">{c.pathName}</span>
                      <span class="progress" aria-hidden="true"><span style={`width:${((c.position) / c.total) * 100}%`}></span></span>
                      <span>{c.position + 1}/{c.total}</span>
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if summary.rediscover}
          {@const p = summary.rediscover}
          <section class="card rediscover">
            <h2 class="eyebrow">{t("today.rediscover")}</h2>
            <button class="feature" onclick={() => void openPage(p.id)}>
              <span class="feature-title"><PageIcon page={p} size={18} />{p.title}</span>
              {#if p.note}<span class="feature-note">{p.note}</span>{/if}
              <span class="meta">{t("today.savedAgo", { when: timeAgo(p.createdAt) })}{#if p.folder}{" · "}{p.folder}{/if}</span>
            </button>
          </section>
        {/if}

        {#if summary.onThisDay.length > 0}
          <section class="card">
            <h2 class="eyebrow">{t("today.onThisDay")}</h2>
            <ul class="list">
              {#each summary.onThisDay as p (p.id)}
                <li>
                  <button class="row two" onclick={() => void openPage(p.id)}>
                    <span class="line"><PageIcon page={p} /><span class="ellipsis">{p.title}</span></span>
                    <span class="meta">{yearsAgo(p)}</span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        <section class="card">
          <h2 class="eyebrow">{t("today.recent")}</h2>
          <ul class="list">
            {#each summary.recent as p (p.id)}
              <li>
                <button class="row two" onclick={() => void openPage(p.id)}>
                  <span class="line"><PageIcon page={p} /><span class="ellipsis">{p.title}</span></span>
                  <span class="meta">{timeAgo(p.createdAt)}{#if p.ext?.source?.tool}{" · "}{p.ext.source.tool}{/if}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      </div>
    {:else}
      <div class="loading"><span class="spinner"></span></div>
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
    gap: 28px;
  }
  .head .eyebrow {
    margin: 0 0 6px;
  }
  .head h1 {
    font-size: var(--fs-3xl, 34px);
    margin: 0;
  }
  .sub {
    margin: 6px 0 0;
    color: var(--muted);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 16px;
    align-items: start;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 22px;
  }
  .card h2.eyebrow {
    margin: 0;
  }
  .big {
    margin: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: var(--fs-md, 15px);
    color: var(--text-soft);
  }
  .big .n {
    font-size: 34px;
    font-weight: 600;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .big.calm {
    align-items: center;
    color: var(--leaf);
  }
  .review > .btn {
    align-self: flex-start;
  }
  .streak {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    margin: 0 -8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }
  .row:hover {
    background: var(--sunken);
  }
  .row.two {
    flex-direction: column;
    align-items: stretch;
    gap: 2px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-2xs);
    color: var(--muted);
    min-width: 0;
  }
  .progress {
    flex: 1;
    min-width: 40px;
    height: 4px;
    border-radius: 2px;
    background: var(--sunken);
    overflow: hidden;
  }
  .progress span {
    display: block;
    height: 100%;
    background: var(--leaf);
  }
  .feature {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--raised);
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .feature:hover {
    border-color: var(--leaf);
  }
  .feature-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-md, 15px);
    font-weight: 500;
  }
  .feature-note {
    font-size: var(--fs-sm);
    color: var(--text-soft);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .empty h2 {
    margin: 0;
    font-size: var(--fs-xl, 20px);
  }
  .empty p {
    margin: 0;
    color: var(--text-soft);
  }
  .ctas {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 10px;
    margin-top: 6px;
  }
  .cta {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--leaf);
    text-align: left;
    cursor: pointer;
  }
  .cta:hover {
    border-color: var(--leaf);
    background: var(--raised);
  }
  .cta span {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: var(--text);
  }
  .cta small {
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .loading {
    display: grid;
    place-items: center;
    padding: 48px;
  }
</style>
