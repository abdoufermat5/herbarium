<script lang="ts">
  // Optional WebGL hero for the reading-path view. The 2D SVG tree stays the
  // source of truth and the fallback: whenever WebGL is missing, fails or the
  // context is lost, this component stays invisible and reports onactive(false).
  import { onMount } from "svelte";
  import type { HeroColors, HeroHandle } from "../lib/treegen/hero3d";

  interface Props {
    seed: string;
    read: number;
    total: number;
    size?: number;
    label: string;
    highlight?: number | null;
    onlimbclick?: (page: number) => void;
    onactive?: (active: boolean) => void;
    animate?: boolean;
    entrance?: boolean;
  }

  let {
    seed,
    read,
    total,
    size = 360,
    label,
    highlight = null,
    onlimbclick,
    onactive,
    animate = true,
    entrance = false,
  }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let handle = $state.raw<HeroHandle | null>(null);
  let active = $state(false);
  let reduced = $state(false);

  const VARS: Array<[keyof HeroColors, string, string]> = [
    ["ink", "--text-soft", "#50555a"],
    ["leaf", "--leaf", "#346538"],
    ["sage", "--c-sage", "#3f6b43"],
    ["sageSoft", "--c-sage-soft", "#e7efe5"],
    ["teal", "--c-teal", "#1f6e67"],
    ["tealSoft", "--c-teal-soft", "#ddefec"],
    ["rose", "--c-rose", "#9c3f55"],
    ["roseSoft", "--c-rose-soft", "#f7e5ea"],
    ["amber", "--c-amber", "#8a5b00"],
    ["clay", "--c-clay-soft", "#f4e6dd"],
  ];

  function cssColors(): HeroColors {
    const style = getComputedStyle(document.documentElement);
    const out = {} as HeroColors;
    for (const [key, name, fallback] of VARS) {
      const v = style.getPropertyValue(name).trim();
      out[key] = /^#[0-9a-f]{3,8}$/i.test(v) ? v : fallback;
    }
    return out;
  }

  function setActive(on: boolean) {
    if (active === on) return;
    active = on;
    onactive?.(on);
  }

  onMount(() => {
    const search = new URLSearchParams(location.search);
    const mode = search.get("hero3d");
    if (mode === "off" || !canvas) {
      onactive?.(false);
      return;
    }
    const force = mode === "force";
    const el = canvas;
    const motion = matchMedia("(prefers-reduced-motion: reduce)");
    reduced = motion.matches;
    const onMotion = () => (reduced = motion.matches);
    motion.addEventListener("change", onMotion);

    let disposed = false;
    let live: HeroHandle | null = null;
    let ro: ResizeObserver | undefined;
    let io: IntersectionObserver | undefined;
    let mo: MutationObserver | undefined;
    let visible = true;
    const sync = () => live?.setVisible(visible && !document.hidden);

    function teardown() {
      ro?.disconnect();
      io?.disconnect();
      mo?.disconnect();
      document.removeEventListener("visibilitychange", sync);
      el.removeEventListener("webglcontextlost", onLost);
      ro = io = mo = undefined;
      const h = live;
      live = null;
      handle = null;
      try {
        h?.dispose();
      } catch {
        /* disposal must never break the page */
      }
    }

    // The SVG is the fallback: if the context goes away, fall back to it.
    function onLost() {
      teardown();
      setActive(false);
    }

    void (async () => {
      try {
        const gl = document.createElement("canvas").getContext("webgl2", {
          alpha: true,
          failIfMajorPerformanceCaveat: !force,
        });
        if (!gl) {
          onactive?.(false);
          return;
        }
        // Release the capability probe so it does not linger as a second context.
        gl.getExtension("WEBGL_lose_context")?.loseContext();
        const { createHero } = await import("../lib/treegen/hero3d");
        if (disposed) return;
        const h = createHero(el, {
          seed,
          read,
          total,
          size,
          animate: animate && !reduced,
          colors: cssColors(),
          entrance,
        });
        if (disposed) {
          h.dispose();
          return;
        }
        live = h;
        handle = h;

        el.addEventListener("webglcontextlost", onLost);

        ro = new ResizeObserver(() => live?.resize(size));
        ro.observe(el.parentElement ?? el);

        io = new IntersectionObserver((entries) => {
          const last = entries[entries.length - 1];
          if (last) visible = last.isIntersecting;
          sync();
        });
        io.observe(el);
        document.addEventListener("visibilitychange", sync);

        mo = new MutationObserver(() => live?.setColors(cssColors()));
        mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });

        setActive(true);
      } catch {
        teardown();
        setActive(false);
        onactive?.(false);
      }
    })();

    return () => {
      disposed = true;
      motion.removeEventListener("change", onMotion);
      const wasActive = active;
      teardown();
      active = false;
      if (wasActive) onactive?.(false);
    };
  });

  $effect(() => {
    handle?.setProgress(read, total);
  });
  $effect(() => {
    handle?.setHighlight(highlight);
  });
  $effect(() => {
    handle?.resize(size);
  });
  $effect(() => {
    handle?.setAnimate(animate && !reduced);
  });

  function onclick(e: MouseEvent) {
    if (!handle) return;
    const page = handle.pick(e.clientX, e.clientY);
    if (page >= 0) onlimbclick?.(page);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<canvas
  bind:this={canvas}
  class="hero3d"
  class:active
  class:clickable={active && !!onlimbclick}
  role={active ? "img" : undefined}
  aria-label={active ? label : undefined}
  aria-hidden={active ? "false" : "true"}
  {onclick}
  style:width="{size * 0.9}px"
  style:height="{size}px"
></canvas>

<style>
  .hero3d {
    position: absolute;
    top: 0;
    left: 50%;
    transform: translateX(-50%);
    opacity: 0;
    pointer-events: none;
    transition: opacity 300ms ease;
  }
  .hero3d.active {
    opacity: 1;
    pointer-events: auto;
  }
  .hero3d.clickable {
    cursor: pointer;
  }
  @media (prefers-reduced-motion: reduce) {
    .hero3d {
      transition: none;
    }
  }
</style>
