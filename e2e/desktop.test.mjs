// End to end: the desktop app itself, driven through tauri-driver and
// WebKitWebDriver on a virtual display. One run walks what a new user does:
// the Today screen and sample pages, reading a page with highlights, a remix
// by Claude (a fake `claude` command) accepted from the comparison panel, a
// page shared as a gist (a fake GitHub), the graph, and the new settings.
// Skipped where the drivers are missing (see webdriver.mjs).
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { cleanup, tempDir, vaultPages } from "./helpers.mjs";
import { desktopBin, desktopUnavailable, launchApp } from "./webdriver.mjs";

const unavailable = desktopUnavailable();
const dirs = [];
let app;
let github;
let work;
let vault;

/** Just enough GitHub for a secret gist. */
async function fakeGitHub() {
  const gists = new Map();
  const server = createServer(async (req, res) => {
    let raw = "";
    for await (const chunk of req) raw += chunk;
    const send = (status, value) => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(value));
    };
    if (req.headers.authorization !== "Bearer t0ken") return send(401, { message: "Bad credentials" });
    if (req.method === "GET" && req.url === "/user") return send(200, { login: "octo" });
    if (req.method === "POST" && req.url === "/gists") {
      const id = `g${gists.size + 1}`;
      gists.set(id, JSON.parse(raw));
      return send(201, { id, html_url: `https://gist.github.com/octo/${id}` });
    }
    send(404, { message: "Not Found" });
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  return { gists, server, base: `http://127.0.0.1:${server.address().port}` };
}

before(async () => {
  if (unavailable) return;
  desktopBin();
  work = tempDir("desktop");
  dirs.push(work);
  vault = join(work, "vault");
  mkdirSync(vault, { recursive: true });
  const configDir = join(work, "config", "io.herbarium.desktop");
  mkdirSync(configDir, { recursive: true });
  writeFileSync(
    join(configDir, "config.json"),
    JSON.stringify({
      vaultPath: vault,
      recentVaults: [vault],
      watchDownloads: false,
      captureShortcut: null,
      aiProvider: "claude-code",
      aiModel: "claude-opus-5-5",
      githubLogin: "octo",
      publishRepo: "herbarium-pages",
    }),
  );
  writeFileSync(join(configDir, "secrets.json"), JSON.stringify({ githubToken: "t0ken" }));
  // Claude Code stand-in: answers every remix with a simpler page.
  const claude = join(work, "claude");
  writeFileSync(
    claude,
    `#!/bin/sh\ncat > '${work}/claude.stdin'\nsleep 1\nprintf '%s\\n' 'Here you go:' '\`\`\`html' '<!doctype html><html><head><title>Welcome, simply</title></head><body><h1>Welcome, simply</h1><p>Herbarium keeps your pages.</p></body></html>' '\`\`\`'\n`,
    { mode: 0o755 },
  );
  github = await fakeGitHub();
  app = await launchApp({
    HOME: work,
    XDG_CONFIG_HOME: join(work, "config"),
    XDG_DATA_HOME: join(work, "data"),
    HERBARIUM_CLAUDE_BIN: claude,
    HERBARIUM_GITHUB_API: github.base,
  });
});

after(async () => {
  await app?.close();
  github?.server.close();
  cleanup(...dirs);
});

/** Screenshot on failure, for the CI artifacts. */
async function step(name, fn) {
  try {
    await fn();
  } catch (e) {
    const shots = process.env.E2E_SCREENSHOTS ?? join(work, "shots");
    mkdirSync(shots, { recursive: true });
    await app.screenshot(join(shots, `${name}.png`)).catch(() => {});
    throw e;
  }
}

/** Keep a picture of the moment when E2E_SCREENSHOTS names a folder. */
async function shot(name) {
  if (!process.env.E2E_SCREENSHOTS) return;
  mkdirSync(process.env.E2E_SCREENSHOTS, { recursive: true });
  await app.screenshot(join(process.env.E2E_SCREENSHOTS, `${name}.png`));
}

