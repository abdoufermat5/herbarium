// End to end: the `herbarium` binary's command-line faces, each run as a
// real process against a throwaway vault: `add`, `import`, the native
// messaging host (framed stdin/stdout), `native-host install`, and the MCP
// server over stdio.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync, execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { cleanup, herbariumBin, tempDir, vaultPages } from "./helpers.mjs";

let bin;
const dirs = [];

before(() => {
  bin = herbariumBin();
});
after(() => cleanup(...dirs));

function run(args, opts = {}) {
  const out = spawnSync(bin, args, { encoding: "utf8", ...opts, env: { ...process.env, ...(opts.env ?? {}) } });
  return { code: out.status, stdout: out.stdout, stderr: out.stderr };
}

const claudeExport = (artifacts) =>
  JSON.stringify([
    {
      uuid: "conv-1",
      name: "Learning chat",
      created_at: "2024-05-01T08:00:00Z",
      chat_messages: [
        { uuid: "h1", sender: "human", text: "Make me pages", created_at: "2024-05-01T08:00:00Z" },
        {
          uuid: "a1",
          sender: "assistant",
          created_at: "2024-05-01T08:01:00Z",
          content: artifacts.map((a) => ({
            type: "tool_use",
            name: "artifacts",
            input: { id: a.id, command: "create", type: a.type ?? "text/html", title: a.title, content: a.content },
          })),
        },
      ],
    },
  ]);

test("herbarium add and herbarium import write pages with their source", () => {
  const work = tempDir("cli");
  dirs.push(work);
  const vault = join(work, "vault");
  const page = join(work, "note.html");
  writeFileSync(page, "<!doctype html><title>Note</title><p>hello</p>");

  const added = run(["add", "--vault", vault, "--tag", "x", "--tool", "Claude Code", "--prompt", "write a note", page]);
  assert.equal(added.code, 0, added.stderr);
  assert.match(added.stdout, /^note\therbarium-app:\/\/open\/note$/m);
  const note = vaultPages(vault).find((p) => p.id === "note");
  assert.deepEqual(note.meta.tags, ["x"]);
  assert.equal(note.meta.ext.source.tool, "Claude Code");

  const exportJson = claudeExport([
    { id: "a", title: "Alpha", content: "<!doctype html><html><head><title>Alpha</title></head><body><p>a</p></body></html>" },
    { id: "b", title: "Beta", content: "<!doctype html><html><head><title>Beta</title></head><body><p>b</p></body></html>" },
    { id: "r", title: "Widget", type: "application/vnd.ant.react", content: "export default () => null" },
  ]);
  writeFileSync(join(work, "conversations.json"), exportJson);
  // The same export as a .zip, the way Claude delivers it.
  execFileSync("python3", [
    "-c",
    "import sys, zipfile; z = zipfile.ZipFile(sys.argv[1], 'w'); z.write(sys.argv[2], 'data-2025/conversations.json'); z.close()",
    join(work, "export.zip"),
    join(work, "conversations.json"),
  ]);

  const dry = run(["import", "--vault", vault, "--dry-run", join(work, "export.zip")]);
  assert.equal(dry.code, 0, dry.stderr);
  assert.match(dry.stdout, /2 artifacts in 1 conversations \(2 new, 1 not standalone pages\)/);
  assert.equal(vaultPages(vault).length, 1, "a dry run saves nothing");

  const imported = run(["import", "--vault", vault, "--folder", "From Claude", join(work, "export.zip")]);
  assert.equal(imported.code, 0, imported.stderr);
  assert.match(imported.stdout, /imported 2, skipped 0/);
  const alpha = vaultPages(vault).find((p) => p.meta.title === "Alpha");
  assert.equal(alpha.folder, "From Claude");
  assert.equal(alpha.meta.ext.source.prompt, "Make me pages");
  assert.equal(alpha.meta.createdAt, Date.parse("2024-05-01T08:01:00Z"));

  const again = run(["import", "--vault", vault, join(work, "conversations.json")]);
  assert.match(again.stdout, /imported 0, skipped 2/, "an export is imported once");

  const bad = run(["import", "--vault", vault, page]);
  assert.equal(bad.code, 1);
  assert.match(bad.stderr, /not a conversations export/);
  assert.equal(run(["import"]).code, 2, "usage error");
});

