<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api } from "../lib/api";
  import {
    app,
    errorMessage,
    openInPath,
    openPage,
    openPathView,
    reloadPages,
    startReviewSession,
    toast,
    openSettings,
    openPlantPath,
  } from "../lib/state.svelte";
  import { dateFormat, fmtDate, plural, timeAgo } from "../lib/format";
  import { i18n, t, LOCALES, type MessageKey } from "../lib/i18n.svelte";
  import type { PageMeta, ReviewStats, TodaySummary } from "../lib/types";
  import { vitality, type VitalityLevel } from "../lib/vitality";
  import { pathGrowth, wasCelebrated, markCelebrated } from "../lib/grove";
  import { celebrate } from "../lib/celebrate";
  import { treeStage, type TreeStage } from "../lib/tree";
  import { reveal } from "../lib/reveal";
  import Icon from "../lib/Icon.svelte";
  import Illustration from "../lib/Illustration.svelte";
  import PageIcon from "./PageIcon.svelte";
  import PathTree from "./PathTree.svelte";
  import VitalityLeaf from "./VitalityLeaf.svelte";

  let summary = $state<TodaySummary | null>(null);
  let stats = $state<ReviewStats | null>(null);
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
      const [nextSummary, nextStats] = await Promise.all([api.today(), api.reviewStats()]);
      summary = nextSummary;
      stats = nextStats;
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

  /* ------------------------------------------------------------- vitality */

  const LEVELS: readonly VitalityLevel[] = ["thriving", "healthy", "thirsty", "wilting", "seed"];

  const LEVEL_LABEL: Record<VitalityLevel, MessageKey> = {
    thriving: "today.levelThriving",
    healthy: "today.levelHealthy",
    thirsty: "today.levelThirsty",
    wilting: "today.levelWilting",
    seed: "today.levelSeed",
  };

  /** The same palette VitalityLeaf draws with. */
  const LEVEL_COLOR: Record<VitalityLevel, string> = {
    thriving: "var(--c-sage)",
    healthy: "var(--c-teal)",
    thirsty: "var(--c-amber)",
    wilting: "var(--c-clay)",
    seed: "var(--muted)",
  };

  const counts = $derived.by(() => {
    const c: Record<VitalityLevel, number> = { seed: 0, thriving: 0, healthy: 0, thirsty: 0, wilting: 0 };
    for (const p of app.library) c[vitality(p).level] += 1;
    return c;
  });
  const thirstyCount = $derived(counts.thirsty + counts.wilting);

  const garden = $derived.by(() => {
    return LEVELS.map((level) => ({ level, count: counts[level] })).filter((segment) => segment.count > 0);
  });

  const subline = $derived.by(() => {
    // The library can trail the summary by one refresh; fall back to the plain count.
    if (app.library.length === 0) {
      return summary ? t("today.subtitle", { pages: plural(summary.totalPages, "page") }) : "";
    }
    if (thirstyCount === 0) return t("today.allThriving");
    const thirsty = t("today.vitalityThirsty", { count: thirstyCount });
    if (counts.thriving === 0) return `${thirsty}.`;
    return `${thirsty}, ${t("today.vitalityThriving", { count: counts.thriving })}.`;
  });

  const rediscover = $derived(summary?.rediscover ?? null);
  const hasDue = $derived((summary?.dueTotal ?? 0) > 0);

  /* ------------------------------------------------------------------ grid */

  type CardKey = "review" | "streak" | "grove" | "garden" | "continue" | "rediscover" | "recent" | "day";

  /** No paths yet but enough pages to build one: the grove slot invites planting. */
  const showPlantInvite = $derived(app.paths.length === 0 && app.library.length >= 2);

  interface CardSlot {
    row: number;
    span: number;
  }

  /** Every row fills the six columns, whatever subset of cards is present. */
  const slots = $derived.by(() => {
    const s: Partial<Record<CardKey, CardSlot>> = {
      review: { row: 1, span: 4 },
      streak: { row: 1, span: 2 },
      garden: { row: 2, span: 2 },
    };
    const hasContinue = (summary?.continue.length ?? 0) > 0;
    const hasRediscover = !!summary?.rediscover;
    const hasDay = (summary?.onThisDay.length ?? 0) > 0;

    if (hasContinue) {
      s.continue = { row: 2, span: 4 };
      if (hasRediscover) {
        s.rediscover = { row: 3, span: 3 };
        s.recent = { row: 3, span: 3 };
        if (hasDay) s.day = { row: 4, span: 6 };
      } else if (hasDay) {
        s.day = { row: 3, span: 3 };
        s.recent = { row: 3, span: 3 };
      } else {
        s.recent = { row: 3, span: 6 };
      }
    } else {
      s.recent = { row: 2, span: 4 };
      if (hasRediscover && hasDay) {
        s.rediscover = { row: 3, span: 3 };
        s.day = { row: 3, span: 3 };
      } else if (hasRediscover) {
        s.rediscover = { row: 3, span: 6 };
      } else if (hasDay) {
        s.day = { row: 3, span: 6 };
      }
    }

    // The grove (or its invitation) is a band of its own, right under review
    // and streak: every later card moves down one row, so each row still fills
    // all six columns.
    if (app.paths.length > 0 || showPlantInvite) {
      for (const slot of Object.values(s)) {
        if (slot && slot.row > 1) slot.row += 1;
      }
      s.grove = { row: 2, span: 6 };
    }
    return s;
  });

  function place(key: CardKey): string {
    const slot = slots[key];
    return slot ? `--row:${slot.row};--span:${slot.span}` : "";
  }

  /* ----------------------------------------------------------------- grove */

  /** Trees that fit on the grove's ground line before it folds the rest. */
  const GROVE_MAX = 6;
  /** Height of a grove tree in px; the shared ground sits at 90% of it. */
  const GROVE_TREE = 112;
  /** Petals bloom about here: the tree's first growth wave is on screen. */
  const GROW_MS = 1200;

  interface GroveTree {
    id: string;
    name: string;
    read: number;
    total: number;
    mature: boolean;
    stage: TreeStage;
  }

  /** Most-read first, then by name, so the grove reads left to right as it grew. */
  const grove = $derived.by(() => {
    const all: GroveTree[] = app.paths
      .map((p) => {
        const g = pathGrowth(p.id);
        return {
          id: p.id,
          name: p.name,
          read: g.read,
          total: g.total,
          mature: g.mature,
          stage: treeStage(g.read, g.total),
        };
      })
      .sort((a, b) => {
        const pa = a.total > 0 ? a.read / a.total : 0;
        const pb = b.total > 0 ? b.read / b.total : 0;
        return pb - pa || a.name.localeCompare(b.name);
      });
    return {
      shown: all.slice(0, GROVE_MAX),
      hidden: Math.max(0, all.length - GROVE_MAX),
      overflow: all[GROVE_MAX]?.id ?? null,
    };
  });

  function openNextGrovePath() {
    if (grove.overflow) void openPathView(grove.overflow);
  }

  // One tree may have matured since the last visit: it grows in on arrival and
  // bursts once. Deciding at mount keeps that moment from replaying on refresh.
  const momentId = untrack(
    () => grove.shown.find((tree) => tree.mature && !wasCelebrated(tree.id))?.id ?? null,
  );
  let momentEl = $state<HTMLElement | null>(null);

  /** Remember the growing tree's box: the petals burst from its centre. */
  function trackMoment(node: HTMLElement, id: string) {
    if (id !== momentId) return;
    momentEl = node;
    return {
      destroy() {
        if (momentEl === node) momentEl = null;
      },
    };
  }

  // The grove renders after the summary arrives, so the clock starts when its
  // tree is actually on screen. The cleanup both re-arms on a new tree and
  // clears the timer if Today is left before the moment lands.
  $effect(() => {
    const el = momentEl;
    const id = momentId;
    if (!el || !id) return;
    const timer = setTimeout(() => {
      const r = el.getBoundingClientRect();
      void celebrate({ x: r.left + r.width / 2, y: r.top + r.height / 2 });
      markCelebrated(id);
    }, GROW_MS);
    return () => clearTimeout(timer);
  });

  /* ---------------------------------------------------------------- streak */

  const miniGarden = $derived(stats?.activity.slice(-28) ?? []);
  const MILESTONES: number[] = [3, 7, 14, 30, 50, 100, 365];
  const milestone = $derived(stats != null && MILESTONES.includes(stats.streak));

  /** Four weeks of reviews as a small heatmap: none, then three rising sages. */
  function dayTint(count: number): string {
    if (count <= 0) return "var(--border)";
    if (count === 1) return "var(--c-sage-soft)";
    if (count === 2) return "color-mix(in srgb, var(--c-sage) 55%, var(--c-sage-soft))";
    return "var(--c-sage)";
  }

  function weekday(day: string): string {
    return dateFormat("utcWeekday", { weekday: "narrow", timeZone: "UTC" }).format(new Date(`${day}T00:00:00Z`));
  }

  /* ------------------------------------------------------------------ ring */

  const RING_RADIUS = 35;
  const RING_LENGTH = 2 * Math.PI * RING_RADIUS;
  let ringReady = $state(false);
  let ringPlayed = false;

  // Fill the arc from empty once the first reading has rendered. The second
  // frame lets the browser paint the empty track before the transition starts.
  $effect(() => {
    if (ringPlayed || !summary || !stats) return;
    ringPlayed = true;
    requestAnimationFrame(() => requestAnimationFrame(() => (ringReady = true)));
  });

  const ringProgress = $derived.by(() => {
    const done = stats?.reviewedToday ?? 0;
    const total = done + (summary?.dueTotal ?? 0);
    return total > 0 ? done / total : 0;
  });
  const ringOffset = $derived(RING_LENGTH * (1 - ringProgress));
  const ringLabel = $derived(
    stats && summary
      ? t("today.reviewProgress", { done: stats.reviewedToday, total: stats.reviewedToday + summary.dueTotal })
      : "",
  );

  /** Count a number up from its previous value in 500ms; instant with reduced motion. */
  function countUp(node: HTMLElement, value: number) {
    const reduced =
      typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    let shown = 0;
    let frame = 0;

    function paint(n: number) {
      shown = n;
      node.textContent = String(Math.round(n));
    }

    function run(to: number) {
      cancelAnimationFrame(frame);
      if (reduced) {
        paint(to);
        return;
      }
      const from = shown;
      const start = performance.now();
      const step = (at: number) => {
        const k = Math.min(1, (at - start) / 500);
        paint(from + (to - from) * (1 - Math.pow(1 - k, 3)));
        if (k < 1) frame = requestAnimationFrame(step);
        else paint(to);
      };
      frame = requestAnimationFrame(step);
    }

    run(value);
    return {
      update(next: number) {
        run(next);
      },
      destroy() {
        cancelAnimationFrame(frame);
      },
    };
  }

  /* -------------------------------------------------------------- keyboard */

  function isTypingTarget(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null;
    if (!el) return false;
    return el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key.toLowerCase() !== "r" || e.repeat) return;
    if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
    if (isTypingTarget(e.target)) return;
    if (app.paletteOpen || app.importOpen) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    e.preventDefault();
    void startReviewSession();
  }

  onMount(() => {
    void load();
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });
</script>

