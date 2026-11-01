// Browser half of `pnpm preview:web` (scripts/web-preview.mjs): stands in for
// the Tauri IPC so the real frontend runs in a plain browser. Vault operations
// go to the real core through the dev server; desktop-only commands get inert
// answers. Injected before main.ts only in the `web-preview` Vite mode; never
// part of the app build. `?onboarding` starts without a vault.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Config, PreviewDigest } from "../lib/types";

const params = new URLSearchParams(location.search);
const PREVIEWS_KEY = `herbarium.preview.digests:${location.port}`;

/** The dev server's seeded vault (scripts/web-preview.mjs answers with `{ path }`). */
async function seededVault(): Promise<string> {
  const body: unknown = await (await fetch("/__preview/vault")).json();
  if (body && typeof body === "object" && "path" in body && typeof body.path === "string") return body.path;
  throw new Error("web preview: the dev server did not report a vault");
}

const vault = params.has("onboarding") ? null : await seededVault();

const config: Config = {
  vaultPath: vault,
  recentVaults: vault ? [vault] : [],
  closeToTray: false,
  captureShortcut: null,
  watchDownloads: false,
  watchClipboard: false,
};

/** Page previews are saved by a UI-only operation; keep them in the browser. */
const previews: Record<string, PreviewDigest> = JSON.parse(localStorage.getItem(PREVIEWS_KEY) ?? "{}");

async function op(name: string, args: Record<string, unknown> = {}): Promise<unknown> {
  switch (name) {
    case "previews.list": {
      const pages = (await op("pages.list")) as Array<{ id: string }>;
      return { previews, missing: pages.map((p) => p.id).filter((id) => !previews[id]) };
    }
    case "previews.save":
      previews[args.id as string] = args.digest as PreviewDigest;
      localStorage.setItem(PREVIEWS_KEY, JSON.stringify(previews));
      return null;
    case "storage.get":
      return { local: {}, personal: {}, shared: {} };
    case "storage.write":
    case "storage.clear":
    case "reads.mark":
      return null;
    case "review.configure":
    case "network.configure":
    case "agents.configure":
      return args;
  }
  const res = await fetch("/__preview/op", { method: "POST", body: JSON.stringify({ name, args }) });
  const text = await res.text();
  if (!res.ok) throw text;
  return JSON.parse(text);
}

/** Answers for desktop commands that have no vault-core equivalent. */
const desktop: Record<string, (args: Record<string, unknown>) => unknown> = {
  get_config: () => config,
  set_vault: () => config,
  create_vault: () => config,
  remove_recent_vault: () => config,
  set_close_to_tray: () => config,
  set_capture: () => config,
  take_deep_links: () => [],
  list_editors: () => [],
  browser_status: () => [],
  browser_setup: () => ({ browsers: [], extensionDir: "", firefoxExtensionDir: "", firefoxPackage: null }),
  claude_code_status: () => ({ path: null, version: null, error: "Not available in the browser preview", chosen: false }),
  ai_settings: () => ({ provider: "anthropic", model: "", baseUrl: null, keys: [], providers: [] }),
  ai_models: () => ({ models: [], recommended: null }),
  github_settings: () => ({ hasToken: false, login: null, repo: "" }),
  check_update: () => null,
  update_managed_by: () => null,
  "plugin:app|version": () => "preview",
  "plugin:app|name": () => "Herbarium",
  "plugin:notification|is_permission_granted": () => false,
  "plugin:notification|request_permission": () => "denied",
  "plugin:dialog|open": () => null,
  "plugin:dialog|save": () => null,
  "plugin:window|is_maximized": () => false,
  "plugin:window|is_fullscreen": () => false,
  "plugin:window|is_focused": () => true,
  "plugin:window|is_visible": () => true,
  "plugin:window|scale_factor": () => window.devicePixelRatio,
  "plugin:window|theme": () => (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"),
};

mockWindows("main");
mockIPC(
  (cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    if (cmd === "invoke_op") return op(args.name as string, (args.args ?? {}) as Record<string, unknown>);
    const answer = desktop[cmd];
    if (answer) return answer(args);
    console.debug(`[preview] no-op for "${cmd}"`, args);
    return null;
  },
  { shouldMockEvents: true },
);

/** Saved pages load from `herbarium://page/<id>`; serve them from the dev server instead. */
function rewrite(url: string): string {
  return url.startsWith("herbarium://page/") ? `/__preview/page/${url.slice("herbarium://page/".length)}` : url;
}
const srcDescriptor = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, "src")!;
Object.defineProperty(HTMLIFrameElement.prototype, "src", {
  ...srcDescriptor,
  set(value: string) {
    srcDescriptor.set!.call(this, rewrite(String(value)));
  },
});
const setAttribute = Element.prototype.setAttribute;
Element.prototype.setAttribute = function (name: string, value: string) {
  setAttribute.call(this, name, this instanceof HTMLIFrameElement && name === "src" ? rewrite(String(value)) : value);
};
