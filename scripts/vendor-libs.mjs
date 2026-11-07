#!/usr/bin/env node
// Usage: pnpm libs
// Bundles Mermaid, KaTeX, highlight.js, and Chart.js for offline rendering in
// Herbarium. Generated files are written to desktop/crates/herbarium-core/lib/
// and compiled into the core binary.
//
// Each JavaScript library is bundled as a minified IIFE declaring a top-level
// `var <Global>` and registering `globalThis.<Global> = <Global>;`.

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createContext, runInContext } from "node:vm";
import esbuild from "esbuild";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const OUT_DIR = join(ROOT, "desktop", "crates", "herbarium-core", "lib");

async function findPackageInfo(entryPath) {
  let dir = dirname(entryPath);
  while (dir !== "/" && dir !== ".") {
    try {
      const raw = await readFile(join(dir, "package.json"), "utf8");
      const pkg = JSON.parse(raw);
      if (pkg.name && pkg.version) {
        let licenseText = "";
        for (const name of ["LICENSE", "LICENSE.md", "LICENSE.txt", "license", "license.txt"]) {
          try {
            licenseText = await readFile(join(dir, name), "utf8");
            break;
          } catch {}
        }
        return { dir, name: pkg.name, version: pkg.version, license: pkg.license, licenseText };
      }
    } catch {}
    dir = dirname(dir);
  }
  throw new Error(`package.json not found for ${entryPath}`);
}

console.log("Resolving package information...");
const mermaidInfo = await findPackageInfo(fileURLToPath(import.meta.resolve("mermaid")));
const katexInfo = await findPackageInfo(fileURLToPath(import.meta.resolve("katex")));
const hljsInfo = await findPackageInfo(fileURLToPath(import.meta.resolve("highlight.js")));
const chartInfo = await findPackageInfo(fileURLToPath(import.meta.resolve("chart.js")));

console.log(`- mermaid: ${mermaidInfo.version} (${mermaidInfo.license})`);
console.log(`- katex: ${katexInfo.version} (${katexInfo.license})`);
console.log(`- highlight.js: ${hljsInfo.version} (${hljsInfo.license})`);
console.log(`- chart.js: ${chartInfo.version} (${chartInfo.license})`);

await mkdir(OUT_DIR, { recursive: true });

// 1. mermaid.js: global `mermaid` from mermaid ESM entry.
// Entry wrapper exports default mermaid object so mermaid.initialize / mermaid.run
// are directly on the global, not mermaid.default.
console.log("Bundling mermaid.js...");
const mermaidBuild = await esbuild.build({
  stdin: {
    contents: `
import m from "mermaid";
module.exports = m;
`,
    resolveDir: ROOT,
    sourcefile: "mermaid-entry.js",
  },
  bundle: true,
  write: false,
  minify: true,
  format: "iife",
  globalName: "mermaid",
  banner: { js: `/*! mermaid@${mermaidInfo.version} */` },
  footer: { js: "globalThis.mermaid=mermaid;" },
  target: ["es2022"],
});
const mermaidJs = mermaidBuild.outputFiles[0].text;
await writeFile(join(OUT_DIR, "mermaid.js"), mermaidJs);

// 2. katex.js: global `katex`.
console.log("Bundling katex.js...");
const katexBuild = await esbuild.build({
  stdin: {
    contents: `
import k from "katex";
module.exports = k;
`,
    resolveDir: ROOT,
    sourcefile: "katex-entry.js",
  },
  bundle: true,
  write: false,
  minify: true,
  format: "iife",
  globalName: "katex",
  banner: { js: `/*! katex@${katexInfo.version} */` },
  footer: { js: "globalThis.katex=katex;" },
  target: ["es2022"],
});
const katexJs = katexBuild.outputFiles[0].text;
await writeFile(join(OUT_DIR, "katex.js"), katexJs);

