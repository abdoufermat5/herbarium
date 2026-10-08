// A small W3C WebDriver client and the desktop harness: an X display (Xvfb),
// tauri-driver in front of WebKitWebDriver, and the Herbarium app built with
// its interface embedded. Only what the desktop suite needs; no dependencies.
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { join } from "node:path";
import { ROOT } from "./helpers.mjs";

const ELEMENT = "element-6066-11e4-a52e-4f735466cecf";

function has(cmd) {
  return spawnSync("sh", ["-c", `command -v ${cmd}`]).status === 0;
}

/** Why the desktop suite cannot run here, or null when it can. */
export function desktopUnavailable() {
  if (process.platform !== "linux") return "desktop e2e runs on Linux (WebKitWebDriver)";
  for (const cmd of ["tauri-driver", "WebKitWebDriver", "Xvfb"]) {
    if (!has(cmd)) return `${cmd} is not installed`;
  }
  return null;
}

/** The app with its interface embedded (not the dev server), built once. */
export function desktopBin() {
  if (process.env.HERBARIUM_DESKTOP_BIN) return process.env.HERBARIUM_DESKTOP_BIN;
  const target = join(ROOT, "src-tauri", "target", "desktop-e2e");
  const bin = join(target, "debug", "herbarium");
  if (!existsSync(bin) || process.env.HERBARIUM_REBUILD) {
    execFileSync("pnpm", ["build"], { cwd: ROOT, stdio: "inherit" });
    execFileSync(
      "cargo",
      ["build", "--manifest-path", join(ROOT, "src-tauri", "Cargo.toml"), "-p", "herbarium", "--features", "tauri/custom-protocol"],
      { stdio: "inherit", env: { ...process.env, CARGO_TARGET_DIR: target } },
    );
  }
  return bin;
}

function freePort() {
  return new Promise((resolve) => {
    const srv = createServer();
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Start a display and tauri-driver, then the app with `env` (HOME, config…),
 * and return a session. `close()` stops everything.
 */
export async function launchApp(env) {
  const display = `:${100 + Math.floor(Math.random() * 400)}`;
  const xvfb = spawn("Xvfb", [display, "-screen", "0", "1400x900x24", "-nolisten", "tcp"], { stdio: "ignore" });
  await sleep(500);
  const port = await freePort();
  const nativePort = await freePort();
  let log = "";
  const driver = spawn("tauri-driver", ["--port", String(port), "--native-port", String(nativePort)], {
    env: { ...process.env, ...env, DISPLAY: display, NO_AT_BRIDGE: "1" },
  });
  driver.stdout.on("data", (d) => (log += d));
  driver.stderr.on("data", (d) => (log += d));
  const base = `http://127.0.0.1:${port}`;
  const stop = () => {
    driver.kill("SIGTERM");
    xvfb.kill("SIGTERM");
  };
  try {
    for (let i = 0; ; i++) {
      try {
        if ((await fetch(`${base}/status`)).ok) break;
      } catch {}
      if (i > 50) throw new Error(`tauri-driver did not start:\n${log}`);
      await sleep(100);
    }
    const created = await call(base, "POST", "/session", {
      capabilities: { alwaysMatch: { "tauri:options": { application: desktopBin() } } },
    });
    const session = new Session(`${base}/session/${created.sessionId}`, () => log);
    // The X display the app is on, for tools that record or drive it.
    session.display = display;
    session.close = async () => {
      await call(`${base}/session/${created.sessionId}`, "DELETE", "").catch(() => {});
      stop();
    };
    return session;
  } catch (e) {
    stop();
    throw e;
  }
}

async function call(base, method, path, body) {
  const res = await fetch(`${base}${path}`, {
    method,
    headers: body === undefined ? {} : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(`${method} ${path}: ${json.value?.error ?? res.status}: ${json.value?.message ?? ""}`);
  return json.value;
}

export class Session {
  constructor(url, log) {
    this.url = url;
    this.log = log;
  }

  cmd(method, path, body) {
    return call(this.url, method, path, body);
  }

  /** Run `fn` (a function's source) in the app's window with `args`. */
  exec(fn, ...args) {
    return this.cmd("POST", "/execute/sync", { script: `return (${fn}).apply(null, arguments)`, args });
  }

  execAsync(fn, ...args) {
    return this.cmd("POST", "/execute/async", {
      script: `const done = arguments[arguments.length - 1]; Promise.resolve((${fn}).apply(null, Array.prototype.slice.call(arguments, 0, -1))).then(done, (e) => done({ error: String(e) }));`,
      args,
    });
  }

  async waitFor(check, what, ms = 15000) {
    const end = Date.now() + ms;
    let last;
    while (Date.now() < end) {
      try {
        last = await check();
        if (last) return last;
      } catch (e) {
        last = e;
      }
      await sleep(150);
    }
    throw new Error(`timed out waiting for ${what} (last: ${last})`);
  }

  /** A visible element whose text includes `text`, among `selector` matches. */
  async find(text, selector = "button, a, [role=menuitem], [role=radio], label, h1, h2") {
    return this.waitFor(
      () =>
        this.exec(
          (text, selector) => {
            const visible = (el) => {
              const r = el.getBoundingClientRect();
              return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== "hidden";
            };
            const label = (el) => `${el.textContent} ${el.getAttribute("aria-label") ?? ""} ${el.getAttribute("title") ?? ""}`;
            return [...document.querySelectorAll(selector)].find((el) => visible(el) && label(el).includes(text)) ?? null;
          },
          text,
          selector,
        ),
      `“${text}”`,
    );
  }

  /** Click it; a click intercepted for a moment (a toast, a fade-in) is retried. */
  async click(text, selector) {
    for (let attempt = 0; ; attempt++) {
      const el = await this.find(text, selector);
      try {
        await this.cmd("POST", `/element/${el[ELEMENT]}/click`, {});
        return;
      } catch (e) {
        if (attempt >= 10 || !/intercepted|not interactable|stale/.test(e.message)) throw e;
        await sleep(200);
      }
    }
  }

  async type(selector, value) {
    const el = await this.waitFor(() => this.exec((s) => document.querySelector(s), selector), selector);
    await this.cmd("POST", `/element/${el[ELEMENT]}/clear`, {}).catch(() => {});
    await this.cmd("POST", `/element/${el[ELEMENT]}/value`, { text: value });
  }

  text() {
    return this.exec(() => document.body.innerText);
  }

  async waitText(text, ms) {
    return this.waitFor(async () => (await this.text()).includes(text), `text “${text}”`, ms);
  }

  async screenshot(path) {
    const png = await this.cmd("GET", "/screenshot");
    writeFileSync(path, Buffer.from(png, "base64"));
  }

  /** Enter the reader's page frame (`null` goes back to the app). */
  async frame(selector) {
    if (!selector) return this.cmd("POST", "/frame", { id: null });
    const el = await this.waitFor(() => this.exec((s) => document.querySelector(s), selector), selector);
    return this.cmd("POST", "/frame", { id: el });
  }
}
