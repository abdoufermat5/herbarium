// Motion helpers shared by the shell. Everything here degrades to "do the
// thing instantly": no view transitions in the webview, or reduced motion,
// means the update simply runs.

import { tick } from "svelte";

/** True when the user asked the OS for less motion. */
export function reducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

interface ViewTransitionHandle {
  finished: Promise<void>;
  updateCallbackDone: Promise<void>;
}

type ViewTransitionDocument = Document & {
  startViewTransition?: (update: () => Promise<void> | void) => ViewTransitionHandle;
};

// Named regions (sidebar, titlebar, main) only exist while a transition runs:
// an element with a `view-transition-name` becomes a containing block for its
// fixed-position descendants, which would break popovers the rest of the time.
// app.css keys the names off this class.
const ACTIVE_CLASS = "vt-active";
let running = 0;

/**
 * Run a state update inside a view transition so the content area crossfades
 * and rises a few pixels, while the sidebar and titlebar stay put. The update
 * runs exactly once either way, and its result (or rejection) is returned, so
 * callers can treat this as a plain `await update()`.
 */
export async function transition(update: () => void | Promise<void>): Promise<void> {
  const doc = document as ViewTransitionDocument;
  if (!doc.startViewTransition || reducedMotion()) {
    await update();
    return;
  }
  const root = document.documentElement;
  running++;
  root.classList.add(ACTIVE_CLASS);
  const done = () => {
    if (--running === 0) root.classList.remove(ACTIVE_CLASS);
  };
  let vt: ViewTransitionHandle;
  try {
    vt = doc.startViewTransition.call(doc, async () => {
      await update();
      // Let Svelte flush the DOM before the browser captures the "new" state.
      await tick();
    });
  } catch (e) {
    done();
    throw e;
  }
  vt.finished.then(done, done);
  await vt.updateCallbackDone;
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
