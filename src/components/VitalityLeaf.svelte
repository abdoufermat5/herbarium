<script lang="ts">
  import type { PageMeta } from "../lib/types";
  import { vitality } from "../lib/vitality";
  import { t } from "../lib/i18n.svelte";

  let {
    meta,
    size = 16,
    now,
  }: { meta: Pick<PageMeta, "lastReview" | "nextReview" | "ext">; size?: number; now?: number } = $props();

  const v = $derived(vitality(meta, now));
  const label = $derived(
    v.recall == null
      ? t(`vitality.${v.level}`)
      : `${t(`vitality.${v.level}`)} · ${t("vitality.recall", { percent: Math.round(v.recall * 100) })}`,
  );
</script>

<svg
  class="vitality {v.level}"
  width={size}
  height={size}
  viewBox="0 0 16 16"
  role="img"
  aria-label={label}
>
  <title>{label}</title>
  {#if v.level === "seed"}
    <ellipse class="seed" cx="8" cy="12" rx="3.2" ry="2.2" />
    <path class="stem" d="M8 9.8 C8 8.2 8.6 7 9.8 6.2" />
  {:else}
    <path class="stem" d="M8 15 V8.5" />
    <g class="leaf">
      <path class="blade" d="M8 8.5 C4.6 8 3.4 5.2 4.2 1.8 C7.4 2.6 8.8 5 8 8.5 Z" />
      <path class="vein" d="M8 8.5 C6.6 6.6 5.6 4.6 4.8 2.6" />
    </g>
    <g class="leaf second">
      <path class="blade" d="M8 11 C10.8 11 12.6 9 12.4 6.4 C9.8 6.6 8.2 8.4 8 11 Z" />
    </g>
  {/if}
</svg>

<style>
  .vitality {
    display: block;
    flex: none;
    overflow: visible;
    --ink: var(--c-sage);
  }
  .healthy { --ink: var(--c-teal); }
  .thirsty { --ink: var(--c-amber); }
  .wilting { --ink: var(--c-clay); }
  .seed { fill: var(--muted); }
  .stem {
    fill: none;
    stroke: var(--ink);
    stroke-width: 1.4;
    stroke-linecap: round;
  }
  .seed + .stem { stroke: var(--muted); }
  .blade {
    fill: var(--ink);
    transition: fill var(--t-slow) var(--ease-out);
  }
  .vein {
    fill: none;
    stroke: var(--surface);
    stroke-width: 0.8;
    stroke-linecap: round;
    opacity: 0.7;
  }
  .leaf {
    transform-box: view-box;
    transform-origin: 8px 8.5px;
    transition: transform var(--t-slow) var(--ease-out), opacity var(--t-slow) var(--ease-out);
  }
  .leaf.second { transform-origin: 8px 11px; }
  .healthy .leaf.second { opacity: 0.55; }
  .thirsty .leaf { transform: rotate(-28deg); }
  .thirsty .leaf.second { transform: rotate(24deg); opacity: 0.6; }
  .wilting .leaf { transform: rotate(-62deg) scale(0.92); }
  .wilting .leaf.second { transform: rotate(48deg) scale(0.85); opacity: 0.45; }
</style>
