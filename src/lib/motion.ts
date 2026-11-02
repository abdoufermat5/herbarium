// Motion helpers shared by the shell. Everything here degrades to "do the
// thing instantly": reduced motion means the update simply runs.

import { tick } from "svelte";

/** True when the user asked the OS for less motion. */
export function reducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Run a route change, then let the content area rise into place. Enter-only
 * on purpose: a view transition rasterizes the old and new views to crossfade
 * them, which cost about 100 ms per navigation; this is one compositor-only
 * animation (opacity and transform) on the already-rendered `main`. The
 * sidebar and titlebar are outside it and never move.
 */
export async function transition(update: () => void | Promise<void>): Promise<void> {
  await update();
  if (reducedMotion()) return;
  await tick();
  document.querySelector<HTMLElement>("main.main")?.animate(
    [
      { opacity: 0, transform: "translateY(6px)" },
      { opacity: 1, transform: "none" },
    ],
    { duration: 180, easing: "cubic-bezier(0.16, 1, 0.3, 1)" },
  );
}

/** Call `fn` at most once per animation frame with the latest arguments. */
export function rafThrottle<A extends unknown[]>(fn: (...args: A) => void): ((...args: A) => void) & { cancel(): void } {
  let frame = 0;
  let last: A;
  const wrapped = (...args: A) => {
    last = args;
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      fn(...last);
    });
  };
  wrapped.cancel = () => {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
  };
  return wrapped;
}

/**
 * Svelte action: one indicator that slides between the items of a group. The
 * container holds a `.slide-ind` element and exactly one `.active` direct
 * child at a time; the action publishes that child's box as `--ind-x/y/w/h`
 * (and `--ind-on`: 0 when nothing is active) for the indicator to transform to.
 * Pass any value that changes whenever the active item does.
 */
export function slideIndicator(node: HTMLElement, _key?: unknown) {
  function measure() {
    const active = node.querySelector<HTMLElement>(":scope > .active");
    if (active) {
      node.style.setProperty("--ind-x", `${active.offsetLeft}px`);
      node.style.setProperty("--ind-y", `${active.offsetTop}px`);
      node.style.setProperty("--ind-w", `${active.offsetWidth}px`);
      node.style.setProperty("--ind-h", `${active.offsetHeight}px`);
    }
    node.style.setProperty("--ind-on", active ? "1" : "0");
  }
  measure();
  // No slide on the first placement: it should simply be there.
  const frame = requestAnimationFrame(() => node.setAttribute("data-ready", ""));
  const resize = new ResizeObserver(measure);
  resize.observe(node);
  return {
    update() {
      void tick().then(measure);
    },
    destroy() {
      cancelAnimationFrame(frame);
      resize.disconnect();
    },
  };
}
