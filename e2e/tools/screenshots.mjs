// Not a test: the screenshots in docs/screenshots/ (README, Flathub listing),
// in light and dark, from the desktop app showing the demo vault.
//
// usage: node e2e/tools/screenshots.mjs [out-dir]   (needs Xvfb, tauri-driver
// and WebKitWebDriver; HERBARIUM_REBUILD=1 rebuilds the app first). With
// Herbarium already open, run it under `dbus-run-session --`: otherwise the
// new window hands itself to the open one and the driver waits forever.
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tempDir } from "../helpers.mjs";
import { desktopBin, launchApp } from "../webdriver.mjs";
import { PAGES, writeDemoVault } from "./demo-vault.mjs";

const OUT = resolve(process.argv[2] ?? "docs/screenshots");
const W = 1280;
const H = 800;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
mkdirSync(OUT, { recursive: true });

const work = tempDir("shots");
const vault = join(work, "Learning");
const config = join(work, "config", "io.herbarium.desktop");
mkdirSync(config, { recursive: true });
writeDemoVault(vault);
// A second vault in the list, so the sidebar shows the vault switcher.
const archive = join(work, "Archive");
mkdirSync(archive, { recursive: true });
writeFileSync(
  join(config, "config.json"),
  JSON.stringify({ vaultPath: vault, recentVaults: [vault, archive], watchDownloads: false, captureShortcut: null }),
);
const env = { HOME: work, XDG_CONFIG_HOME: join(work, "config"), XDG_DATA_HOME: join(work, "data") };

// Tags, a reading path and some review history, as an agent would add them.
const calls = [
  ...PAGES.map((p) => ["pages_update", { id: p.id, tags: p.tags }]),
  ["paths_create", { name: "Networking basics", pages: ["dns", "tcp-handshake", "http-caching"] }],
  ...["photosynthesis", "tcp-handshake"].map((id) => ["review_schedule", { id, intervalMinutes: 1 }]),
  ...["big-o", "mitosis", "french-revolution", "color-contrast"].map((id) => ["review_complete", { id, grade: "good" }]),
];
const lines = [
  { jsonrpc: "2.0", id: 0, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "shots", version: "1" } } },
  ...calls.map(([name, args], i) => ({ jsonrpc: "2.0", id: i + 1, method: "tools/call", params: { name, arguments: args } })),
];
spawnSync(desktopBin(), ["mcp", "--vault", vault], {
  input: lines.map((l) => JSON.stringify(l)).join("\n") + "\n",
  env: { ...process.env, ...env },
});
// Due a minute after scheduling.
await sleep(61000);

const app = await launchApp(env);
const key = (k, ctrl = false) =>
  app.exec((k, ctrl) => document.dispatchEvent(new KeyboardEvent("keydown", { key: k, ctrlKey: ctrl, bubbles: true })), k, ctrl);
try {
  await app.cmd("POST", "/window/rect", { x: 0, y: 0, width: W, height: H });
  // Page previews render in the background, one at a time, while the library is open.
  await sleep(3000);
  await app.click("All pages");
  await app.waitFor(
    () => app.exec(() => document.querySelectorAll(".preview:not(.no-preview)").length >= 12 || null),
    "page previews",
    120000,
  ).catch(() => console.error("Not every page preview was ready."));
  for (const theme of ["light", "dark"]) {
    const shot = async (name) => {
      await sleep(1200);
      await app.screenshot(join(OUT, `${theme}-${name}.png`));
    };
    await app.exec((t) => {
      localStorage.setItem("herbarium.theme", t);
      location.reload();
    }, theme);
    await sleep(3000);
    await app.click("Today");
    await shot("today");
    await app.click("All pages");
    await app.exec(() => document.querySelectorAll("*").forEach((el) => el.scrollTop > 0 && (el.scrollTop = 0)));
    await sleep(1500);
    await shot("library");
    await app.click("Rust Ownership in Five Rules", ".card, li, button, a");
    await sleep(2000);
    await shot("reader");
    await app.click("Review today");
    await shot("review");
    await key("Escape");
    await app.click("Graph");
    await sleep(1500);
    await shot("graph");
    await key("k", true);
    await app.type(".overlay input", "cach");
    await shot("palette");
    await key("Escape");
  }
} catch (e) {
  console.error(e);
  await app.screenshot(join(OUT, "error.png")).catch(() => {});
  process.exitCode = 1;
} finally {
  await app.close();
}
console.log(`Screenshots in ${OUT}`);
// Lingering helpers (portals, the accessibility bus) would keep a private D-Bus session open.
process.exit();