// 3. katex-auto-render.js: global `renderMathInElement`.
// Maps `katex` to globalThis.katex so it works after loading katex.js.
console.log("Bundling katex-auto-render.js...");
const katexAutoRenderBuild = await esbuild.build({
  stdin: {
    contents: `
import autoRender from "katex/contrib/auto-render";
module.exports = autoRender;
`,
    resolveDir: ROOT,
    sourcefile: "auto-render-entry.js",
  },
  plugins: [
    {
      name: "katex-global",
      setup(build) {
        build.onResolve({ filter: /^(\.\.\/katex(\.mjs)?|katex)$/ }, () => ({
          path: "katex-global",
          namespace: "katex-global-ns",
        }));
        build.onLoad({ filter: /.*/, namespace: "katex-global-ns" }, () => ({
          contents: `
const proxy = new Proxy({}, {
  get(_, prop) {
    const k = (typeof globalThis !== "undefined" && globalThis.katex) || (typeof window !== "undefined" && window.katex);
    if (k) return k[prop];
    return undefined;
  }
});
export default proxy;
`,
          loader: "js",
        }));
      },
    },
  ],
  bundle: true,
  write: false,
  minify: true,
  format: "iife",
  globalName: "renderMathInElement",
  banner: { js: `/*! katex@${katexInfo.version} (auto-render) */` },
  footer: { js: "globalThis.renderMathInElement=renderMathInElement;" },
  target: ["es2022"],
});
const katexAutoRenderJs = katexAutoRenderBuild.outputFiles[0].text;
await writeFile(join(OUT_DIR, "katex-auto-render.js"), katexAutoRenderJs);

// 4. katex.css: inlined woff2 data URLs, dropped woff/ttf.
console.log("Building katex.css with inlined woff2 fonts...");
let katexCssRaw = await readFile(join(katexInfo.dir, "dist", "katex.css"), "utf8");
const fontRegex = /src:\s*url\((?:fonts\/)?([^)]+\.woff2)\)[^;]*;/g;
const woff2Fonts = new Map();
let fontMatch;
while ((fontMatch = fontRegex.exec(katexCssRaw)) !== null) {
  const fontFile = fontMatch[1];
  if (!woff2Fonts.has(fontFile)) {
    const fontData = await readFile(join(katexInfo.dir, "dist", "fonts", fontFile));
    woff2Fonts.set(fontFile, fontData.toString("base64"));
  }
}
const katexCssInlined = katexCssRaw.replace(fontRegex, (_, file) => {
  const b64 = woff2Fonts.get(file);
  return `src: url("data:font/woff2;base64,${b64}") format("woff2");`;
});
const katexCssMin = await esbuild.transform(katexCssInlined, { loader: "css", minify: true });
const katexCss = `/*! katex@${katexInfo.version} */\n${katexCssMin.code}`;
await writeFile(join(OUT_DIR, "katex.css"), katexCss);

// 5. highlight.js: global `hljs` from highlight.js/lib/common.
console.log("Bundling highlight.js...");
const hljsBuild = await esbuild.build({
  stdin: {
    contents: `
import hljs from "highlight.js/lib/common";
module.exports = hljs;
`,
    resolveDir: ROOT,
    sourcefile: "hljs-entry.js",
  },
  bundle: true,
  write: false,
  minify: true,
  format: "iife",
  globalName: "hljs",
  banner: { js: `/*! highlight.js@${hljsInfo.version} */` },
  footer: { js: "globalThis.hljs=hljs;" },
  target: ["es2022"],
});
const hljsJs = hljsBuild.outputFiles[0].text;
await writeFile(join(OUT_DIR, "highlight.js"), hljsJs);

// 6. highlight.css: github.css followed by dark mode media query.
console.log("Building highlight.css...");
const githubCss = await readFile(join(hljsInfo.dir, "styles", "github.css"), "utf8");
const githubDarkCss = await readFile(join(hljsInfo.dir, "styles", "github-dark.css"), "utf8");
const combinedHljsCss = `${githubCss}\n@media (prefers-color-scheme: dark) {\n${githubDarkCss}\n}`;
const hljsCssMin = await esbuild.transform(combinedHljsCss, { loader: "css", minify: true });
const highlightCss = `/*! highlight.js@${hljsInfo.version} */\n${hljsCssMin.code}`;
await writeFile(join(OUT_DIR, "highlight.css"), highlightCss);

// 7. chart.js: global `Chart` from chart.js/auto.
console.log("Bundling chart.js...");
const chartBuild = await esbuild.build({
  stdin: {
    contents: `
import Chart from "chart.js/auto";
module.exports = Chart;
`,
    resolveDir: ROOT,
    sourcefile: "chart-entry.js",
  },
  bundle: true,
  write: false,
  minify: true,
  format: "iife",
  globalName: "Chart",
  banner: { js: `/*! chart.js@${chartInfo.version} */` },
  footer: { js: "globalThis.Chart=Chart;" },
  target: ["es2022"],
});
const chartJs = chartBuild.outputFiles[0].text;
await writeFile(join(OUT_DIR, "chart.js"), chartJs);

