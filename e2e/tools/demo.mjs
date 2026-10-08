// Not a test: record the README demo (docs/demo.gif). Drives the desktop app
// on a virtual display with a real mouse and keyboard (xdotool) while ffmpeg
// films the screen, then turns the film into a GIF.
//
// usage: node e2e/tools/demo.mjs [out.gif]   (needs Xvfb, tauri-driver,
// WebKitWebDriver, xdotool and ffmpeg; HERBARIUM_REBUILD=1 rebuilds the app)
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join, resolve } from "node:path";
import { tempDir } from "../helpers.mjs";
import { desktopBin, launchApp } from "../webdriver.mjs";
import { PAGES, remixedWithQuiz, writeDemoVault } from "./demo-vault.mjs";

const OUT = resolve(process.argv[2] ?? "docs/demo.gif");
const W = 1200;
const H = 800;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* --------------------------------------------------------------- the vault */

const work = tempDir("demo");
const vault = join(work, "Learning");
const config = join(work, "config", "io.herbarium.desktop");
mkdirSync(config, { recursive: true });
writeDemoVault(vault);

// A fake Anthropic API: the remix streams in over a second and a half.
const ownership = PAGES.find((p) => p.id === "ownership");
const api = createServer(async (req, res) => {
  for await (const _ of req);
  const reply = "```html\n" + remixedWithQuiz(ownership) + "\n```";
  res.writeHead(200, { "content-type": "text/event-stream" });
  const send = (e) => res.write(`event: ${e.type}\ndata: ${JSON.stringify(e)}\n\n`);
  send({ type: "message_start", message: { model: "claude-opus-5-5" } });
  send({ type: "content_block_start", index: 0, content_block: { type: "text", text: "" } });
  const parts = 12;
  for (let i = 0; i < parts; i++) {
    const a = Math.floor((reply.length * i) / parts);
    const b = Math.floor((reply.length * (i + 1)) / parts);
    send({ type: "content_block_delta", index: 0, delta: { type: "text_delta", text: reply.slice(a, b) } });
    await sleep(120);
  }
  send({ type: "content_block_stop", index: 0 });
  send({ type: "message_delta", delta: { stop_reason: "end_turn" } });
  send({ type: "message_stop" });
  res.end();
});
await new Promise((r) => api.listen(0, "127.0.0.1", r));

writeFileSync(
  join(config, "config.json"),
  JSON.stringify({
    vaultPath: vault,
    recentVaults: [vault],
    watchDownloads: false,
    captureShortcut: null,
    aiProvider: "anthropic",
    aiModel: "claude-opus-5-5",
  }),
);
writeFileSync(join(config, "secrets.json"), JSON.stringify({ anthropicKey: "sk-demo" }), { mode: 0o600 });

// Tags, a reading path, a few pages due and a little review history, through
// the MCP server like an agent would.
const env = {
  HOME: work,
  XDG_CONFIG_HOME: join(work, "config"),
  XDG_DATA_HOME: join(work, "data"),
  HERBARIUM_ANTHROPIC_API: `http://127.0.0.1:${api.address().port}`,
};
function mcp(calls) {
  const lines = [
    { jsonrpc: "2.0", id: 0, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "demo", version: "1" } } },
    ...calls.map(([name, args], i) => ({ jsonrpc: "2.0", id: i + 1, method: "tools/call", params: { name, arguments: args } })),
  ];
  spawnSync(desktopBin(), ["mcp", "--vault", vault], {
    input: lines.map((l) => JSON.stringify(l)).join("\n") + "\n",
    env: { ...process.env, ...env },
  });
}
mcp([
  ...PAGES.map((p) => ["pages_update", { id: p.id, tags: p.tags }]),
  ["paths_create", { name: "Networking basics", pages: ["dns", "tcp-handshake", "http-caching"] }],
  ...["photosynthesis", "tcp-handshake"].map((id) => ["review_schedule", { id, intervalMinutes: 1 }]),
  ...["big-o", "mitosis", "french-revolution", "color-contrast"].map((id) => ["review_complete", { id, grade: "good" }]),
]);