/** Speak the native messaging protocol to a host process. */
function nativeHost(vault, env = {}) {
  const proc = spawn(bin, ["native-host", "--vault", vault, "chrome-extension://fimfpdpamppfkhnbgodhgegfaefenmml/"], {
    env: { ...process.env, ...env },
  });
  let buffer = Buffer.alloc(0);
  const waiting = [];
  proc.stdout.on("data", (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    while (buffer.length >= 4) {
      const len = buffer.readUInt32LE(0);
      if (buffer.length < 4 + len) break;
      const msg = JSON.parse(buffer.subarray(4, 4 + len).toString("utf8"));
      buffer = buffer.subarray(4 + len);
      waiting.shift()?.(msg);
    }
  });
  return {
    ask(message) {
      const body = Buffer.from(JSON.stringify(message));
      const head = Buffer.alloc(4);
      head.writeUInt32LE(body.length, 0);
      proc.stdin.write(Buffer.concat([head, body]));
      return new Promise((resolve) => waiting.push(resolve));
    },
    close() {
      proc.stdin.end();
      return new Promise((resolve) => proc.on("exit", resolve));
    },
  };
}

test("native messaging host: framed requests against the vault", async () => {
  const work = tempDir("native");
  dirs.push(work);
  const vault = join(work, "vault");
  const log = join(work, "launch.log");
  const host = nativeHost(vault, { HERBARIUM_LAUNCH_LOG: log });

  const pong = await host.ask({ id: 1, type: "ping" });
  assert.deepEqual([pong.id, pong.ok, pong.vault, pong.pages], [1, true, "vault", 0]);

  const big = "<p>" + "x".repeat(2 * 1024 * 1024) + "</p>";
  const saved = await host.ask({ id: 2, type: "save", html: `<!doctype html><title>Big</title>${big}`, url: "https://ex.com/a" });
  assert.equal(saved.ok, true, saved.error);
  assert.equal(saved.page.folder, "Inbox");

  const found = await host.ask({ id: 3, type: "lookup", url: "https://ex.com/a/" });
  assert.equal(found.pages[0].id, saved.page.id);
  const today = await host.ask({ id: 4, type: "today" });
  assert.equal(today.today.totalPages, 1);
  const opened = await host.ask({ id: 5, type: "open", page: saved.page.id });
  assert.equal(opened.ok, true);
  const bad = await host.ask({ id: 6, type: "nope" });
  assert.deepEqual([bad.id, bad.ok], [6, false]);
  assert.equal(await host.close(), 0);
  assert.match(readFileSync(log, "utf8"), new RegExp(`herbarium-app://open/${saved.page.id}`));
});

test("native-host install registers with the browsers it finds", { skip: process.platform === "win32" }, () => {
  const home = tempDir("home");
  dirs.push(home);
  const isMac = process.platform === "darwin";
  const chromium = isMac ? join(home, "Library/Application Support/Chromium") : join(home, ".config/chromium");
  const firefox = isMac ? join(home, "Library/Application Support/Mozilla") : join(home, ".mozilla");
  mkdirSync(chromium, { recursive: true });
  mkdirSync(firefox, { recursive: true });

  const out = run(["native-host", "install", "--extension-id", "abcdefghijklmnopabcdefghijklmnop"], { env: { HOME: home } });
  assert.equal(out.code, 0, out.stderr);
  assert.match(out.stdout, /Chromium: /);
  assert.match(out.stdout, /Firefox: /);
  assert.doesNotMatch(out.stdout, /Chrome: |Brave: |Edge: /, "browsers that are not installed are left alone");

  const chromeManifest = JSON.parse(readFileSync(join(chromium, "NativeMessagingHosts/app.herbarium.host.json"), "utf8"));
  assert.equal(chromeManifest.path, bin);
  assert.deepEqual(chromeManifest.allowed_origins, [
    "chrome-extension://fimfpdpamppfkhnbgodhgegfaefenmml/",
    "chrome-extension://abcdefghijklmnopabcdefghijklmnop/",
  ]);
  const ffDir = isMac ? "NativeMessagingHosts" : "native-messaging-hosts";
  const ffManifest = JSON.parse(readFileSync(join(firefox, ffDir, "app.herbarium.host.json"), "utf8"));
  assert.deepEqual(ffManifest.allowed_extensions, ["herbarium@herbarium.app"]);

  const removed = run(["native-host", "uninstall"], { env: { HOME: home } });
  assert.equal(removed.code, 0);
  assert.equal(existsSync(join(chromium, "NativeMessagingHosts/app.herbarium.host.json")), false);
});