<div class="pane">
  <div class="inner">
    <header class="head">
      <p class="eyebrow">{dateLabel}</p>
      <h1 class="display">{greeting}</h1>
      {#if subline}
        <p class="sub">{subline}</p>
      {/if}
    </header>

    {#if error}
      <div class="notice notice-error" role="alert">
        <Icon name="info" size={16} />
        <span>{error}</span>
        <button class="btn btn-sm" onclick={load}>{t("boot.retry")}</button>
      </div>
    {:else if summary && summary.totalPages === 0}
      <section class="card empty" use:reveal>
        <div class="empty-art">
          <Illustration name="seedling" size={104} animate />
        </div>
        <h2 class="display">{t("today.emptyTitle")}</h2>
        <p>{t("today.emptyText")}</p>
        <div class="ctas">
          <button class="cta" onclick={() => (app.aiImportOpen = true)}>
            <Icon name="files" size={18} />
            <span><strong>{t("today.ctaExport")}</strong><small>{t("today.ctaExportHint")}</small></span>
          </button>
          <button class="cta" onclick={() => void openSettings("capture")}>
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
    {:else if summary && stats}
      <div class="grid">
        <section class="card review" use:reveal style={place("review")}>
          <h2 class="eyebrow">{t("sidebar.review")}</h2>
          {#if hasDue}
            <div class="review-main">
              <div class="ring-wrap" role="img" aria-label={ringLabel}>
                <svg class="ring" width="76" height="76" viewBox="0 0 76 76" aria-hidden="true">
                  <circle class="track" cx="38" cy="38" r={RING_RADIUS} />
                  <circle
                    class="arc"
                    cx="38"
                    cy="38"
                    r={RING_RADIUS}
                    stroke-dasharray={RING_LENGTH}
                    style={`stroke-dashoffset:${ringReady ? ringOffset : RING_LENGTH}`}
                  />
                </svg>
                <span class="ring-count" use:countUp={stats.reviewedToday}></span>
              </div>
              <div class="review-side">
                <p class="due">
                  <span class="due-n" use:countUp={summary.dueTotal}></span>
                  <span class="due-label">{t("today.dueLabel", { count: summary.dueTotal })}</span>
                </p>
                <button class="btn btn-primary start" onclick={() => void startReviewSession()}>
                  {t("review.start")}
                  <span class="kbd">R</span>
                </button>
              </div>
            </div>
            <ul class="list due-list">
              {#each summary.due.slice(0, 5) as p (p.id)}
                <li>
                  <button class="row" onclick={() => void openPage(p.id)}>
                    <VitalityLeaf meta={p} size={14} />
                    <span class="ellipsis">{p.title}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {:else}
            <div class="caught">
              <Illustration name="watering-can" size={96} animate />
              <p class="caught-line display">{t("today.caughtUpText")}</p>
            </div>
          {/if}
        </section>

        <section class="card streak" use:reveal style={place("streak")}>
          <h2 class="eyebrow">{t("today.streakCard")}</h2>
          <div class="streak-figure">
            <span class="streak-n" use:countUp={stats.streak}></span>
            <span class="streak-label display">{t("today.streakLabel", { count: stats.streak })}</span>
            {#if milestone}
              <span class="chip" data-color="amber">{t("today.milestone")}</span>
            {/if}
          </div>
          <div class="mini-garden" aria-hidden="true">
            <div class="mini-head">
              {#each miniGarden.slice(0, 7) as day (day.day)}
                <span>{weekday(day.day)}</span>
              {/each}
            </div>
            <div class="mini-grid">
              {#each miniGarden as day, i (day.day)}
                <span
                  class="cell"
                  class:today={i === miniGarden.length - 1}
                  style={`background:${dayTint(day.count)}`}
                ></span>
              {/each}
            </div>
          </div>
          <p class="longest">{t("today.longestStreak", { count: stats.longestStreak })}</p>
        </section>

        {#if app.paths.length > 0}
          <section class="card grove" use:reveal style={place("grove")}>
            <h2 class="eyebrow">{t("today.grove.title")}</h2>
            <div class="grove-row" style:--grove-tree={`${GROVE_TREE}px`}>
              {#each grove.shown as tree (tree.id)}
                <button
                  class="grove-tree"
                  aria-label={t("today.grove.open", { name: tree.name })}
                  title={t("today.grove.open", { name: tree.name })}
                  onclick={() => void openPathView(tree.id)}
                >
                  <span class="grove-art" use:trackMoment={tree.id}>
                    <PathTree
                      seed={tree.id}
                      read={tree.read}
                      total={tree.total}
                      size={GROVE_TREE}
                      detail="full"
                      label={tree.name}
                      animate={tree.id === momentId}
                      growOnMount={tree.id === momentId}
                    />
                  </span>
                  <span class="grove-name ellipsis">{tree.name}</span>
                  <span class="grove-meta">
                    <span>{t(`tree.stage.${tree.stage}` as MessageKey)}</span>
                    <span>{t("today.grove.read", { read: tree.read, total: tree.total })}</span>
                  </span>
                </button>
              {/each}
              {#if grove.overflow}
                <button class="grove-more" onclick={openNextGrovePath}>
                  {t("today.grove.more", { count: grove.hidden })}
                </button>
              {/if}
            </div>
          </section>
        {:else if showPlantInvite}
          <section class="card plant-invite" use:reveal style={place("grove")}>
            <div class="plant-invite-art" aria-hidden="true">
              <Illustration name="seedling" size={96} animate />
            </div>
            <div class="plant-invite-copy">
              <h2 class="display plant-invite-title">{t("today.plantTitle")}</h2>
              <p class="plant-invite-body">{t("today.plantBody")}</p>
            </div>
            <button type="button" class="btn btn-primary plant-invite-cta" onclick={() => openPlantPath()}>
              {t("today.plantCta")}
              <span class="kbd">P</span>
            </button>
          </section>
        {/if}

        <section class="card garden" use:reveal style={place("garden")}>
          <h2 class="eyebrow">{t("today.garden")}</h2>
          <div class="bar" aria-hidden="true">
            {#each garden as segment (segment.level)}
              <span
                class="seg"
                style={`flex: ${segment.count} 1 0; background: ${LEVEL_COLOR[segment.level]}`}
              ></span>
            {/each}
          </div>
          <ul class="legend">
            {#each garden as segment (segment.level)}
              <li>
                <span class="swatch" style={`background: ${LEVEL_COLOR[segment.level]}`}></span>
                <span class="ellipsis">{t(LEVEL_LABEL[segment.level])}</span>
                <span class="legend-count">{segment.count}</span>
              </li>
            {/each}
          </ul>
          {#if thirstyCount > 0}
            <button class="btn btn-sm water" onclick={() => void startReviewSession()}>
              {t("today.water")}
            </button>
          {/if}
        </section>

        {#if summary.continue.length > 0}
          <section class="card continue" use:reveal style={place("continue")}>
            <h2 class="eyebrow">{t("today.continue")}</h2>
            <ul class="list">
              {#each summary.continue as c (c.pathId)}
                {@const growth = pathGrowth(c.pathId)}
                <li>
                  <button class="row with-tree" onclick={() => void openInPath(c.pathId, c.page.id)}>
                    <span class="row-tree" aria-hidden="true">
                      <PathTree detail="mini" size={24} seed={c.pathId} read={growth.read} total={growth.total} animate={false} />
                    </span>
                    <span class="row-text">
                      <span class="line"><PageIcon page={c.page} /><span class="ellipsis">{c.page.title}</span></span>
                      <span class="meta">
                        <span class="ellipsis">{c.pathName}</span>
                        <span class="progress" aria-hidden="true">
                          <span
                            class="progress-fill"
                            style={`transform: scaleX(${c.total > 0 ? c.position / c.total : 0})`}
                          ></span>
                        </span>
                        <span class="mono">{c.position + 1}/{c.total}</span>
                      </span>
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if rediscover}
          <section class="card rediscover" use:reveal style={place("rediscover")}>
            <h2 class="eyebrow">{t("today.rediscover")}</h2>
            <h3 class="feature-title display">
              <VitalityLeaf meta={rediscover} size={14} />
              <span class="ellipsis">{rediscover.title}</span>
            </h3>
            {#if rediscover.note}<p class="feature-note">{rediscover.note}</p>{/if}
            <p class="meta">
              <span>{t("today.savedAgo", { when: timeAgo(rediscover.createdAt) })}</span>
              {#if rediscover.folder}<span aria-hidden="true">·</span><span class="ellipsis">{rediscover.folder}</span>{/if}
            </p>
            <button class="btn btn-sm open" onclick={() => void openPage(rediscover.id)}>{t("common.open")}</button>
          </section>
        {/if}

        <section class="card recent" use:reveal style={place("recent")}>
          <h2 class="eyebrow">{t("today.recent")}</h2>
          <ul class="list">
            {#each summary.recent as p (p.id)}
              <li>
                <button class="row two" onclick={() => void openPage(p.id)}>
                  <span class="line">
                    <PageIcon page={p} />
                    <span class="ellipsis">{p.title}</span>
                    <VitalityLeaf meta={p} size={14} />
                  </span>
                  <span class="meta">{timeAgo(p.createdAt)}{#if p.ext?.source?.tool}{" · "}{p.ext.source.tool}{/if}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>

        {#if summary.onThisDay.length > 0}
          <section class="card day" use:reveal style={place("day")}>
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
    container: today / inline-size;
  }
  .head .eyebrow {
    margin: 0 0 6px;
  }
  .head h1 {
    font-size: var(--fs-3xl);
    margin: 0;
  }
  .sub {
    margin: 8px 0 0;
    color: var(--text-soft);
    font-size: var(--fs-base);
  }

  /* ---------------------------------------------------------------- grid */
  .grid {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 16px;
    align-items: stretch;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 24px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    min-width: 0;
    grid-row: var(--row, auto);
    grid-column: span var(--span, 3);
  }
  .card h2.eyebrow {
    margin: 0;
  }

  @container today (max-width: 900px) {
    .grid {
      grid-template-columns: 1fr;
    }
    .card {
      grid-row: auto;
      grid-column: 1 / -1;
    }
  }

  /* --------------------------------------------------------------- review */
  .review-main {
    display: flex;
    align-items: center;
    gap: 20px;
  }
  .ring-wrap {
    position: relative;
    width: 76px;
    height: 76px;
    flex: none;
  }
  .ring {
    display: block;
    transform: rotate(-90deg);
  }
  .ring .track {
    fill: none;
    stroke: var(--border);
    stroke-width: 6;
  }
  .ring .arc {
    fill: none;
    stroke: var(--c-sage);
    stroke-width: 6;
    stroke-linecap: round;
    transition: stroke-dashoffset var(--t-slow) var(--ease-out);
  }
  .ring-count {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    font-family: var(--mono);
    font-size: var(--fs-lg);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }
  .review-side {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    min-width: 0;
  }
  .due {
    margin: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
  }
  .due-n {
    font-family: var(--mono);
    font-size: 34px;
    font-weight: 500;
    line-height: 1;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }
  .due-label {
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .review .due-list {
    margin-top: auto;
  }
  .caught {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    padding: 8px 0 12px;
    text-align: center;
  }
  .caught-line {
    margin: 0;
    font-size: var(--fs-lg);
    color: var(--text-soft);
  }

  /* --------------------------------------------------------------- streak */
  .streak-figure {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 6px 10px;
  }
  .streak-n {
    font-family: var(--mono);
    font-size: 44px;
    font-weight: 500;
    line-height: 1;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }
  .streak-label {
    font-size: var(--fs-md);
    color: var(--text-soft);
  }
  .streak .chip {
    align-self: center;
  }
  .mini-garden {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .mini-head,
  .mini-grid {
    display: grid;
    grid-template-columns: repeat(7, 10px);
    gap: 3px;
  }
  .mini-head span {
    font-family: var(--mono);
    font-size: var(--fs-2xs);
    line-height: 1;
    text-align: center;
    color: var(--muted);
  }
  .mini-grid {
    grid-auto-rows: 10px;
  }
  .cell {
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }
  .cell.today {
    outline: 1px solid var(--text-soft);
    outline-offset: 2px;
  }
  .longest {
    margin: auto 0 0;
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  /* --------------------------------------------------------------- garden */
  .bar {
    display: flex;
    gap: 2px;
    height: 8px;
    border-radius: 4px;
    overflow: hidden;
    background: var(--sunken);
  }
  .seg {
    height: 100%;
    min-width: 2px;
  }
  .legend {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px 12px;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: var(--fs-xs);
    color: var(--text-soft);
  }
  .swatch {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex: none;
  }
  .legend-count {
    margin-left: auto;
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }
  .water {
    align-self: flex-start;
    margin-top: auto;
  }

  /* ---------------------------------------------------------------- grove */
  .grove-row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: center;
  }
  .grove-tree {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    flex: 1 1 116px;
    max-width: 200px;
    margin: 0;
    padding: 0 8px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  /* One continuous ground line: each tree's segment meets its neighbours'. */
  .grove-tree::before {
    content: "";
    position: absolute;
    top: calc(var(--grove-tree, 112px) * 0.9);
    left: 0;
    right: 0;
    height: 1px;
    background: var(--border);
  }
  .grove-art {
    display: flex;
    align-items: flex-end;
    justify-content: center;
    height: var(--grove-tree, 112px);
    transition: transform var(--t-fast) var(--ease-out);
  }
  .grove-tree:hover .grove-art {
    transform: translateY(-2px);
  }
  .grove-name {
    max-width: 100%;
    margin-top: 4px;
    font-size: var(--fs-sm);
    line-height: 1.35;
  }
  .grove-meta {
    display: flex;
    align-items: baseline;
    justify-content: center;
    gap: 6px;
    max-width: 100%;
    margin-top: 2px;
    font-family: var(--mono);
    font-size: var(--fs-2xs);
    font-variant-numeric: tabular-nums;
    color: var(--muted);
    white-space: nowrap;
  }
  .grove-more {
    align-self: flex-end;
    flex: none;
    margin: 0 8px 10px;
    padding: 5px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-soft);
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .grove-more:hover {
    border-color: var(--border-hover);
    background: var(--raised);
  }

  /* ------------------------------------------------------ first path invite */
  .plant-invite {
    flex-direction: row;
    align-items: center;
    gap: 24px;
  }
  .plant-invite-art {
    flex: none;
    display: grid;
    place-items: center;
    width: 96px;
    height: 96px;
  }
  .plant-invite-copy {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .plant-invite-title {
    margin: 0;
    font-size: var(--fs-xl);
  }
  .plant-invite-body {
    margin: 0;
    max-width: 52ch;
    color: var(--text-soft);
    font-size: var(--fs-sm);
    line-height: 1.5;
  }
  .plant-invite-cta {
    flex: none;
    align-self: center;
  }
  .plant-invite-cta .kbd {
    background: var(--surface);
    border-color: var(--border-strong);
    color: var(--text-soft);
  }

  @container today (max-width: 640px) {
    .plant-invite {
      flex-direction: column;
      align-items: flex-start;
    }
  }

  /* ---------------------------------------------------------- reading rows */
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
    margin: 0 -8px;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
    min-width: 0;
    transition: background var(--t-fast) var(--ease-out);
  }
  .row:hover {
    background: var(--raised);
  }
  .row.two {
    flex-direction: column;
    align-items: stretch;
    gap: 3px;
  }
  .row.with-tree {
    flex-direction: row;
    align-items: center;
    gap: 10px;
  }
  .row-tree {
    display: flex;
    align-items: center;
    flex: none;
  }
  .row-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 1;
    min-width: 0;
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
    min-width: 0;
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .mono {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
  }
  .progress {
    flex: 1;
    min-width: 40px;
    height: 4px;
    border-radius: 2px;
    background: var(--sunken);
    overflow: hidden;
  }
  .progress-fill {
    display: block;
    width: 100%;
    height: 100%;
    background: var(--leaf);
    transform-origin: left;
    transition: transform var(--t-enter) var(--ease-out);
  }

  /* ------------------------------------------------------------ rediscover */
  .feature-title {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--fs-xl);
    min-width: 0;
  }
  .feature-note {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text-soft);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .rediscover .meta {
    margin: 0;
  }
  .rediscover .open {
    align-self: flex-start;
    margin-top: auto;
  }

  /* ------------------------------------------------------------ empty vault */
  .empty {
    padding: 56px 40px;
    align-items: center;
    text-align: center;
    gap: 18px;
  }
  .empty-art {
    color: var(--leaf);
  }
  .empty h2 {
    margin: 0;
    font-size: var(--fs-2xl);
  }
  .empty p {
    margin: 0;
    max-width: 52ch;
    color: var(--text-soft);
    font-size: var(--fs-sm);
    line-height: 1.6;
  }
  .ctas {
    width: 100%;
    max-width: 620px;
    margin-top: 8px;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }
  .cta {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--leaf);
    text-align: left;
    cursor: pointer;
    transition:
      background var(--t-fast) var(--ease-out),
      border-color var(--t-fast) var(--ease-out);
  }
  .cta:hover {
    border-color: var(--border-hover);
    background: var(--raised);
  }
  .cta span {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    color: var(--text);
  }
  /* Not the big empty-state heading the global .empty styles give <strong>. */
  .cta strong {
    font-family: var(--font);
    font-size: var(--fs-base);
    font-weight: 500;
    letter-spacing: 0;
    line-height: 1.35;
  }
  .cta small {
    color: var(--muted);
    font-size: var(--fs-xs);
    line-height: 1.45;
  }
  @container today (max-width: 620px) {
    .ctas {
      grid-template-columns: 1fr;
    }
  }

  .loading {
    display: grid;
    place-items: center;
    padding: 48px;
  }
</style>