/* ----------------------------------------------------------- the pointer */

const app = await launchApp(env);
const x11 = { ...process.env, DISPLAY: app.display };
const xdo = (...args) => execFileSync("xdotool", args.map(String), { env: x11 });
let pointer = { x: W / 2, y: H / 2 };

/** Glide the pointer to (x, y), easing in and out like a hand, in `ms` of real time. */
async function glide(x, y, ms = 600) {
  const from = { ...pointer };
  const start = Date.now();
  for (;;) {
    const t = Math.min(1, (Date.now() - start) / ms);
    const e = t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
    xdo("mousemove", Math.round(from.x + (x - from.x) * e), Math.round(from.y + (y - from.y) * e));
    if (t >= 1) break;
    await sleep(8);
  }
  pointer = { x, y };
}

let t0 = Date.now();
const mark = (scene) => console.log(`${((Date.now() - t0) / 1000).toFixed(1)}s ${scene}`);

/** The centre of the first visible element matching `selector` whose text includes `text`. */
async function where(text, selector = "button, a, [role=menuitem], [role=tab], [role=option], label, .card, li") {
  const at = await app.waitFor(
    () =>
      app.exec(
        (text, selector) => {
          for (const el of document.querySelectorAll(selector)) {
            const r = el.getBoundingClientRect();
            const label = `${el.textContent} ${el.getAttribute("aria-label") ?? ""}`;
            if (r.width > 0 && r.height > 0 && label.includes(text)) return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
          }
          return null;
        },
        text,
        selector,
      ),
    `“${text}”`,
  );
  return { x: Math.round(at.x), y: Math.round(at.y) };
}

async function click(text, selector, { ms, pause = 250 } = {}) {
  const p = await where(text, selector);
  await glide(p.x, p.y, ms);
  await sleep(pause);
  xdo("click", 1);
}

async function type(text, delay = 85) {
  xdo("type", "--delay", delay, text);
}

/* --------------------------------------------------------------- warm up */

await app.cmd("POST", "/window/rect", { x: 0, y: 0, width: W, height: H });
await app.exec(() => {
  localStorage.setItem("herbarium.theme", "dark");
  location.reload();
});
await sleep(2500);
// Let the library draw its page previews while nobody watches.
await app.click("All pages");
await sleep(20000);
await app.click("Today");
await sleep(1500);
xdo("mousemove", pointer.x, pointer.y);

/* ---------------------------------------------------------------- action */

const film = join(work, "demo.mp4");
const ffmpeg = spawn(
  "ffmpeg",
  ["-y", "-loglevel", "error", "-f", "x11grab", "-draw_mouse", "1", "-framerate", "30", "-video_size", `${W}x${H}`, "-i", `${app.display}+0,0`, "-c:v", "libx264", "-crf", "12", "-preset", "veryfast", "-pix_fmt", "yuv444p", film],
  { stdio: ["pipe", "inherit", "inherit"] },
);
await sleep(1200);
t0 = Date.now();

