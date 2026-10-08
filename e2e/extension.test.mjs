// End to end: the real extension in Chromium, talking to the real
// `herbarium` binary as its native messaging host, saving into a real vault.
// Fake Claude, ChatGPT, Gemini and article sites are served over HTTPS from
// localhost, with every host name resolved there.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "playwright";
import {
  EXTENSION_DIR,
  EXTENSION_ID,
  cleanup,
  installNativeHost,
  siteServer,
  tempDir,
  vaultPages,
  waitFor,
} from "./helpers.mjs";

const ARTIFACT = `<!DOCTYPE html><html><head><title>Solar system</title></head><body><h1>Planets</h1><p>Mercury first.</p></body></html>`;
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64",
);
const CLAUDE_CONV = "11111111-2222-3333-4444-555555555555";
const GPT_CONV = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

let dirs = [];
let server;
let context;
let worker;
let vault;
let launchLog;
let base;

/** The server's port, as the browser addressed it. */
const portOf = (req) => req.headers.host.split(":")[1];

const chatPage = (title) => `<!doctype html><html><head><title>${title}</title></head><body><main><p>chat</p></main></body></html>`;

before(async () => {
  const work = tempDir("ext");
  dirs.push(work);
  vault = join(work, "vault");
  launchLog = join(work, "launches.log");
  const profile = join(work, "profile");

  server = await siteServer(work, {
    [`claude.ai/chat/${CLAUDE_CONV}`]: () => ({ body: chatPage("Claude") }),
    "claude.ai/api/organizations": () => ({ type: "application/json", body: JSON.stringify([{ uuid: "org-1" }]) }),
    [`claude.ai/api/organizations/org-1/chat_conversations/${CLAUDE_CONV}`]: () => ({
      type: "application/json",
      body: JSON.stringify({
        uuid: CLAUDE_CONV,
        name: "Space chat",
        created_at: "2025-03-01T09:00:00Z",
        chat_messages: [
          { uuid: "m1", sender: "human", created_at: "2025-03-01T09:00:00Z", text: "Explain the solar system", content: [] },
          {
            uuid: "m2",
            sender: "assistant",
            created_at: "2025-03-01T09:01:00Z",
            content: [
              { type: "text", text: "Here you go" },
              { type: "tool_use", name: "artifacts", input: { id: "solar", command: "create", type: "text/html", title: "Solar system", content: ARTIFACT } },
            ],
          },
        ],
      }),
    }),
    [`chatgpt.com/c/${GPT_CONV}`]: () => ({ body: chatPage("ChatGPT") }),
    "chatgpt.com/api/auth/session": () => ({ type: "application/json", body: JSON.stringify({ accessToken: "token-123" }) }),
    [`chatgpt.com/backend-api/conversation/${GPT_CONV}`]: (req) => {
      if (req.headers.authorization !== "Bearer token-123") return { status: 401, body: "{}" };
      const doc = JSON.stringify({ name: "quiz", type: "code/html", content: "<!doctype html><html><head><title>Quiz</title></head><body><p>Q1</p></body></html>" });
      return {
        type: "application/json",
        body: JSON.stringify({
          title: "Quiz chat",
          create_time: 1740000000,
          current_node: "n2",
          mapping: {
            n0: { id: "n0", parent: null, children: ["n1"], message: { author: { role: "user" }, content: { parts: ["Make a quiz"] } } },
            n1: { id: "n1", parent: "n0", children: ["n2"], message: { author: { role: "assistant" }, recipient: "canmore.create_textdoc", content: { parts: [doc] } } },
            n2: { id: "n2", parent: "n1", children: [], message: { author: { role: "assistant" }, content: { parts: ["Done"] } } },
          },
        }),
      };
    },
    "gemini.google.com/app/chat": () => ({
      body: `<!doctype html><html><head><title>Gemini</title></head><body>
        <user-query>Build a tip calculator page</user-query>
        <code-block><pre><code>&lt;!DOCTYPE html&gt;&lt;html&gt;&lt;head&gt;&lt;title&gt;Tip calculator&lt;/title&gt;&lt;/head&gt;&lt;body&gt;&lt;p&gt;tips&lt;/p&gt;&lt;/body&gt;&lt;/html&gt;</code></pre></code-block>
        <pre><code>&lt;p&gt;just a fragment&lt;/p&gt;</code></pre>
      </body></html>`,
    }),
    "news.example/article": (req) => ({
      body: `<!doctype html><html><head><title>Big article</title>
        <link rel="stylesheet" href="https://cdn.example:${portOf(req)}/style.css">
        <script>window.tracking = true;</script></head>
        <body><h1>Headline</h1><p id="lead">The lead paragraph that matters.</p>
        <img src="https://img.example:${portOf(req)}/pic.png" alt="pic" onclick="alert(1)">
        <a href="javascript:alert(1)">bad</a><a href="/relative">rel</a>
        <iframe src="https://ads.example/"></iframe></body></html>`,
    }),
    "cdn.example/style.css": () => ({ type: "text/css", body: "h1 { color: rgb(1, 2, 3); background: url(bg.png); }" }),
    "img.example/pic.png": () => ({ type: "image/png", body: PNG }),
  });
  base = (host, path) => `https://${host}:${server.port}${path}`;

  installNativeHost(profile, vault, launchLog);
  context = await chromium.launchPersistentContext(profile, {
    channel: "chromium",
    headless: true,
    ignoreHTTPSErrors: true,
    args: [
      `--disable-extensions-except=${EXTENSION_DIR}`,
      `--load-extension=${EXTENSION_DIR}`,
      "--host-resolver-rules=MAP * 127.0.0.1",
      "--ignore-certificate-errors",
      // The fake sites live on localhost; never send them through a proxy.
      "--no-proxy-server",
    ],
  });
  worker = context.serviceWorkers()[0] ?? (await context.waitForEvent("serviceworker"));
  assert.equal(worker.url(), `chrome-extension://${EXTENSION_ID}/background.js`);
});

