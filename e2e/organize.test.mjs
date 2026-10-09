// End to end: "Organize with AI" in the desktop app. A fake Anthropic API
// answers with a plan (some of it junk); the dialog shows only what is
// sound, applying moves the files and tags the pages, and Undo puts it back.
// Skipped where the drivers are missing (see webdriver.mjs).
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { cleanup, tempDir } from "./helpers.mjs";
import { desktopBin, desktopUnavailable, launchApp } from "./webdriver.mjs";

const unavailable = desktopUnavailable();
const dirs = [];
let app;
let api;
let vault;

const page = (title) => `<!doctype html><title>${title}</title><h1>${title}</h1><p>About ${title}.</p>`;

/** A Messages API that answers every request with this plan, as an event stream. */
async function fakeApi(state) {
  const server = createServer(async (req, res) => {
    let raw = "";
    for await (const chunk of req) raw += chunk;
    if (req.method === "GET") {
      res.writeHead(200, { "content-type": "application/json" });
      return res.end(JSON.stringify({ data: [{ type: "model", id: "claude-opus-5-5", display_name: "Claude Opus 5.5" }], has_more: false }));
    }
    state.requests.push(JSON.parse(raw));
    const text = JSON.stringify(state.plan);
    const send = (e) => `event: ${e.type}\ndata: ${JSON.stringify(e)}\n\n`;
    res.writeHead(200, { "content-type": "text/event-stream" });
    res.end(
      [
        { type: "message_start", message: { model: "claude-opus-5-5" } },
        { type: "content_block_start", index: 0, content_block: { type: "text", text: "" } },
        { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: `Here is the plan:\n\`\`\`json\n${text}\n\`\`\`` } },
        { type: "content_block_stop", index: 0 },
        { type: "message_delta", delta: { stop_reason: "end_turn" } },
        { type: "message_stop" },
      ]
        .map(send)
        .join(""),
    );
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  return { server, base: `http://127.0.0.1:${server.address().port}` };
}

const state = {
  requests: [],
  plan: {
    summary: "Grouped the Rust pages and the biology pages.",
    moves: [
      { id: "ownership", folder: "Programming/Rust", tags: ["Systems"], reason: "about Rust" },
      { id: "lifetimes", folder: "Programming/Rust", reason: "about Rust" },
      { id: "cells", folder: "Biology", tags: ["science"], reason: "cell biology" },
      { id: "ghost-page", folder: "Nowhere", reason: "does not exist" },
      { id: "dna", folder: "../../outside", reason: "escapes the library" },
    ],
  },
};

before(async () => {
  if (unavailable) return;
  desktopBin();
  const work = tempDir("organize");
  dirs.push(work);
  vault = join(work, "vault");
  mkdirSync(join(vault, "Inbox"), { recursive: true });
  mkdirSync(join(vault, "Notes"), { recursive: true });
  writeFileSync(join(vault, "ownership.html"), page("Rust ownership"));
  writeFileSync(join(vault, "Inbox", "lifetimes.html"), page("Rust lifetimes"));
  writeFileSync(join(vault, "Inbox", "cells.html"), page("How cells divide"));
  writeFileSync(join(vault, "Inbox", "dna.html"), page("DNA"));
  writeFileSync(join(vault, "Notes", "kept.html"), page("Already filed"));
  const configDir = join(work, "config", "io.herbarium.desktop");
  mkdirSync(configDir, { recursive: true });
  api = await fakeApi(state);
  writeFileSync(
    join(configDir, "config.json"),
    JSON.stringify({ vaultPath: vault, recentVaults: [vault], watchDownloads: false, captureShortcut: null, aiProvider: "anthropic", aiModel: "" }),
  );
  writeFileSync(join(configDir, "secrets.json"), JSON.stringify({ anthropicKey: "sk-test" }), { mode: 0o600 });
  app = await launchApp({
    HOME: work,
    XDG_CONFIG_HOME: join(work, "config"),
    XDG_DATA_HOME: join(work, "data"),
    HERBARIUM_ANTHROPIC_API: api.base,
  });
});

after(async () => {
  if (app) await app.close();
  api?.server.close();
  cleanup(...dirs);
});

/** A picture of the moment when E2E_SCREENSHOTS names a folder. */
async function shot(name) {
  if (!process.env.E2E_SCREENSHOTS) return;
  mkdirSync(process.env.E2E_SCREENSHOTS, { recursive: true });
  await new Promise((r) => setTimeout(r, 700));
  await app.screenshot(join(process.env.E2E_SCREENSHOTS, `organize-${name}.png`));
}

const where = (rel) => existsSync(join(vault, rel));

test("organize with AI: review, apply and undo", { skip: unavailable ?? false }, async () => {
  await app.waitText("Today");
  await app.click("All pages", "aside button, nav button");
  // The one AI panel: what you type goes along with the action you pick.
  await app.click("Ask AI", ".titlebar button");
  // Unsorted pages: the three at the top level or in the Inbox, not the filed one.
  await app.waitText("Pages in no folder or in the Inbox (4)");
  await app.type(".ask textarea", "keep Rust together");
  await shot("options");
  await app.click("Organize new and unsorted pages", "[role=option]");

  // Only the sound part of the plan is shown: three pages, two destinations.
  await app.waitText("Grouped the Rust pages and the biology pages.", 30000);
  await app.waitText("Apply 3 changes");
  await app.waitText("Programming/Rust");
  await app.waitText("2 new folders");
  await shot("review");
  assert.equal(state.requests.length, 1);
  const prompt = state.requests[0].messages[0].content;
  assert.match(prompt, /keep Rust together/);
  assert.match(prompt, /"id":"ownership"/);
  assert.doesNotMatch(prompt, /"id":"kept"/, "the filed page is not sent");

  // Leave one out, then apply.
  await app.click("How cells divide", ".row");
  await app.waitText("Apply 2 changes");
  await app.click("Apply 2 changes", ".btn-primary");
  await app.waitText("2 pages organized");
  await app.waitFor(() => app.exec(() => !document.querySelector("[role=dialog][aria-modal=true]")), "the panel closed");
  assert.ok(where("Programming/Rust/ownership.html"));
  assert.ok(where("Programming/Rust/lifetimes.html"));
  assert.ok(where("Inbox/cells.html"), "the page left out stayed");
  assert.ok(where("Inbox/dna.html"), "the unsafe suggestion changed nothing");
  const sidecar = JSON.parse(readFileSync(join(vault, "Programming/Rust/ownership.json"), "utf8"));
  assert.deepEqual(sidecar.tags, ["systems"]);

  // Undo: back where they were, the tag removed.
  await app.click("Undo", ".toast button, button");
  await app.waitText("Everything is back where it was");
  assert.ok(where("ownership.html"));
  assert.ok(where("Inbox/lifetimes.html"));
  const back = JSON.parse(readFileSync(join(vault, "ownership.json"), "utf8"));
  assert.deepEqual(back.tags, []);
});
