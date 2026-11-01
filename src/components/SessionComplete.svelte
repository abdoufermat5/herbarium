<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { api } from "../lib/api";
  import { app, fmtClock, goAll, nextDueLine, session, startReviewSession } from "../lib/state.svelte";
  import { plural } from "../lib/format";
  import { celebrate } from "../lib/celebrate";
  import { t } from "../lib/i18n.svelte";
  import type { ReviewGrade, ReviewStats } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import VitalityLeaf from "./VitalityLeaf.svelte";

  // Shown in place of the review view when a session ends with pages graded.
  // `session.summary` is frozen by the state layer; the streak and what is still
  // due come from a fresh `review.stats`.
  const summary = $derived(session.summary);

  const GRADES: ReviewGrade[] = ["again", "hard", "good", "easy"];
  const MAX_RECOVERED = 5;

  let stats = $state<ReviewStats | null>(null);
  let bloomEl = $state<SVGSVGElement | null>(null);
  let primaryEl = $state<HTMLButtonElement | null>(null);
  /** Flips the recovered leaves from how they were to how they are. */
  let revived = $state(false);
  let reviveTimer: ReturnType<typeof setTimeout> | undefined;

  const shownStats = $derived(stats ?? app.reviewStats);
  const moreDue = $derived((shownStats?.dueTotal ?? 0) > 0);
  const newBest = $derived(!!shownStats && shownStats.streak > 1 && shownStats.streak === shownStats.longestStreak);

  const headline = $derived.by(() => {
    if (!summary) return "";
    const n = summary.graded;
    const words = t("session.words").split(",");
    const spelled = n <= words.length ? words[n - 1] : String(n);
    return t("session.headline", { count: n, words: spelled.charAt(0).toUpperCase() + spelled.slice(1) });
  });

  const recovered = $derived(summary?.recovered ?? []);

  function gradesLabel(): string {
    const c = summary?.counts;
    return t("session.gradesLabel", { again: c?.again ?? 0, hard: c?.hard ?? 0, good: c?.good ?? 0, easy: c?.easy ?? 0 });
  }

  function primary() {
    if (moreDue) void startReviewSession();
    else void goAll();
  }

  function onKey(e: KeyboardEvent) {
    if (e.defaultPrevented || e.repeat || e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
    if (app.paletteOpen || app.importOpen || document.querySelector('[aria-modal="true"]')) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
    if (e.key === "Escape") {
      e.preventDefault();
      void goAll();
    } else if (e.key === "Enter" && !target?.closest("button, a")) {
      // A focused button already activates itself on Enter.
      e.preventDefault();
      primary();
    }
  }

  onMount(() => {
    void api
      .reviewStats()
      .then((value) => (stats = value))
      .catch((e) => console.error(e));

    // The bloom opens, then the petals fall from its centre; leaves perk up after.
    if (bloomEl) {
      const box = bloomEl.getBoundingClientRect();
      void celebrate({ x: box.left + box.width / 2, y: box.top + box.height * 0.42 });
    }
    reviveTimer = setTimeout(() => (revived = true), 700);

    // Enter is the primary action: put it under the keyboard right away.
    void tick().then(() => primaryEl?.focus({ preventScroll: true }));

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  onDestroy(() => {
    clearTimeout(reviveTimer);
    // Leaving the screen, by any route, retires the summary.
    session.summary = null;
  });
</script>

{#if summary}
  <div class="pane">
    <div class="inner">
      <section class="done" aria-labelledby="session-headline">
        <svg
          bind:this={bloomEl}
          class="bloom"
          viewBox="0 0 120 120"
          width="96"
          height="96"
          aria-hidden="true"
        >
          <path class="stem" d="M60 72 C60 90 58 102 55 114" />
          <path class="bloom-leaf l1" d="M59 96 C49 96 43 90 42 81 C52 81 58 86 59 96 Z" />
          <path class="bloom-leaf l2" d="M58 104 C67 104 74 99 75 91 C66 91 60 96 58 104 Z" />
          {#each [0, 1, 2, 3, 4, 5, 6, 7] as i (i)}
            <path
              class="petal"
              class:alt={i % 2 === 1}
              style:--a={`${i * 45}deg`}
              style:--i={i}
              d="M60 54 C49 42 50 24 60 13 C70 24 71 42 60 54 Z"
            />
          {/each}
          <circle class="eye" cx="60" cy="54" r="7" />
        </svg>

        <span class="eyebrow">{t("session.eyebrow")}</span>
        <h1 id="session-headline" class="display headline">{headline}</h1>
        {#if summary.skipped > 0}
          <p class="sub">{t("session.skipped", { count: summary.skipped })}</p>
        {/if}
      </section>

      <section class="figures" aria-label={t("session.grades")}>
        <div class="dist-block">
          <span class="eyebrow">{t("session.grades")}</span>
          <div class="dist" role="img" aria-label={gradesLabel()}>
            {#each GRADES as g (g)}
              {#if summary.counts[g] > 0}
                <span class="dist-seg" data-grade={g} style:flex-grow={summary.counts[g]}></span>
              {/if}
            {/each}
          </div>
          <ul class="legend">
            {#each GRADES as g (g)}
              <li data-grade={g} class:zero={summary.counts[g] === 0}>
                <span class="dot" aria-hidden="true"></span>
                {t(`review.${g}`)}
                <span class="n">{summary.counts[g]}</span>
              </li>
            {/each}
          </ul>
        </div>

        <dl class="stats">
          <div class="stat">
            <dt>{t("session.time")}</dt>
            <dd class="mono">{fmtClock(summary.durationMs)}</dd>
          </div>
          <div class="stat">
            <dt>{t("session.recovered")}</dt>
            <dd class="mono">{recovered.length}</dd>
          </div>
          <div class="stat">
            <dt>{t("session.streak")}</dt>
            <dd class="mono" class:best={newBest}>{shownStats ? plural(shownStats.streak, "day") : "–"}</dd>
          </div>
        </dl>
      </section>

      {#if newBest && shownStats}
        <p class="best-line" role="status">
          <Icon name="leaf" size={14} />
          {t("session.streakBest", { days: plural(shownStats.streak, "day") })}
        </p>
      {/if}

      {#if recovered.length > 0}
        <section class="recovered">
          <h2 class="eyebrow">{t("session.recoveredCount", { count: recovered.length })}</h2>
          <ul>
            {#each recovered.slice(0, MAX_RECOVERED) as e, k (e.id)}
              <li style:--k={k}>
                <VitalityLeaf meta={revived ? e.afterSnapshot : e.beforeSnapshot} now={revived ? e.afterSnapshot.at : e.beforeSnapshot.at} size={18} />
                <span class="ellipsis">{e.title || t("common.untitled")}</span>
              </li>
            {/each}
          </ul>
          {#if recovered.length > MAX_RECOVERED}
            <p class="more">{t("session.recoveredMore", { count: recovered.length - MAX_RECOVERED })}</p>
          {/if}
        </section>
      {/if}

      {#if shownStats}
        <p class="next">
          {#if shownStats.dueTotal > 0}{t("session.stillDue", { count: shownStats.dueTotal })}{" "}{/if}{nextDueLine(shownStats)}
        </p>
      {/if}

      <div class="actions">
        <button
          bind:this={primaryEl}
          class="btn btn-primary"
          title={moreDue ? t("session.moreHint") : t("session.backHint")}
          onclick={primary}
        >
          {moreDue ? t("session.more") : t("session.back")}
          <span class="kbd">Enter</span>
        </button>
        {#if moreDue}
          <button class="btn" onclick={() => void goAll()} title={t("session.backHint")}>
            {t("session.back")}
            <span class="kbd">Esc</span>
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .pane {
    height: 100%;
    overflow-y: auto;
    background: radial-gradient(ellipse 60% 40% at 50% 0%, var(--glow), transparent 70%);
  }
  .inner {
    max-width: 640px;
    margin: 0 auto;
    padding: 40px 32px 56px;
    display: flex;
    flex-direction: column;
    gap: 22px;
    animation: rise var(--t-enter) var(--ease-out) backwards;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
  }

  .done {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 10px;
  }
  .headline {
    font-size: var(--fs-4xl);
    max-width: 24ch;
    text-wrap: balance;
  }
  .sub {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--muted);
  }

  /* ---------------------------------------------------------------- bloom */

  .bloom {
    margin-bottom: 6px;
    overflow: visible;
  }
  .stem {
    fill: none;
    stroke: var(--c-sage);
    stroke-width: 2;
    stroke-linecap: round;
    animation: stem-grow 500ms var(--ease-out) backwards;
  }
  @keyframes stem-grow {
    from {
      opacity: 0;
      transform: scaleY(0.6);
    }
  }
  .stem,
  .bloom-leaf,
  .petal,
  .eye {
    transform-box: view-box;
  }
  .stem {
    transform-origin: 60px 114px;
  }
  .bloom-leaf {
    fill: var(--c-sage-soft);
    stroke: var(--c-sage);
    stroke-width: 1.4;
    stroke-linejoin: round;
    transform-origin: 60px 100px;
    animation: unfold 520ms var(--ease-spring) 700ms backwards;
  }
  .petal {
    fill: var(--c-rose-soft);
    stroke: var(--c-rose);
    stroke-width: 1.4;
    stroke-linejoin: round;
    transform-origin: 60px 54px;
    transform: rotate(var(--a));
    /* Eight petals, 60ms apart: the last one lands at about 1s. */
    animation: petal-open 520ms var(--ease-spring) calc(var(--i) * 60ms) backwards;
  }
  .petal.alt {
    fill: var(--c-amber-soft);
    stroke: var(--c-amber);
  }
  @keyframes petal-open {
    from {
      transform: rotate(calc(var(--a) - 40deg)) scale(0);
      opacity: 0;
    }
  }
  @keyframes unfold {
    from {
      transform: scale(0);
      opacity: 0;
    }
  }
  .eye {
    fill: var(--c-amber);
    transform-origin: 60px 54px;
    animation: unfold 360ms var(--ease-spring) 480ms backwards;
  }

  /* -------------------------------------------------------------- figures */

  .figures {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .dist-block {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .dist {
    display: flex;
    gap: 3px;
    height: 12px;
    transform-origin: left center;
    animation: bar-grow 520ms var(--ease-out) 300ms backwards;
  }
  @keyframes bar-grow {
    from {
      transform: scaleX(0);
    }
  }
  [data-grade="again"] {
    --g: var(--c-rose);
    --g-soft: var(--c-rose-soft);
  }
  [data-grade="hard"] {
    --g: var(--c-amber);
    --g-soft: var(--c-amber-soft);
  }
  [data-grade="good"] {
    --g: var(--c-sage);
    --g-soft: var(--c-sage-soft);
  }
  [data-grade="easy"] {
    --g: var(--c-sky);
    --g-soft: var(--c-sky-soft);
  }
  .dist-seg {
    flex-basis: 0;
    min-width: 8px;
    border-radius: var(--radius-xs);
    background: var(--g-soft);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--g) 55%, transparent);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--fs-xs);
    color: var(--text-soft);
  }
  .legend li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .legend li.zero {
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--g-soft);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--g) 55%, transparent);
  }
  .n {
    font-family: var(--mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    margin: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    overflow: hidden;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 18px;
    box-shadow: -1px 0 0 var(--border);
  }
  .stat dt {
    font-size: var(--fs-2xs);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .stat dd {
    margin: 0;
    font-size: var(--fs-xl);
    line-height: 1;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .mono {
    font-family: var(--mono);
  }
  .stat dd.best {
    color: var(--c-sage);
    animation: pop 400ms var(--ease-spring) 900ms backwards;
  }
  @keyframes pop {
    from {
      transform: scale(0.8);
      opacity: 0;
    }
  }
  .stat dd {
    transform-origin: left center;
  }

  .best-line {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: -8px 0 0;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    background: var(--c-sage-soft);
    color: var(--c-sage);
    font-size: var(--fs-sm);
    font-weight: 500;
    animation: rise var(--t-enter) var(--ease-spring) 900ms backwards;
  }

  /* ------------------------------------------------------------ recovered */

  .recovered {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .recovered h2 {
    margin: 0;
  }
  .recovered ul {
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--border);
  }
  .recovered li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 2px;
    border-bottom: 1px solid var(--border);
    font-size: var(--fs-sm);
    color: var(--text);
    animation: rise var(--t-med) var(--ease-out) calc(500ms + var(--k) * 60ms) backwards;
  }
  .more {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .next {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--muted);
    text-align: center;
  }

  .actions {
    display: flex;
    justify-content: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .actions .kbd {
    margin-left: 4px;
  }
  .actions .btn-primary .kbd {
    background: transparent;
    border-color: color-mix(in oklab, var(--on-accent) 35%, transparent);
    color: var(--on-accent);
  }
</style>