after(async () => {
  await context?.close();
  await server?.close();
  cleanup(...dirs);
});

/** Open `url` and return its tab id as the extension sees it. */
async function openTab(url) {
  const page = await context.newPage();
  await page.goto(url);
  const tabId = await waitFor(
    // The newest tab with that address (an earlier test may have left one open).
    () => worker.evaluate(async (u) => Math.max(...(await chrome.tabs.query({ url: u.split("#")[0] })).map((t) => t.id)), url),
    10000,
    "tab id",
  );
  return { page, tabId };
}

test("the extension reaches the app through native messaging", async () => {
  const pong = await worker.evaluate(() => herbarium.ask("ping"));
  assert.equal(pong.ok, true);
  assert.equal(pong.vault, "vault");
  assert.equal(typeof pong.version, "string");
});

test("Claude: artifacts come from the conversation, with prompt and link", async () => {
  const { page, tabId } = await openTab(base("claude.ai", `/chat/${CLAUDE_CONV}`));
  const scan = await worker.evaluate((id) => herbarium.scanArtifacts(id), tabId);
  assert.equal(scan.source, "conversation");
  assert.deepEqual(
    scan.candidates.map((c) => [c.key, c.title, c.alreadyImported]),
    [[`claude:${CLAUDE_CONV}:solar`, "Solar system", false]],
  );
  const pages = await worker.evaluate(([s, keys]) => herbarium.saveArtifacts(s, keys), [scan, [scan.candidates[0].key]]);
  assert.equal(pages[0].title, "Solar system");

  const saved = vaultPages(vault).find((p) => p.meta.title === "Solar system");
  assert.ok(saved, "the page is in the vault");
  assert.equal(saved.folder, "Inbox");
  assert.equal(saved.meta.ext.source.tool, "Claude");
  assert.equal(saved.meta.ext.source.prompt, "Explain the solar system");
  assert.equal(saved.meta.ext.source.url, `https://claude.ai/chat/${CLAUDE_CONV}`);
  assert.equal(saved.meta.createdAt, Date.parse("2025-03-01T09:01:00Z"), "the artifact's own date");
  assert.match(saved.html, /Mercury first/);

  const again = await worker.evaluate((id) => herbarium.scanArtifacts(id), tabId);
  assert.equal(again.candidates[0].alreadyImported, true, "a saved artifact is recognised");
  await page.close();
});

test("ChatGPT: canvas documents come from the conversation with the session token", async () => {
  const { page, tabId } = await openTab(base("chatgpt.com", `/c/${GPT_CONV}`));
  const scan = await worker.evaluate((id) => herbarium.scanArtifacts(id), tabId);
  assert.equal(scan.source, "conversation");
  assert.equal(scan.candidates.length, 1);
  assert.equal(scan.candidates[0].title, "quiz");
  await worker.evaluate(([s, keys]) => herbarium.saveArtifacts(s, keys), [scan, [scan.candidates[0].key]]);
  const saved = vaultPages(vault).find((p) => p.meta.ext?.source?.tool === "ChatGPT");
  assert.ok(saved);
  assert.equal(saved.meta.ext.source.prompt, "Make a quiz");
  const asked = server.requests.find((r) => r.key === `chatgpt.com/backend-api/conversation/${GPT_CONV}`);
  assert.equal(asked.headers.authorization, "Bearer token-123");
  await page.close();
});