test("desktop: a new user's first session", { skip: unavailable ?? false }, async () => {
  await step("today", async () => {
    await app.waitText("Let's fill your herbarium");
    await app.click("Add sample pages");
    await app.waitFor(() => vaultPages(vault).length === 3, "three sample pages");
    await app.waitText("Getting started with Herbarium");
  });

  const welcome = () => vaultPages(vault).find((p) => p.id === "herbarium-welcome");

  await step("highlight", async () => {
    await app.click("Welcome to Herbarium");
    await app.waitFor(() => app.exec(() => !!document.querySelector("iframe.ready")), "the page");
    await app.frame("iframe.ready");
    await app.waitFor(
      () =>
        app.exec(() => {
          const h1 = document.querySelector("h1");
          if (!h1) return false;
          const range = document.createRange();
          range.selectNodeContents(h1);
          const sel = getSelection();
          sel.removeAllRanges();
          sel.addRange(range);
          document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
          return true;
        }),
      "the heading",
    );
    await app.frame(null);
    await app.click("Green", ".hl-toolbar button");
    await app.waitFor(() => welcome().meta.ext?.highlights?.[0]?.quote === "Welcome to your herbarium", "the highlight saved");
    assert.equal(welcome().meta.ext.highlights[0].color, "green");
  });

  await step("remix", async () => {
    await app.click("Remix", ".actions button");
    await app.waitText("Remix this page");
    await app.click("Simplify", "[role=radio]");
    await app.waitText("Claude Code · claude-opus-5-5");
    await shot("remix-dialog");
    await app.click("Remix", "[role=dialog] .btn-primary");
    // The comparison panel opens on the proposal; the page waits for approval.
    await app.waitFor(() => app.exec(() => !!document.querySelector(".panel[role=dialog]")), "the proposal panel", 30000);
    await app.find("Remixed by Claude", ".panel .eyebrow");
    await shot("remix-proposal");
    assert.match(welcome().html, /Welcome to your herbarium/);
    const prompt = readFileSync(join(work, "claude.stdin"), "utf8");
    assert.match(prompt, /plainer words/);
    assert.match(prompt, /- "Welcome to your herbarium"/, "highlights go with the page");
    await app.click("Accept", ".panel[role=dialog] button");
    await app.waitFor(() => /Welcome, simply/.test(welcome().html), "the remix accepted");
  });

  await step("share", async () => {
    await app.click("Share", ".actions button");
    await app.click("Share as a secret gist", "[role=menuitem]");
    await app.click("Share as a secret gist", "[role=alertdialog] button");
    await app.waitFor(() => github.gists.size === 1, "the gist");
    const gist = github.gists.get("g1");
    assert.equal(gist.public, false);
    assert.match(gist.files["herbarium-welcome.html"].content, /Welcome, simply/);
    await app.waitText("Shared as a secret gist");
    const published = JSON.parse(readFileSync(join(vault, ".herbarium", "published.json"), "utf8"));
    assert.equal(published["herbarium-welcome"].gist.url, "https://gist.github.com/octo/g1");
    await app.click("Share", ".actions button");
    await app.find("Update the gist", "[role=menuitem]");
    await shot("share-menu");
    await app.exec(() => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  });

  await step("graph", async () => {
    await app.click("Graph", "nav button, aside button, button");
    await app.waitFor(() => app.exec(() => document.querySelectorAll("circle.dot-mark").length === 3), "three nodes");
    await shot("graph");
  });

  await step("settings", async () => {
    await app.click("Settings", "aside button, nav button, button");
    await app.waitText("Remix with");
    await app.waitText("Connected as octo.");
    await app.waitText("https://octo.github.io/herbarium-pages/");
  });

  await app.exec(() => document.getElementById("settings-ai")?.scrollIntoView());
  await new Promise((r) => setTimeout(r, 1200));
  await shot("settings");
});
