// Shared helpers for the end-to-end suites: the built `herbarium` binary,
// throwaway vaults, a local HTTPS server that answers for any host name, and
// reading what landed in a vault.
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, existsSync, rmSync, writeFileSync, statSync } from "node:fs";
import { createServer } from "node:https";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
export const EXTENSION_DIR = join(ROOT, "extension");
export const EXTENSION_ID = "fimfpdpamppfkhnbgodhgegfaefenmml";

/** The `herbarium` binary, built once (debug) unless HERBARIUM_BIN points at one. */
export function herbariumBin() {
  if (process.env.HERBARIUM_BIN) return process.env.HERBARIUM_BIN;
  const exe = process.platform === "win32" ? "herbarium.exe" : "herbarium";
  const bin = join(ROOT, "src-tauri", "target", "debug", exe);
  if (!existsSync(bin) || process.env.HERBARIUM_REBUILD) {
    execFileSync("cargo", ["build", "--manifest-path", join(ROOT, "src-tauri", "Cargo.toml"), "-p", "herbarium"], {
      stdio: "inherit",
    });
  }
  return bin;
}

export function tempDir(tag) {
  return mkdtempSync(join(tmpdir(), `herbarium-e2e-${tag}-`));
}

export function cleanup(...dirs) {
  // Retries: a process that just exited may still be finishing its writes.
  for (const d of dirs) rmSync(d, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
}

/** Every page in a vault: `{ id, folder, meta, html }`, read from the files. */
export function vaultPages(vault) {
  const out = [];
  const walk = (dir, folder) => {
    for (const name of readdirSync(dir)) {
      if (name.startsWith(".")) continue;
      const path = join(dir, name);
      if (statSync(path).isDirectory()) {
        walk(path, folder ? `${folder}/${name}` : name);
      } else if (name.endsWith(".json")) {
        const id = name.slice(0, -5);
        const htmlPath = join(dir, `${id}.html`);
        if (!existsSync(htmlPath)) continue;
        out.push({ id, folder, meta: JSON.parse(readFileSync(path, "utf8")), html: readFileSync(htmlPath, "utf8") });
      }
    }
  };
  if (existsSync(vault)) walk(vault, null);
  return out;
}

/** A self-signed certificate for the local HTTPS server. */
export function certificate(dir) {
  const key = join(dir, "key.pem");
  const cert = join(dir, "cert.pem");
  execFileSync("openssl", [
    "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "2",
    "-subj", "/CN=herbarium-e2e", "-keyout", key, "-out", cert,
  ], { stdio: "ignore" });
  return { key: readFileSync(key), cert: readFileSync(cert) };
}

/**
 * An HTTPS server on a free port that answers for any host name: `routes`
 * maps "host/path" (path without the query) to a handler
 * `(req, url) => { status?, type?, body, headers? }`.
 */
export async function siteServer(dir, routes) {
  const requests = [];
  const server = createServer(certificate(dir), (req, res) => {
    const url = new URL(req.url, `https://${req.headers.host}`);
    const key = `${url.hostname}${url.pathname}`;
    requests.push({ key, url, headers: req.headers });
    const handler = routes[key];
    if (!handler) {
      res.writeHead(404, { "content-type": "text/plain" });
      res.end("not found");
      return;
    }
    const out = handler(req, url);
    res.writeHead(out.status ?? 200, {
      "content-type": out.type ?? "text/html; charset=utf-8",
      "access-control-allow-origin": "*",
      ...(out.headers ?? {}),
    });
    res.end(out.body);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return { port: server.address().port, requests, close: () => new Promise((r) => server.close(r)) };
}

/** Register the native host for a Chromium profile directory. */
export function installNativeHost(profileDir, vault, launchLog) {
  const hostDir = join(profileDir, "NativeMessagingHosts");
  mkdirSync(hostDir, { recursive: true });
  const wrapper = join(profileDir, "herbarium-host.sh");
  writeFileSync(
    wrapper,
    `#!/bin/sh\nHERBARIUM_LAUNCH_LOG='${launchLog}' exec '${herbariumBin()}' native-host --vault '${vault}' "$@"\n`,
    { mode: 0o755 },
  );
  writeFileSync(
    join(hostDir, "app.herbarium.host.json"),
    JSON.stringify({
      name: "app.herbarium.host",
      description: "Herbarium (e2e)",
      path: wrapper,
      type: "stdio",
      allowed_origins: [`chrome-extension://${EXTENSION_ID}/`],
    }),
  );
}

/** Wait until `check()` returns a truthy value (or throw after `ms`). */
export async function waitFor(check, ms = 10000, what = "condition") {
  const until = Date.now() + ms;
  let last;
  while (Date.now() < until) {
    try {
      last = await check();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`timed out waiting for ${what}: ${last instanceof Error ? last.message : JSON.stringify(last)}`);
}
