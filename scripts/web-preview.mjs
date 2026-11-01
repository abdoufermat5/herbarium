// Dev tool, not shipped: `pnpm preview:web` serves the real frontend in a
// browser, backed by the real vault core through `herbarium mcp`, on a seeded
// throwaway copy of the demo vault (e2e/tools/demo-vault.mjs). Lets UI work be
// checked in Chromium without the Tauri webview. Only loaded in the
// `web-preview` Vite mode; see src/preview/ipc.ts for the browser half.
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { herbariumBin, ROOT } from "../e2e/helpers.mjs";
import { PAGES, writeDemoVault } from "../e2e/tools/demo-vault.mjs";

const DAY = 86_400_000;
const read = (file) => readFileSync(join(ROOT, "desktop", "src", file), "utf8");
const SHIM_JS = read("storage_shim.js");
const BRIDGE_JS = read("reader_bridge.js");
const RECALL_JS = read("recall.js");

/** JSON-RPC over the stdio of a long-lived `herbarium mcp` process. */
function mcpClient(vault) {
  const child = spawn(herbariumBin(), ["mcp", "--vault", vault], { stdio: ["pipe", "pipe", "inherit"] });
  const pending = new Map();
  let seq = 0;
  createInterface({ input: child.stdout }).on("line", (line) => {
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      return;
    }
    const waiter = pending.get(msg.id);
    if (!waiter) return;
    pending.delete(msg.id);
    waiter(msg);
  });
  const request = (method, params) =>
    new Promise((resolve) => {
      const id = ++seq;
      pending.set(id, resolve);
      child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  /** Run one core operation (`area.verb`); rejects with the core's message. */
  async function call(name, args = {}) {
    const msg = await request("tools/call", { name: name.replaceAll(".", "_"), arguments: args ?? {} });
    if (msg.error) throw new Error(msg.error.message);
    const text = msg.result?.content?.[0]?.text ?? "null";
    if (msg.result?.isError) throw new Error(text);
    return JSON.parse(text);
  }
  return {
    ready: request("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "web-preview", version: "1" },
    }),
    call,
    close: () => child.kill(),
  };
}

/**
 * Tags, a reading path and a spread of review states, so every vitality level
 * and the due queue show up. Review dates are written into the sidecars, then
 * the vault is rescanned.
 */
async function seed(mcp, vault) {
  for (const p of PAGES) await mcp.call("pages.update", { id: p.id, tags: p.tags });
  await mcp.call("paths.create", { name: "Networking basics", pages: ["dns", "tcp-handshake", "http-caching"] });
  for (const id of ["big-o", "mitosis", "french-revolution", "color-contrast", "photosynthesis", "tcp-handshake", "dns"]) {
    await mcp.call("review.complete", { id, grade: "good" });
  }
  const now = Date.now();
  // [lastReview days ago, next review in days (negative: overdue), stability days]
  const states = {
    "big-o": [1, 9, 10],
    mitosis: [6, 4, 10],
    "french-revolution": [2, 1, 3],
    "color-contrast": [12, -2, 10],
    photosynthesis: [5, -1, 4],
    "tcp-handshake": [40, -30, 10],
    dns: [3, 0, 3],
  };
  for (const file of sidecars(vault)) {
    const meta = JSON.parse(readFileSync(file, "utf8"));
    const state = states[meta.id];
    if (!state) continue;
    const [ago, next, stability] = state;
    meta.lastReview = now - ago * DAY;
    meta.nextReview = next === 0 ? now - 60_000 : now + next * DAY;
    meta.intervalMinutes = Math.round(((meta.nextReview - meta.lastReview) / DAY) * 1440);
    meta.ext ??= {};
    const log = Array.from({ length: 3 }, (_, i) => ({ at: meta.lastReview - i * 3 * DAY, grade: "good", intervalMinutes: 1440 * (3 - i) }));
    meta.ext.review = { count: log.length, log, stability, difficulty: 5 };
    writeFileSync(file, JSON.stringify(meta, null, 2));
  }
  await mcp.call("vault.rescan").catch(() => {});
}

/** Every page sidecar (`*.json` next to a `.html`) under the vault. */
function sidecars(dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith(".")) continue;
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...sidecars(path));
    else if (entry.name.endsWith(".json")) out.push(path);
  }
  return out;
}

function inject(html, recall) {
  const script =
    `<script>window.__herbariumState={};window.__herbariumPersist=false;${SHIM_JS}</script>` +
    `<script>${BRIDGE_JS}</script>` +
    (recall ? `<script>${RECALL_JS}</script>` : "");
  const head = html.search(/<head[^>]*>/i);
  if (head < 0) return script + html;
  const at = html.indexOf(">", head) + 1;
  return html.slice(0, at) + script + html.slice(at);
}

async function body(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  return chunks.length ? JSON.parse(Buffer.concat(chunks).toString("utf8")) : {};
}

export function webPreview() {
  return {
    name: "herbarium-web-preview",
    configureServer(server) {
      const work = mkdtempSync(join(tmpdir(), "herbarium-web-preview-"));
      const vault = join(work, "Learning");
      mkdirSync(vault, { recursive: true });
      writeDemoVault(vault);
      const mcp = mcpClient(vault);
      const seeded = mcp.ready.then(() => seed(mcp, vault));
      const stop = () => {
        mcp.close();
        rmSync(work, { recursive: true, force: true });
      };
      // A config change restarts the server: the old vault goes with it.
      server.httpServer?.once("close", stop);
      process.once("exit", stop);
      for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, () => process.exit());
      server.middlewares.use(async (req, res, next) => {
        const url = new URL(req.url, "http://preview");
        if (!url.pathname.startsWith("/__preview/")) return next();
        const send = (status, type, payload) => {
          res.statusCode = status;
          res.setHeader("content-type", type);
          res.end(payload);
        };
        try {
          await seeded;
          if (url.pathname === "/__preview/vault") return send(200, "application/json", JSON.stringify({ path: vault }));
          if (url.pathname === "/__preview/op" && req.method === "POST") {
            const { name, args } = await body(req);
            return send(200, "application/json", JSON.stringify(await mcp.call(name, args) ?? null));
          }
          const page = url.pathname.match(/^\/__preview\/page\/([^/]+)/);
          if (page) {
            const { html } = await mcp.call("pages.get", { id: decodeURIComponent(page[1]), format: "html" });
            return send(200, "text/html; charset=utf-8", inject(html, url.searchParams.get("recall") === "1"));
          }
          return send(404, "text/plain", "not found");
        } catch (e) {
          return send(400, "text/plain; charset=utf-8", e instanceof Error ? e.message : String(e));
        }
      });
    },
    /** Install the IPC stand-in before the app's entry runs (one module, so the order is guaranteed). */
    transformIndexHtml: {
      order: "pre",
      handler: (html) =>
        html.replace(
          /<script type="module" src="\/src\/main\.ts[^"]*"><\/script>/,
          '<script type="module">await import("/src/preview/ipc.ts"); await import("/src/main.ts");</script>',
        ),
    },
  };
}
