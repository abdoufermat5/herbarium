// Dev tool, not shipped: `pnpm preview:web` serves the real frontend in a
// browser, backed by the real vault core through `herbarium mcp`, on a seeded
// throwaway copy of the demo vault (e2e/tools/demo-vault.mjs). Lets UI work be
// checked in Chromium without the Tauri webview. Only loaded in the
// `web-preview` Vite mode; see src/preview/ipc.ts for the browser half.
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { herbariumBin, ROOT } from "../e2e/helpers.mjs";
import { seedDemoVault, writeDemoVault } from "../e2e/tools/demo-vault.mjs";
import { mcpClient } from "../e2e/tools/mcp.mjs";

const read = (file) => readFileSync(join(ROOT, "desktop", "src", file), "utf8");
const SHIM_JS = read("storage_shim.js");
const BRIDGE_JS = read("reader_bridge.js");
const RECALL_JS = read("recall.js");

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
      const mcp = mcpClient(herbariumBin(), vault);
      const seeded = mcp.ready.then(() => seedDemoVault(vault, mcp.call));
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
            // `reads.mark` is UI-only, so the MCP server lacks it: keep reads.json here.
            if (name === "reads.mark") return send(200, "application/json", JSON.stringify(markRead(vault, args.id)));
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
    /**
     * Install the IPC stand-in before the app's entry runs (one module, so the
     * order is guaranteed). `?scratch=<name>` loads `/src/preview/<name>.ts`
     * instead of the app: a place for throwaway component galleries.
     */
    transformIndexHtml: {
      order: "pre",
      handler: (html) =>
        html.replace(
          /<script type="module" src="\/src\/main\.ts[^"]*"><\/script>/,
          `<script type="module">
      await import("/src/preview/ipc.ts");
      const scratch = new URLSearchParams(location.search).get("scratch");
      if (scratch && /^[\\w-]+$/.test(scratch)) await import(/* @vite-ignore */ \`/src/preview/\${scratch}.ts\`);
      else await import("/src/main.ts");
    </script>`,
        ),
    },
  };
}
