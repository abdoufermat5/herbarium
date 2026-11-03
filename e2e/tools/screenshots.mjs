// Not a test: the screenshots in docs/screenshots/ (README, Flathub listing),
// in light and dark, from the desktop app showing the demo vault.
//
// usage: node e2e/tools/screenshots.mjs [out-dir]   (needs Xvfb, tauri-driver
// and WebKitWebDriver; HERBARIUM_REBUILD=1 rebuilds the app first). With
// Herbarium already open, run it under `dbus-run-session --`: otherwise the
// new window hands itself to the open one and the driver waits forever.
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tempDir } from "../helpers.mjs";
import { desktopBin, launchApp } from "../webdriver.mjs";
import { seedDemoVault, writeDemoVault } from "./demo-vault.mjs";
import { mcpClient } from "./mcp.mjs";

const OUT = resolve(process.argv[2] ?? "docs/screenshots");
const W = 1280;
const H = 800;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
mkdirSync(OUT, { recursive: true });

const bin = desktopBin();

/**
 * Seed a vault, launch the app on it and capture every view in one theme.
 * A fresh vault per theme keeps the due queue and the session screenshots
 * identical in both.
 */
async function capture(theme) {
  const work = tempDir(`shots-${theme}`);
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

  // Tags, reading paths, reads and review history, as an agent would add them.
  const mcp = mcpClient(bin, vault, env);
  await seedDemoVault(vault, mcp.call);
  mcp.close();

  const app = await launchApp(env);
  const key = (k, ctrl = false) =>
    app.exec((k, ctrl) => document.dispatchEvent(new KeyboardEvent("keydown", { key: k, ctrlKey: ctrl, bubbles: true })), k, ctrl);
  const shot = async (name) => {
    await sleep(1400);
    await app.screenshot(join(OUT, `${theme}-${name}.png`));
  };
  try {
    await app.cmd("POST", "/window/rect", { x: 0, y: 0, width: W, height: H });
    await app.exec((t) => {
      localStorage.setItem("herbarium.theme", t);
      location.reload();
    }, theme);
    await sleep(3000);
    // Page previews render in the background, one at a time, while the library is open.
    await app.click("All pages");
    await app.waitFor(
      () => app.exec(() => document.querySelectorAll(".preview:not(.no-preview)").length >= 12 || null),
      "page previews",
      120000,
    ).catch(() => console.error("Not every page preview was ready."));

    // The fully grown path has already had its moment in these shots.
    await app.exec(
      (vault) => localStorage.setItem(`herbarium.grown:${vault}:rust-from-the-ground-up`, "1"),
      vault,
    );

    await app.click("Today");
    await shot("today");

    await app.click("All pages");
    await app.exec(() => document.querySelectorAll("*").forEach((el) => el.scrollTop > 0 && (el.scrollTop = 0)));
    await sleep(1500);
    await shot("library");

    await app.type("#page-search", "ownership");
    await sleep(900);
    await app.click("Rust Ownership in Five Rules", ".open-overlay");
    await sleep(2200);
    await shot("reader");

    await app.click("Rust from the ground up", "aside .path-name");
    await app.waitFor(() => app.exec(() => !!document.querySelector(".hero .stage")), "the path hero");
    await sleep(1800);
    await shot("path");

    await app.click("Review today", "aside button, nav button");
    await shot("review");
    await app.click("Start session", "button");
    await sleep(1200);
    for (let i = 0; i < 4; i++) {
      await app.click("Good", "button");
      await sleep(1100);
    }
    await app.waitText("SESSION COMPLETE", 15000);
    await sleep(1800);
    await shot("session");

    await key("Escape");
    await app.click("Graph", "aside button, nav button");
    await sleep(1800);
    await app.type(".search input", "owner");
    await app.press(".search input", "\uE007");
    await shot("graph");

    await key("k", true);
    await app.type(".overlay input", "cach");
    await shot("palette");
    await key("Escape");

    await app.click("Settings", "aside button, nav button");
    await sleep(1200);
    await app.exec(() => {
      const settings = document.querySelector(".settings");
      if (settings) settings.scrollTop = 370;
    });
    await app.click("Field notes", ".specimen");
    await shot("appearance");
  } catch (e) {
    console.error(`${theme}:`, e);
    await app.screenshot(join(OUT, `${theme}-error.png`)).catch(() => {});
    process.exitCode = 1;
  } finally {
    await app.close();
  }
}

for (const theme of ["light", "dark"]) await capture(theme);
console.log(`Screenshots in ${OUT}`);
// Lingering helpers (portals, the accessibility bus) would keep a private D-Bus session open.
process.exit();
