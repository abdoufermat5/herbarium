// Not a test: time common interactions in the desktop app, from the input to
// the next painted frame, on a given vault. usage: node e2e/tools/perf.mjs <vault>
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tempDir } from "../helpers.mjs";
import { launchApp } from "../webdriver.mjs";

const vault = process.argv[2];
const work = tempDir("perf");
mkdirSync(join(work, "config", "io.herbarium.desktop"), { recursive: true });
writeFileSync(
  join(work, "config", "io.herbarium.desktop", "config.json"),
  JSON.stringify({ vaultPath: vault, recentVaults: [vault], watchDownloads: false, captureShortcut: null }),
);
const t0 = Date.now();
const app = await launchApp({ HOME: work, XDG_CONFIG_HOME: join(work, "config"), XDG_DATA_HOME: join(work, "data") });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Run `action` in the page and time it until the DOM settles and a frame is painted. */
async function measure(name, action, arg) {
  const script = `
    const done = arguments[arguments.length - 1];
    const arg = arguments[0];
    const frame = () => new Promise((r) => requestAnimationFrame(() => r()));
    (async () => {
      const t = performance.now();
      let last = t;
      const obs = new MutationObserver(() => (last = performance.now()));
      obs.observe(document.body, { childList: true, subtree: true, characterData: true, attributes: true });
      await (${action.toString()})(arg);
      for (let i = 0; i < 200; i++) {
        await frame();
        await frame();
        if (performance.now() - last > 40) break;
      }
      obs.disconnect();
      done(Math.round((last - t) * 10) / 10);
    })().catch((e) => done("error: " + e));`;
  const ms = await app.cmd("POST", "/execute/async", { script, args: [arg ?? null] });
  console.log(`${String(ms).padStart(8)} ms  ${name}`);
}

/** The longest gap between frames while idle for `ms`. */
async function jank(name, ms) {
  const worst = await app.execAsync(
    (ms) =>
      new Promise((resolve) => {
        let prev = performance.now();
        let worst = 0;
        const end = prev + ms;
        const tick = (now) => {
          worst = Math.max(worst, now - prev);
          prev = now;
          if (now < end) requestAnimationFrame(tick);
          else resolve(Math.round(worst));
        };
        requestAnimationFrame(tick);
      }),
    ms,
  );
  console.log(`${String(worst).padStart(8)} ms  longest frame gap, ${name}`);
}

const clickText = (text) => {
  const el = [...document.querySelectorAll("button, a")].find((b) => b.textContent.trim().startsWith(text));
  el.click();
};

try {
  await app.waitFor(() => app.exec(() => document.querySelectorAll("nav button, aside button").length > 3), "app", 60000);
  console.log(`startup to shell: ${Date.now() - t0} ms`);
  await jank("idle 5 s after start (previews being made)", 5000);
  console.log("backend pages.search 'r' ms:", await app.execAsync(async () => {
    const t = performance.now();
    const hits = await window.__TAURI_INTERNALS__.invoke("invoke_op", { name: "pages.search", args: { query: "r" } });
    return `${Math.round(performance.now() - t)} (${hits.length} hits)`;
  }));
  console.log("backend pages.list ms:", await app.execAsync(async () => {
    const t = performance.now();
    const hits = await window.__TAURI_INTERNALS__.invoke("invoke_op", { name: "pages.list", args: {} });
    return `${Math.round(performance.now() - t)} (${hits.length})`;
  }));
  await measure("go to All pages", clickText, "All pages");
  const search = (q) =>
    new Promise((resolve) => {
      const count = () => document.querySelector(".count")?.textContent;
      const before = count();
      const i = document.querySelector("input[type=search], .search input");
      i.value = q;
      i.dispatchEvent(new Event("input", { bubbles: true }));
      const t = performance.now();
      const poll = () => (count() !== before || performance.now() - t > 3000 ? resolve() : setTimeout(poll, 2));
      poll();
    });
  await measure("library search 'tokio' until results shown (incl. 120 ms debounce)", search, "tokio");
  await measure("library search 'notes 1234' until results shown", search, "notes 1234");
  await measure("clear search until all pages shown", search, "");
  await measure("switch to list layout", () => document.querySelector("[aria-label*='List'], [title*='List']")?.click());
  await measure("switch to grid layout", () => document.querySelector("[aria-label*='Grid'], [title*='Grid']")?.click());
  await measure("open palette", () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true, bubbles: true })));
  await measure("palette 'cargo' until results shown (incl. 90 ms debounce)", (q) => {
    const i = document.querySelector("[role=dialog] input");
    i.value = q;
    i.dispatchEvent(new Event("input", { bubbles: true }));
    return new Promise((r) => setTimeout(r, 95));
  }, "cargo");
  await measure("close palette", () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await measure("go to Today", clickText, "Today");
  await measure("go to Review today", clickText, "Review today");
  await measure("go to Graph", clickText, "Graph");
  await measure("go to Settings", clickText, "Settings");
  await measure("go to All pages", clickText, "All pages");
  await measure("open a page", () => document.querySelector("article .open-overlay").click());
  await sleep(1500);
  await measure("back to library", () => document.querySelector("[aria-label*='Back'], .bar > button")?.click());
  await measure("expand a folder in sidebar", () => [...document.querySelectorAll("aside button, nav button")].find((b) => b.textContent.includes("topic-1"))?.click());
  await jank("idle 5 s", 5000);
} catch (e) {
  console.error(e.message);
} finally {
  await app.close();
}