test("other chats: whole HTML documents shown as code are found on the page", async () => {
  const { page, tabId } = await openTab(base("gemini.google.com", "/app/chat"));
  const scan = await worker.evaluate((id) => herbarium.scanArtifacts(id), tabId);
  assert.equal(scan.source, "page");
  assert.deepEqual(scan.candidates.map((c) => c.title), ["Tip calculator"], "fragments are not pages");
  await worker.evaluate(([s, keys]) => herbarium.saveArtifacts(s, keys), [scan, ["page:0"]]);
  const saved = vaultPages(vault).find((p) => p.meta.title === "Tip calculator");
  assert.ok(saved);
  assert.equal(saved.meta.ext.source.tool, "Gemini");
  assert.equal(saved.meta.ext.source.prompt, "Build a tip calculator page");
  await page.close();
});

test("any page: a clean offline snapshot with images and styles inlined", async () => {
  const url = base("news.example", "/article");
  const { page, tabId } = await openTab(url);
  const saved = await worker.evaluate((id) => herbarium.saveSnapshot(id, "page"), tabId);
  assert.equal(saved.title, "Big article");
  const file = vaultPages(vault).find((p) => p.id === saved.id);
  assert.equal(file.meta.allowCdn, false, "web pages never get network access");
  assert.equal(file.meta.ext.source.url, url);
  assert.doesNotMatch(file.html, /<script/i, "scripts are removed");
  assert.doesNotMatch(file.html, /<iframe/i, "frames are removed");
  assert.doesNotMatch(file.html, /onclick|javascript:/i, "handlers and javascript: links are removed");
  assert.match(file.html, /src="data:image\/png;base64,/, "the image is inlined");
  assert.match(file.html, /<style>h1 \{ color: rgb\(1, 2, 3\)/, "the cross-origin stylesheet is inlined");
  assert.match(file.html, /url\("https:\/\/cdn\.example(:\d+)?\/bg\.png"\)/, "url() in CSS is made absolute");
  assert.match(file.html, new RegExp(`href="https://news\\.example:${server.port}/relative"`), "links are absolute");

  // The toolbar badge now says the page is saved.
  const badge = await waitFor(
    () => worker.evaluate((id) => chrome.action.getBadgeText({ tabId: id }), tabId),
    10000,
    "badge",
  );
  assert.equal(badge, "✓");
  const lookup = await worker.evaluate((u) => herbarium.ask("lookup", { url: u + "#section" }), url);
  assert.equal(lookup.pages[0].id, saved.id, "found again by address, fragment ignored");
  await page.close();
});

test("selection: only the selected part, in a readable page with its source", async () => {
  const url = base("news.example", "/article");
  const { page, tabId } = await openTab(url);
  await page.evaluate(() => {
    const range = document.createRange();
    range.selectNodeContents(document.getElementById("lead"));
    getSelection().removeAllRanges();
    getSelection().addRange(range);
  });
  const info = await worker.evaluate((id) => herbarium.ask && chrome.scripting.executeScript({ target: { tabId: id }, func: HerbariumCapture.pageInfo }), tabId);
  assert.equal(info[0].result.selection, true);
  const saved = await worker.evaluate((id) => herbarium.saveSnapshot(id, "selection"), tabId);
  const file = vaultPages(vault).find((p) => p.id === saved.id);
  assert.match(file.html, /The lead paragraph that matters\./);
  assert.doesNotMatch(file.html, /Headline/, "only the selection");
  assert.match(file.html, /Saved from <a href="https:\/\/news\.example/);

  await page.evaluate(() => getSelection().removeAllRanges());
  await assert.rejects(worker.evaluate((id) => herbarium.saveSnapshot(id, "selection"), tabId), /Select some text/);
  await page.close();
});

test("address bar search and opening a page in the app", async () => {
  const results = await worker.evaluate(() => herbarium.ask("search", { query: "solar" }));
  assert.equal(results.results[0].title, "Solar system");
  await worker.evaluate((id) => herbarium.ask("open", { page: id }), results.results[0].id);
  await waitFor(() => existsSync(launchLog), 5000, "launch log");
  assert.match(readFileSync(launchLog, "utf8"), new RegExp(`herbarium-app://open/${results.results[0].id}`));
});

test("the Today page shows the vault", async () => {
  const page = await context.newPage();
  await page.goto(`chrome-extension://${EXTENSION_ID}/today.html`);
  await page.waitForSelector("#content:not([hidden])");
  const recent = await page.locator("#recent li").allInnerTexts();
  assert.ok(recent.some((t) => t.includes("Solar system")), recent.join(" | "));
  await page.fill("#search", "quiz");
  await page.waitForSelector("#results li");
  assert.match(await page.locator("#results").innerText(), /quiz/i);
  await page.close();
});

test("the popup reports the connection", async () => {
  const page = await context.newPage();
  await page.goto(`chrome-extension://${EXTENSION_ID}/popup.html`);
  await page.waitForFunction(() => !document.getElementById("main").hidden);
  assert.match(await page.locator("#status").innerText(), /vault · \d+ pages/);
  await page.close();
});
