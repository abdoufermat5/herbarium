<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { app, goView, openInPath, openPlantPath } from "../lib/state.svelte";
  import { pathGrowth, wasCelebrated, markCelebrated } from "../lib/grove";
  import { celebrate } from "../lib/celebrate";
  import { treeStage } from "../lib/tree";
  import { t, type MessageKey } from "../lib/i18n.svelte";
  import { reveal } from "../lib/reveal";
  import Icon from "../lib/Icon.svelte";
  import VitalityLeaf from "./VitalityLeaf.svelte";
  import PathTree from "./PathTree.svelte";

  const GROW_MS = 1200;

  const id = untrack(() => app.pathView);
  const path = $derived(app.paths.find((p) => p.id === id) ?? null);
  const growth = $derived(id ? pathGrowth(id) : { read: 0, total: 0, mature: false, next: null });
  const readSet = $derived(new Set(id ? (app.pathProgress[id]?.read ?? []) : []));
  const byId = $derived(new Map(app.library.map((p) => [p.id, p])));
  const rows = $derived(
    (path?.pages ?? []).map((pageId, i) => {
      const meta = byId.get(pageId) ?? null;
      return {
        pageId,
        i,
        meta,
        title: meta ? meta.title || t("common.untitled") : null,
        state: !meta ? "missing" : readSet.has(pageId) ? "read" : pageId === growth.next ? "next" : "todo",
      } as const;
    }),
  );
  const nextTitle = $derived(rows.find((r) => r.pageId === growth.next)?.title ?? null);
  const firstTitle = $derived(rows[0]?.title ?? rows[0]?.pageId ?? null);
  const stage = $derived(t(`tree.stage.${treeStage(growth.read, growth.total)}` as MessageKey));

  // The maturity moment plays once per path: decided when the view opens.
  const playMoment = untrack(() => !!id && pathGrowth(id).mature && !wasCelebrated(id));
  let momentDone = $state(false);
  let planted = $state(false);
  let stageEl = $state<HTMLElement | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    // Planting is a discrete act, so it drops once and never celebrates.
    if (id && app.justPlanted === id) {
      planted = true;
      app.justPlanted = null;
    }
    if (!playMoment || !id) return;
    const pathId = id;
    timer = setTimeout(() => {
      momentDone = true;
      markCelebrated(pathId);
      const r = stageEl?.getBoundingClientRect();
      void celebrate(r ? { x: r.left + r.width / 2, y: r.top + r.height * 0.4 } : undefined);
    }, GROW_MS);
  });
  onDestroy(() => clearTimeout(timer));

  let highlight = $state<number | null>(null);
  let rowEls: Array<HTMLButtonElement | null> = [];

  function open(pageId: string) {
    if (path) void openInPath(path.id, pageId);
  }
  function openIndex(i: number) {
    const r = rows[i];
    if (r?.meta) open(r.pageId);
  }
  function primary() {
    if (!path) return;
    const target = growth.mature ? path.pages.find((p) => byId.has(p)) : growth.next;
    if (target) open(target);
  }

  function onTrailKey(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const current = rowEls.findIndex((el) => el === document.activeElement);
    const step = e.key === "ArrowDown" ? 1 : -1;
    for (let i = current + step; i >= 0 && i < rows.length; i += step) {
      if (rows[i].meta) {
        e.preventDefault();
        rowEls[i]?.focus();
        return;
      }
    }
    e.preventDefault();
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.key !== "Enter" || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
    const el = document.activeElement;
    if (el && el !== document.body && el.tagName !== "MAIN" && !el.classList.contains("pane")) return;
    e.preventDefault();
    primary();
  }
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="pane">
  <div class="inner">
    {#if !path}
      <div class="gone" use:reveal>
        <p class="display gone-title">{t("paths.view.notFound")}</p>
        <button class="btn" onclick={() => void goView("today")}>{t("paths.view.back")}</button>
      </div>
    {:else}
      <header class="head" use:reveal>
        <span class="eyebrow">{t("paths.view.eyebrow")}</span>
        <h1 class="display">{path.name}</h1>
        {#if path.description}<p class="desc">{path.description}</p>{/if}
        <p class="stage-line">
          <span>{stage}</span>
          {#if growth.total > 0}
            <span class="dot" aria-hidden="true">·</span>
            <span><span class="mono">{growth.read} / {growth.total}</span> {t("paths.view.pagesRead")}</span>
          {/if}
        </p>
        {#if planted}
          <div class="planted" role="status">
            <p class="planted-title display">{t("paths.view.planted")}</p>
            {#if firstTitle}
              <p class="planted-body">{t("paths.view.plantedBody", { title: firstTitle })}</p>
            {/if}
          </div>
        {/if}
        <div class="actions">
          {#if growth.mature || growth.next}
            <button class="btn btn-primary" onclick={primary}>
              {growth.mature ? t("paths.view.again") : t("paths.view.continue", { title: nextTitle ?? "" })}
            </button>
          {/if}
          <button class="btn" onclick={() => openPlantPath({ pathId: path.id })}>
            <Icon name="list-checks" size={14} />
            {t("paths.view.addPages")}
          </button>
        </div>
      </header>

      <div class="hero">
        <div class="stage" bind:this={stageEl} use:reveal>
          <div class="tree">
            <span class="tree-seed" class:drop={planted}>
              <PathTree
                seed={path.id}
                read={growth.read}
                total={growth.total}
                size={360}
                detail="full"
                animate
                growOnMount={playMoment}
                {highlight}
                onlimbclick={openIndex}
                label={path.name}
              />
            </span>
          </div>
          {#if playMoment && momentDone}
            <div class="grown" role="status">
              <p class="grown-title">{t("paths.view.grownTitle")}</p>
              <p class="grown-body">{t("paths.view.grownBody", { name: path.name })}</p>
            </div>
          {/if}
          {#if rows.length === 0}
            <p class="hint">{t("paths.view.emptyHint")}</p>
            <button class="btn btn-primary empty-pick" onclick={() => openPlantPath({ pathId: path.id })}>
              {t("paths.view.pick")}
            </button>
          {/if}
        </div>

        {#if rows.length > 0}
          <ol class="trail" aria-label={t("paths.view.trail")}>
            {#each rows as r (r.pageId)}
              <li class="step {r.state}" use:reveal>
                <button
                  class="row"
                  bind:this={rowEls[r.i]}
                  disabled={r.state === "missing"}
                  aria-current={r.state === "next" ? "step" : undefined}
                  title={r.state === "missing" ? t("paths.missing") : undefined}
                  onclick={() => open(r.pageId)}
                  onkeydown={onTrailKey}
                  onmouseenter={() => (highlight = r.i)}
                  onmouseleave={() => (highlight = null)}
                  onfocus={() => (highlight = r.i)}
                  onblur={() => (highlight = null)}
                >
                  <span class="node" aria-hidden="true">
                    {#if r.state === "read"}<Icon name="check" size={11} />{/if}
                  </span>
                  <span class="num mono">{r.i + 1}</span>
                  <span class="title ellipsis">{r.title ?? r.pageId}</span>
                  {#if r.state === "read"}<span class="sr">{t("paths.view.readMark")}</span>{/if}
                  {#if r.state === "next"}
                    <span class="chip" data-color="amber">{t("paths.view.nextChip")}</span>
                  {/if}
                  {#if r.meta}<span class="leaf"><VitalityLeaf meta={r.meta} size={14} /></span>{/if}
                </button>
              </li>
            {/each}
          </ol>
        {/if}
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
    gap: 28px;
    container: pathview / inline-size;
  }
  .head .eyebrow {
    margin: 0 0 6px;
    display: block;
  }
  .head h1 {
    font-size: var(--fs-3xl);
    margin: 0;
  }
  .desc {
    margin: 8px 0 0;
    max-width: 62ch;
    color: var(--text-soft);
    font-size: var(--fs-base);
  }
  .stage-line {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 12px 0 0;
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .stage-line .mono {
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    color: var(--text-soft);
  }
  .actions {
    margin-top: 18px;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .planted {
    margin-top: 14px;
    animation: planted-in var(--t-enter) var(--ease-out) both;
  }
  .planted-title {
    margin: 0;
    font-size: var(--fs-2xl);
    color: var(--c-sage);
  }
  .planted-body {
    margin: 4px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-soft);
  }
  @keyframes planted-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }

  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 24px;
    align-items: start;
  }
  @container pathview (min-width: 820px) {
    .hero {
      grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr);
      gap: 32px;
    }
  }

  .stage {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    min-height: 400px;
    padding: 24px 16px 20px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: radial-gradient(ellipse 60% 55% at 50% 58%, var(--glow), transparent 70%);
    overflow: hidden;
  }
  .tree {
    width: min(100%, 360px);
    color: var(--text-soft);
  }
  .tree-seed {
    display: block;
    transform-origin: 50% 92%;
  }
  .tree-seed.drop {
    animation: seed-drop 420ms var(--ease-spring) both;
  }
  @keyframes seed-drop {
    from {
      opacity: 0;
      transform: translateY(-14px) scale(0.9);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
  .empty-pick {
    margin-top: 10px;
  }
  .grown {
    position: absolute;
    top: 18px;
    left: 0;
    right: 0;
    text-align: center;
    pointer-events: none;
    animation: grown-in var(--t-enter) var(--ease-out) both;
  }
  .grown-title {
    margin: 0;
    font-family: var(--font-display);
    font-variation-settings: var(--font-display-settings);
    font-size-adjust: none;
    font-size: calc(var(--fs-2xl) * var(--display-size-adjust));
    font-weight: var(--display-weight);
    letter-spacing: var(--display-tracking);
    line-height: var(--display-leading);
    color: var(--c-sage);
  }
  .grown-body {
    margin: 4px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-soft);
  }
  @keyframes grown-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  .hint {
    margin: 12px 0 0;
    max-width: 38ch;
    text-align: center;
    font-size: var(--fs-sm);
    color: var(--muted);
  }

  /* ---------------------------------------------------------------- trail */
  .trail {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .step {
    position: relative;
  }
  /* The thin ink line connecting the nodes. */
  .step::before {
    content: "";
    position: absolute;
    left: 20px;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--border-strong);
  }
  .step:first-child::before {
    top: 50%;
  }
  .step:last-child::before {
    bottom: 50%;
  }
  .step:only-child::before {
    display: none;
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 9px 12px 9px 10px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
    transition: background 120ms var(--ease-out);
  }
  .row:hover:not(:disabled),
  .row:focus-visible {
    background: var(--sunken);
  }
  .row:disabled {
    cursor: default;
    opacity: 0.5;
  }
  .row:active {
    scale: 1;
  }
  .node {
    flex: none;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    box-sizing: border-box;
    border-radius: 50%;
    border: 1.5px solid var(--border-strong);
    background: var(--bg);
    color: var(--bg);
    transition:
      background 120ms var(--ease-out),
      border-color 120ms var(--ease-out);
  }
  .read .node {
    background: var(--c-sage);
    border-color: var(--c-sage);
  }
  .next .node {
    border-color: var(--c-sage);
    box-shadow: 0 0 0 3px var(--c-sage-soft);
  }
  .todo .row:hover .node {
    border-color: var(--c-sage);
  }
  .num {
    flex: none;
    min-width: 14px;
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-2xs);
    color: var(--muted);
    text-align: right;
  }
  .title {
    flex: 1;
    min-width: 0;
  }
  .missing .title {
    text-decoration: line-through;
  }
  .next .title {
    font-weight: 500;
  }
  .leaf {
    flex: none;
    display: inline-flex;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  .gone {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 16px;
    padding-top: 40px;
  }
  .gone-title {
    margin: 0;
    font-size: var(--fs-2xl);
    color: var(--text-soft);
  }

  @media (prefers-reduced-motion: reduce) {
    .grown,
    .planted,
    .tree-seed.drop {
      animation: none;
    }
  }
</style>