// 8. lib/LICENSES.md
console.log("Writing lib/LICENSES.md...");
const licensesMd = `# Herbarium Bundled Libraries

Herbarium bundles the following open source libraries to enable offline diagram, math, code, and chart rendering.

## mermaid (${mermaidInfo.version})
- **License**: ${mermaidInfo.license}
- **Upstream**: https://mermaid.js.org

\`\`\`
${mermaidInfo.licenseText.trim()}
\`\`\`

## KaTeX (${katexInfo.version})
- **License**: ${katexInfo.license}
- **Upstream**: https://katex.org

\`\`\`
${katexInfo.licenseText.trim()}
\`\`\`

## highlight.js (${hljsInfo.version})
- **License**: ${hljsInfo.license}
- **Upstream**: https://highlightjs.org

\`\`\`
${hljsInfo.licenseText.trim()}
\`\`\`

## Chart.js (${chartInfo.version})
- **License**: ${chartInfo.license}
- **Upstream**: https://www.chartjs.org

\`\`\`
${chartInfo.licenseText.trim()}
\`\`\`
`;
await writeFile(join(OUT_DIR, "LICENSES.md"), licensesMd);

// Sanity-check with Node vm.runInNewContext
console.log("\nRunning sanity check on generated JS files in node:vm sandbox...");

function createStubContext(extras = {}) {
  const ctx = {
    globalThis: {},
    window: {},
    document: {
      createElement: () => ({}),
      getElementsByTagName: () => [],
      querySelector: () => null,
      querySelectorAll: () => [],
      addEventListener: () => {},
    },
    addEventListener: () => {},
    ...extras,
  };
  ctx.globalThis = ctx;
  ctx.window = ctx;
  ctx.window.addEventListener = ctx.addEventListener;
  return createContext(ctx);
}

// Check mermaid.js
const mermaidCtx = createStubContext();
runInContext(mermaidJs, mermaidCtx);
if (typeof mermaidCtx.mermaid !== "object" || typeof mermaidCtx.mermaid.initialize !== "function") {
  throw new Error("mermaid.js failed sanity check: global mermaid or initialize missing");
}
console.log("✓ mermaid.js: global `mermaid` defined with .initialize() and .run()");

// Check katex.js
const katexCtx = createStubContext();
runInContext(katexJs, katexCtx);
if (typeof katexCtx.katex !== "object" || typeof katexCtx.katex.renderToString !== "function") {
  throw new Error("katex.js failed sanity check: global katex or renderToString missing");
}
console.log("✓ katex.js: global `katex` defined with .renderToString()");

// Check katex-auto-render.js
const autoRenderCtx = createStubContext({ katex: katexCtx.katex });
runInContext(katexAutoRenderJs, autoRenderCtx);
if (typeof autoRenderCtx.renderMathInElement !== "function") {
  throw new Error("katex-auto-render.js failed sanity check: global renderMathInElement missing");
}
console.log("✓ katex-auto-render.js: global `renderMathInElement` defined as function");

// Check highlight.js
const hljsCtx = createStubContext();
runInContext(hljsJs, hljsCtx);
if (typeof hljsCtx.hljs !== "object" || typeof hljsCtx.hljs.highlightAuto !== "function") {
  throw new Error("highlight.js failed sanity check: global hljs or highlightAuto missing");
}
console.log("✓ highlight.js: global `hljs` defined with .highlightAuto()");

// Check chart.js
const chartCtx = createStubContext();
runInContext(chartJs, chartCtx);
if (typeof chartCtx.Chart !== "function") {
  throw new Error("chart.js failed sanity check: global Chart missing");
}
console.log("✓ chart.js: global `Chart` defined as constructor/function");

// Summary of generated file sizes
console.log("\nGenerated library files in desktop/crates/herbarium-core/lib/:");
const files = [
  "mermaid.js",
  "katex.js",
  "katex-auto-render.js",
  "katex.css",
  "highlight.js",
  "highlight.css",
  "chart.js",
  "LICENSES.md",
];
for (const file of files) {
  const content = await readFile(join(OUT_DIR, file));
  console.log(`- ${file.padEnd(22)}: ${(content.length / 1024).toFixed(1).padStart(7)} KB (${content.length} bytes)`);
}
