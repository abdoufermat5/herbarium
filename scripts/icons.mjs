#!/usr/bin/env node
// Usage: pnpm icons
// Renders the brand SVGs in docs/brand/ into every PNG the app ships:
// the desktop icons (through `tauri icon`, which also makes .icns, .ico,
// Android and iOS), the browser extension's toolbar icons in both modes, and
// the master PNG. The small variants (fewer, thicker veins) are used at 32 px
// and below, where the full leaf turns to mush.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";

const brand = (name) => readFileSync(join("docs/brand", `${name}.svg`), "utf8");

const browser = await chromium.launch();
const page = await browser.newPage();

/** Render `svg` to a transparent `size`×`size` PNG at `out`. */
async function render(svg, size, out) {
  await page.setViewportSize({ width: size, height: size });
  await page.setContent(
    `<html><body style="margin:0;background:transparent">${svg.replace("<svg ", `<svg width="${size}" height="${size}" `)}</body></html>`,
  );
  await page.screenshot({ path: out, omitBackground: true, clip: { x: 0, y: 0, width: size, height: size } });
}

const work = mkdtempSync(join(tmpdir(), "herbarium-icons-"));
try {
  // Desktop: the operating system shows one icon whatever the mode, so the
  // dark one, the brand's primary.
  const master = join(work, "icon.png");
  await render(brand("icon-dark"), 1024, master);
  execFileSync("pnpm", ["tauri", "icon", master, "-o", "desktop/icons", "--ios-color", "#11201a"], { stdio: "inherit" });
  // The tray and the smallest window icon: the simplified leaf.
  await render(brand("icon-dark-small"), 32, "desktop/icons/32x32.png");

  // Extension: both modes; the background script picks one (see background.js).
  for (const mode of ["dark", "light"]) {
    for (const size of [16, 32]) await render(brand(`icon-${mode}-small`), size, `extension/icons/${mode}-${size}.png`);
    for (const size of [48, 64, 128]) await render(brand(`icon-${mode}`), size, `extension/icons/${mode}-${size}.png`);
  }
} finally {
  await browser.close();
  rmSync(work, { recursive: true, force: true });
}
console.log("Icons rendered from docs/brand/.");