try {
  mark("today");
  // Today: what is due, the reading path in progress, recent saves.
  await glide(560, 300, 700);
  await sleep(900);

  mark("library");
  // The library, with each page's first screen, and a search filter.
  await click("All pages", "aside button, nav button");
  await sleep(1200);
  await click("", "#page-search", { pause: 150 });
  await type("tag:rust");
  await sleep(1100);

  mark("reader");
  // Read a page and highlight a passage.
  await click("Rust Ownership in Five Rules", ".card, li, button, a");
  await sleep(1500);
  const frame = await app.exec(() => {
    const r = document.querySelector("iframe.ready").getBoundingClientRect();
    return { x: r.left, y: r.top };
  });
  await app.frame("iframe.ready");
  const span = await app.exec(() => {
    const p = [...document.querySelectorAll("p")].find((el) => el.textContent.startsWith("Borrows can never"));
    const range = document.createRange();
    const text = p.firstChild;
    range.setStart(text, 0);
    range.setEnd(text, "Borrows can never outlive the value they point to".length);
    const rects = range.getClientRects();
    const a = rects[0];
    const b = rects[rects.length - 1];
    return { x1: a.left + 1, y1: a.top + a.height / 2, x2: b.right - 1, y2: b.top + b.height / 2 };
  });
  await app.frame(null);
  await glide(frame.x + span.x1, frame.y + span.y1);
  await sleep(200);
  xdo("mousedown", 1);
  await glide(frame.x + span.x2, frame.y + span.y2, 900);
  xdo("mouseup", 1);
  await sleep(700);
  await click("Yellow", ".hl-toolbar button");
  await sleep(1000);

  mark("remix");
  // Remix it with AI: the result waits as a proposal to compare and accept.
  await click("Remix", ".actions button");
  await sleep(700);
  await click("Add a quiz", ".dialog button");
  await sleep(400);
  await click("Remix", ".dialog .btn-primary");
  await sleep(2700);
  await click("Accept", ".panel button, button", { pause: 600 });
  await sleep(1000);

  mark("quiz");
  // Quiz mode hides the marked answers until revealed.
  await click("Quiz me", "button");
  await sleep(900);
  const box = await app.exec(() => {
    const r = document.querySelector("iframe.ready").getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  });
  await glide(box.x + box.w / 2, box.y + box.h * 0.6, 500);
  for (let i = 0; i < 9; i++) {
    xdo("click", 5);
    await sleep(60);
  }
  await sleep(700);
  await app.frame("iframe.ready");
  const cover = await app.exec(() => {
    const r = document.querySelector(".herbarium-recall-cover")?.getBoundingClientRect();
    return r ? { x: r.left + Math.min(r.width / 2, 180), y: r.top + r.height / 2 } : null;
  });
  await app.frame(null);
  if (cover) {
    await glide(box.x + cover.x, box.y + cover.y, 600);
    await sleep(500);
    xdo("click", 1);
    await sleep(1400);
  }

  mark("graph");
  // The graph: folders settle into islands; a page lists its links.
  await click("Graph", "aside button, nav button");
  await sleep(2600);
  // Find a page on the map: it is selected and its links listed.
  await click("", ".search input", { pause: 150 });
  await type("owner", 90);
  await sleep(500);
  xdo("key", "Return");
  await sleep(2300);

  mark("review");
  // A review session: grade each page due today.
  await click("Review today", "aside button, nav button");
  await sleep(1300);
  await click("Start session", "button");
  await sleep(1400);
  for (let i = 0; i < 2; i++) {
    const good = await app.exec(() => !!document.querySelector(".review-bar, [class*=session]"));
    if (!good) break;
    await click("Good", "button", { ms: 450 });
    await sleep(1100);
  }
  await sleep(1300);
  mark("end");
} catch (e) {
  console.error(e);
  await app.screenshot(join(work, "error.png"));
  console.error("screenshot:", join(work, "error.png"));
} finally {
  ffmpeg.stdin.write("q");
  await new Promise((r) => ffmpeg.on("close", r));
  await app.close();
  api.close();
}

/* ------------------------------------------------------------------ GIF */

const palette = join(work, "palette.png");
const filters = "fps=10,scale=1000:-1:flags=lanczos";
execFileSync("ffmpeg", ["-y", "-loglevel", "error", "-i", film, "-vf", `${filters},palettegen=max_colors=160:stats_mode=diff`, palette]);
execFileSync("ffmpeg", [
  "-y", "-loglevel", "error", "-i", film, "-i", palette,
  "-lavfi", `${filters} [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle`,
  "-loop", "0", OUT,
]);
console.log(`wrote ${OUT} (film: ${film})`);
