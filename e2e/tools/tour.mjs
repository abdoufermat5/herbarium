// Not a test: walk the desktop app and screenshot each screen (UI review).
// usage: node e2e/tools/tour.mjs <out-dir> [light|dark] [WIDTHxHEIGHT]
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tempDir } from "../helpers.mjs";
import { launchApp } from "../webdriver.mjs";

const out = process.argv[2];
const theme = process.argv[3] ?? "light";
const [width, height] = (process.argv[4] ?? "1200x800").split("x").map(Number);
mkdirSync(out, { recursive: true });
const work = tempDir("tour");
const vault = join(work, "vault");
mkdirSync(join(work, "config", "io.herbarium.desktop"), { recursive: true });
mkdirSync(vault, { recursive: true });
writeFileSync(join(work, "config", "io.herbarium.desktop", "config.json"), JSON.stringify({ vaultPath: vault, recentVaults: [vault], watchDownloads: false, captureShortcut: null }));
const app = await launchApp({ HOME: work, XDG_CONFIG_HOME: join(work, "config"), XDG_DATA_HOME: join(work, "data") });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const shot = async (name) => { await sleep(900); await app.screenshot(join(out, `${theme}-${width}-${name}.png`)); };
try {
  await app.cmd("POST", "/window/rect", { width, height }).catch((e) => console.error("resize:", e.message));
  await app.exec((t) => { localStorage.setItem("herbarium.theme", t); }, theme);
  await app.exec(() => location.reload());
  await sleep(1500);
  await shot("01-today-empty");
  await app.click("Add sample pages");
  await sleep(1500);
  await shot("02-today");
  await app.click("All pages");
  await shot("03-library");
  await app.exec(() => document.dispatchEvent(new KeyboardEvent("keydown", { key: "j", ctrlKey: true, bubbles: true })));
  await shot("03b-ai-library");
  await app.exec(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await sleep(300);
  await app.click("Welcome to Herbarium", "a, button, [role=button], .card, li");
  await sleep(1500);
  await shot("04-reader");
  await app.click("Details", ".actions button");
  await shot("05-details");
  await app.click("Details", ".actions button");
  await app.click("Ask AI", ".titlebar button");
  await shot("05b-ai-page");
  await app.type(".ask textarea", "Use a warmer tone");
  await shot("05c-ai-typed");
  await app.exec(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await sleep(300);
  await app.click("Review today");
  await shot("06-review");
  await app.click("Graph");
  await shot("07-graph");
  await app.click("Settings");
  await shot("08-settings");
  for (const tab of ["Review", "Vault", "Capture", "AI & sharing", "About"]) {
    await app.click(tab, "[role=tab]");
    await shot(`08-settings-${tab.split(" ")[0].toLowerCase()}`);
  }
  await app.exec(() => document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true, bubbles: true })));
  await shot("09-palette");
} catch (e) {
  console.error(e);
  await app.screenshot(join(out, `${theme}-error.png`));
} finally {
  await app.close();
}