/** A JSON-RPC session with `herbarium mcp`. */
function mcp(vault) {
  const proc = spawn(bin, ["mcp", "--vault", vault]);
  let buffer = "";
  const waiting = new Map();
  proc.stdout.on("data", (chunk) => {
    buffer += chunk.toString("utf8");
    let nl;
    while ((nl = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, nl);
      buffer = buffer.slice(nl + 1);
      if (!line.trim()) continue;
      const msg = JSON.parse(line);
      waiting.get(msg.id)?.(msg);
      waiting.delete(msg.id);
    }
  });
  let id = 0;
  return {
    call(method, params) {
      const msgId = ++id;
      proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: msgId, method, params }) + "\n");
      return new Promise((resolve) => waiting.set(msgId, resolve));
    },
    tool(name, args) {
      return this.call("tools/call", { name, arguments: args }).then((r) => r.result);
    },
    close() {
      proc.stdin.end();
      return new Promise((resolve) => proc.on("exit", resolve));
    },
  };
}

test("MCP server: tools, resources, prompts, proposals and paths", async () => {
  const work = tempDir("mcp");
  dirs.push(work);
  const vault = join(work, "vault");
  const s = mcp(vault);

  const init = await s.call("initialize", { protocolVersion: "2025-06-18" });
  assert.equal(init.result.serverInfo.name, "herbarium");
  assert.ok(init.result.capabilities.resources && init.result.capabilities.prompts);
  const tools = (await s.call("tools/list")).result.tools.map((t) => t.name);
  for (const name of ["pages_create", "pages_links", "paths_create", "today_summary", "searches_save", "proposals_list"]) {
    assert.ok(tools.includes(name), name);
  }
  assert.ok(!tools.includes("proposals_accept") && !tools.includes("agents_configure"), "approval stays with the user");

  const created = await s.tool("pages_create", {
    html: "<!doctype html><title>Cargo</title><p>Cargo.lock pins versions.</p><a href='herbarium-app://open/rust'>rust</a>",
    tags: ["rust"],
    source: { tool: "Claude Code", prompt: "explain cargo" },
  });
  assert.equal(created.isError, false);
  const id = created.structuredContent.id;
  const rust = await s.tool("pages_create", { html: "<!doctype html><title>Rust</title><p>r</p>" });
  assert.equal(rust.structuredContent.id, "rust");

  const read = await s.call("resources/read", { uri: `herbarium://page/${id}/text` });
  assert.match(read.result.contents[0].text, /Cargo\.lock pins versions/);
  const prompt = await s.call("prompts/get", { name: "save_page", arguments: { topic: "lifetimes" } });
  assert.match(prompt.result.messages[0].content.text, /lifetimes/);

  const hits = await s.tool("pages_search", { query: "tag:rust explain" });
  assert.equal(hits.structuredContent.items[0].id, id, "the prompt is searchable and filters apply");
  const links = await s.tool("pages_links", { id: "rust" });
  assert.equal(links.structuredContent.backlinks[0].id, id);
  const path = await s.tool("paths_create", { name: "Rust course", pages: ["rust", id] });
  assert.deepEqual(path.structuredContent.pages, ["rust", id]);

  // With review on (a user setting), an agent rewrite waits as a proposal.
  writeFileSync(join(vault, ".herbarium", "agents.json"), JSON.stringify({ reviewEdits: true }));
  const rewrite = await s.tool("pages_set_html", { id, html: "<!doctype html><title>Cargo 2</title><p>v2</p>" });
  assert.equal(rewrite.structuredContent.pendingApproval, true);
  const proposals = await s.tool("proposals_list", {});
  assert.equal(proposals.structuredContent.items[0].id, id);
  assert.match(vaultPages(vault).find((p) => p.id === id).html, /Cargo\.lock pins/, "the page is unchanged");

  assert.equal(await s.close(), 0);
});
