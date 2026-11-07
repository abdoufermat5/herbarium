// Fail when the main JS chunk of the web build outgrows its budget.
//
//   pnpm build && node scripts/bundle-budget.mjs
//
// The main chunk is the entry script `dist/index.html` loads. The limit is
// `mainJsKB` in scripts/bundle-budget.json (kilobytes of the file on disk, 1 KB
// = 1024 bytes); lower it whenever the bundle shrinks. Sizes are printed for
// every chunk so a regression is easy to place.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");
const budget = JSON.parse(readFileSync(join(root, "scripts", "bundle-budget.json"), "utf8"));

const html = readFileSync(join(dist, "index.html"), "utf8");
const entry = /<script[^>]*type="module"[^>]*src="([^"]+)"/.exec(html)?.[1];
if (!entry) {
  console.error("bundle-budget: no module script in dist/index.html (run `pnpm build` first)");
  process.exit(2);
}

const kb = (bytes) => (bytes / 1024).toFixed(1).padStart(8);
const assets = join(dist, "assets");
let total = 0;
for (const name of readdirSync(assets).filter((n) => n.endsWith(".js")).sort()) {
  const file = join(assets, name);
  const size = statSync(file).size;
  total += size;
  const gz = gzipSync(readFileSync(file)).length;
  console.log(`${kb(size)} KB  ${kb(gz)} KB gzip  ${name}`);
}
console.log(`${kb(total)} KB  total JS`);

const main = statSync(join(dist, entry.replace(/^\//, ""))).size;
const limit = budget.mainJsKB * 1024;
console.log(`main chunk ${entry}: ${(main / 1024).toFixed(1)} KB, budget ${budget.mainJsKB} KB`);
if (main > limit) {
  console.error(`bundle-budget: main chunk is ${(main / 1024 - budget.mainJsKB).toFixed(1)} KB over budget`);
  process.exit(1);
}
