<script lang="ts">
  import { onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { app, mergePreviews, onPageEvents } from "../lib/state.svelte";
  import type { PreviewDigest } from "../lib/types";

  /**
   * Makes the library's previews in the background: loads each page that
   * needs one into a hidden sandboxed frame, asks the reader bridge for its
   * layout digest and saves it. One page at a time, after the app settles.
   */
  const SETTLE_MS = 700;
  const ANSWER_MS = 3000;
  /** Only work after this long without input: the hidden frame shares the
   *  window's thread, so a page loading there would stall a click or a keystroke. */
  const QUIET_MS = 1500;

  let lastInput = 0;
  const onInput = () => (lastInput = performance.now());
  for (const type of ["pointerdown", "keydown", "wheel", "pointermove"]) {
    window.addEventListener(type, onInput, { passive: true, capture: true });
  }

  async function quiet() {
    // Also wait while a page is open in the reader.
    while (!stopped && (performance.now() - lastInput < QUIET_MS || app.readId)) {
      await new Promise((r) => setTimeout(r, 400));
    }
  }

  let frame: HTMLIFrameElement | undefined = $state();
  let src = $state("about:blank");
  let running = false;
  let stopped = false;
  let nonce = 0;

  function waitForLoad(): Promise<void> {
    return new Promise((resolve) => {
      const done = () => {
        frame?.removeEventListener("load", done);
        resolve();
      };
      frame?.addEventListener("load", done);
      setTimeout(done, 8000);
    });
  }

  function askDigest(): Promise<PreviewDigest | null> {
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        window.removeEventListener("message", onMessage);
        resolve(null);
      }, ANSWER_MS);
      function onMessage(e: MessageEvent) {
        if (!frame || e.source !== frame.contentWindow) return;
        const data = e.data as { type?: string; digest?: PreviewDigest | null } | null;
        if (data?.type !== "herbarium:digest") return;
        clearTimeout(timer);
        window.removeEventListener("message", onMessage);
        resolve(data.digest ?? null);
      }
      window.addEventListener("message", onMessage);
      frame?.contentWindow?.postMessage({ type: "herbarium:digest" }, "*");
    });
  }

  /** Pages whose preview is missing or stale, found from pushed events. */
  const queued = new Set<string>();
  /** Ask the backend which previews exist: the first run and after a rescan found changes. */
  let needsList = true;
  let listedRevision = app.vaultRevision;

  async function make(id: string): Promise<boolean> {
    await quiet();
    if (stopped || !app.config?.vaultPath) return false;
    const loaded = waitForLoad();
    src = `herbarium://page/${encodeURIComponent(id)}?v=preview-${++nonce}`;
    await loaded;
    await new Promise((r) => setTimeout(r, SETTLE_MS));
    const digest = await askDigest();
    if (!digest) return true;
    try {
      await api.savePreview(id, digest);
      mergePreviews({ [id]: digest });
    } catch (e) {
      console.warn("preview not saved", id, e);
    }
    return true;
  }

  async function run() {
    if (running || stopped || !app.config?.vaultPath) return;
    running = true;
    try {
      let todo: string[] = [];
      if (needsList || listedRevision !== app.vaultRevision) {
        needsList = false;
        listedRevision = app.vaultRevision;
        const { previews, missing } = await api.listPreviews();
        app.previews = previews;
        todo = missing;
        for (const id of missing) queued.delete(id);
      }
      // Pages changed since: only those need a new preview, not a whole-map refetch.
      while (!stopped && (todo.length > 0 || queued.size > 0)) {
        const id = todo.shift() ?? queued.values().next().value!;
        queued.delete(id);
        if (!app.library.some((p) => p.id === id)) continue;
        if (!(await make(id))) break;
      }
    } catch (e) {
      console.error(e);
      needsList = true;
    } finally {
      src = "about:blank";
      running = false;
    }
  }

  // Run on the first load, in a newly opened vault and after a rescan found
  // changes, once the window is idle.
  let listedVault = app.config?.vaultPath;
  $effect(() => {
    void app.vaultRevision;
    // Another vault: nothing queued or listed so far applies to it.
    if (app.config?.vaultPath !== listedVault) {
      listedVault = app.config?.vaultPath;
      needsList = true;
      queued.clear();
    }
    const timer = setTimeout(() => void run(), 1500);
    return () => clearTimeout(timer);
  });

  // A page created or changed here needs a preview; a deleted one loses it.
  $effect(() =>
    onPageEvents((events) => {
      for (const event of events) {
        const id = event.meta.id;
        if (event.kind === "page.deleted") {
          queued.delete(id);
        } else if (event.kind === "page.created" || event.htmlChanged) {
          // A previous digest stays until the new one replaces it. Metadata
          // edits (grades, tags, notes) leave the rendering unchanged.
          queued.add(id);
        }
      }
      if (queued.size > 0) setTimeout(() => void run(), 1500);
    }),
  );

  onDestroy(() => {
    stopped = true;
    for (const type of ["pointerdown", "keydown", "wheel", "pointermove"]) {
      window.removeEventListener(type, onInput, { capture: true });
    }
  });
</script>

<iframe
  bind:this={frame}
  {src}
  title="preview"
  class="thumbnailer"
  sandbox="allow-scripts"
  tabindex="-1"
  aria-hidden="true"
></iframe>

<style>
  .thumbnailer {
    position: fixed;
    left: -10000px;
    top: 0;
    width: 1024px;
    height: 768px;
    border: 0;
    visibility: hidden;
    pointer-events: none;
  }
</style>
