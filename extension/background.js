// Herbarium extension background: talks to the Herbarium app through native
// messaging, saves pages and AI artifacts, marks pages already saved, and
// powers the `h` keyword in the address bar.
"use strict";

if (typeof importScripts === "function" && typeof HerbariumCapture === "undefined") {
  importScripts("capture.js");
}

const ext = globalThis.browser ?? globalThis.chrome;
const HOST = "app.herbarium.host";
const MAX_IMAGE = 4 * 1024 * 1024;
const MAX_INLINE = 24 * 1024 * 1024;
/** Resources fetched at once while inlining a page. */
const FETCH_CONCURRENCY = 6;

/* ------------------------------------------------------------- the host */

let port = null;
let nextId = 1;
const pending = new Map();

/** What a failed native connection means for the user, from the browser's terse message. */
function explain(raw) {
  const text = String(raw || "");
  if (/not found|no such native application/i.test(text)) {
    return "Herbarium isn't connected to this browser yet.";
  }
  if (/forbidden/i.test(text)) {
    return "Herbarium is connected to this browser, but not to this copy of the extension.";
  }
  if (/exited|failed to start|communicating/i.test(text)) {
    return "Herbarium couldn't start. It may have been moved or reinstalled.";
  }
  return text || "Herbarium is not connected";
}

function connect() {
  if (port) return port;
  const current = ext.runtime.connectNative(HOST);
  port = current;
  current.onMessage.addListener((msg) => {
    const waiter = pending.get(msg && msg.id);
    if (!waiter) return;
    pending.delete(msg.id);
    if (msg.ok) waiter.resolve(msg);
    else waiter.reject(new Error(msg.error || "Herbarium refused the request"));
  });
  current.onDisconnect.addListener(() => {
    const reason = explain((ext.runtime.lastError && ext.runtime.lastError.message) || (current.error && current.error.message));
    if (port === current) port = null;
    for (const waiter of pending.values()) waiter.reject(new Error(reason));
    pending.clear();
  });
  return current;
}

/** Send one request to the app; resolves with its reply. */
function ask(type, fields = {}, timeoutMs = 60000) {
  return new Promise((resolve, reject) => {
    const id = nextId++;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error("Herbarium did not answer"));
    }, timeoutMs);
    pending.set(id, {
      resolve: (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      reject: (e) => {
        clearTimeout(timer);
        reject(e);
      },
    });
    try {
      connect().postMessage({ id, type, ...fields });
    } catch (e) {
      clearTimeout(timer);
      pending.delete(id);
      port = null;
      reject(e);
    }
  });
}

/* ------------------------------------------------------- tab helpers */

async function runInTab(tabId, func, args = [], allFrames = false) {
  const results = await ext.scripting.executeScript({ target: { tabId, allFrames }, func, args });
  return results.map((r) => r.result);
}

async function pageInfo(tabId) {
  const [info] = await runInTab(tabId, HerbariumCapture.pageInfo);
  return info;
}

async function toDataUrl(blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
  }
  return `data:${blob.type};base64,${btoa(binary)}`;
}

function escapeAttr(value) {
  return value.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/ /g, "&nbsp;");
}

/** Run `fn` over `items`, at most `limit` at a time. `fn` must not throw. */
async function forEachLimit(items, limit, fn) {
  let next = 0;
  const worker = async () => {
    while (next < items.length) await fn(items[next++]);
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
}

/** The text of a stylesheet with its `url(...)`s made absolute, or null when it cannot be had. */
async function fetchStylesheet(href) {
  try {
    const res = await fetch(href, { credentials: "omit" });
    if (!res.ok) return null;
    const css = await res.text();
    // url(...) inside the sheet is relative to the sheet.
    return css.replace(/url\(\s*(['"]?)([^'")]+)\1\s*\)/g, (m, q, u) => {
      if (/^(data:|#)/i.test(u)) return m;
      try {
        return `url("${new URL(u, href).href}")`;
      } catch {
        return m;
      }
    });
  } catch {
    return null; // keep the link
  }
}

/** Inline a snapshot's stylesheets and images so it works offline. */
async function inlineResources(snap) {
  let html = snap.html;
  let budget = MAX_INLINE;

  // Stylesheets: fetched concurrently, then every `<link>` swapped in one pass.
  const sheets = new Map(); // attribute-escaped href -> <style> element
  await forEachLimit(snap.stylesheets, FETCH_CONCURRENCY, async (href) => {
    const css = await fetchStylesheet(href);
    if (css !== null) sheets.set(escapeAttr(href), `<style>${css.replace(/<\/style/gi, "<\\/style")}</style>`);
  });
  if (sheets.size) {
    html = html.replace(/<link[^>]*>/g, (tag) => {
      const href = /\shref="([^"]*)"/.exec(tag);
      return (href && sheets.get(href[1])) || tag;
    });
  }

  // Images: fetched concurrently within the byte budget, then every
  // attribute value that is one of those URLs replaced in one pass.
  const images = new Map(); // attribute-escaped src -> data URL
  await forEachLimit(snap.images, FETCH_CONCURRENCY, async (src) => {
    if (budget <= 0) return;
    try {
      const res = await fetch(src, { credentials: "omit" });
      if (!res.ok) return;
      const blob = await res.blob();
      if (blob.size > MAX_IMAGE || blob.size > budget || !blob.type.startsWith("image/")) return;
      budget -= blob.size;
      images.set(escapeAttr(src), await toDataUrl(blob));
    } catch {
      /* keep the remote URL */
    }
  });
  if (images.size) {
    // `="…"` with no markup inside: a stray quote in page text cannot pair up
    // with a later attribute and hide its URL.
    html = html.replace(/="([^"<>]*)"/g, (m, value) => {
      const data = images.get(value);
      return data === undefined ? m : `="${data}"`;
    });
  }
  return html;
}

/* ------------------------------------------------------------ actions */

/** Save the tab as a page (`mode: "page"`) or its selection. */
async function saveSnapshot(tabId, mode) {
  const [snap] = await runInTab(tabId, HerbariumCapture.snapshot, [mode]);
  if (!snap || snap.error) throw new Error(snap && snap.error === "no-selection" ? "Select some text first" : "This page cannot be captured");
  const html = await inlineResources(snap);
  const reply = await ask("save", { html, title: snap.title, url: snap.url });
  await refreshBadge(tabId, snap.url);
  return reply.page;
}

/**
 * The AI artifacts of the tab: from the conversation itself on Claude and
 * ChatGPT, else whole HTML documents shown as code on the page.
 */
async function scanArtifacts(tabId) {
  const info = await pageInfo(tabId);
  if (info.site === "claude" || info.site === "chatgpt") {
    const [conv] = await runInTab(tabId, HerbariumCapture.fetchConversation, [info.site]);
    if (conv && conv.json) {
      const listed = await ask("scan", { json: conv.json });
      return { source: "conversation", json: conv.json, candidates: listed.candidates, site: info.site };
    }
  }
  const [found] = await runInTab(tabId, HerbariumCapture.findCodeDocuments);
  const tool = { claude: "Claude", chatgpt: "ChatGPT", gemini: "Gemini" }[info.site] || null;
  const docs = (found && found.docs) || [];
  return {
    source: "page",
    site: info.site,
    candidates: docs.map((d, i) => ({ key: `page:${i}`, title: d.title || `${info.title} (${i + 1})`, bytes: d.html.length, tool, alreadyImported: false })),
    docs,
    prompt: (found && found.prompt) || "",
    url: info.url,
    tool,
  };
}

/** Save the chosen artifacts of a scan. */
async function saveArtifacts(scan, keys) {
  if (scan.source === "conversation") {
    const reply = await ask("import", { json: scan.json, keys });
    return reply.pages;
  }
  const pages = [];
  for (const key of keys) {
    const doc = scan.docs[Number(key.split(":")[1])];
    if (!doc) continue;
    const reply = await ask("save", { html: doc.html, title: doc.title, url: scan.url, tool: scan.tool, prompt: scan.prompt });
    pages.push(reply.page);
  }
  return pages;
}

/* -------------------------------------------------- "already saved" */

async function refreshBadge(tabId, url) {
  if (!/^https?:/i.test(url || "")) {
    await ext.action.setBadgeText({ tabId, text: "" });
    return;
  }
  try {
    const reply = await ask("lookup", { url }, 10000);
    const saved = reply.pages.length > 0;
    await ext.action.setBadgeText({ tabId, text: saved ? "✓" : "" });
    await ext.action.setBadgeBackgroundColor({ tabId, color: "#346538" });
    await ext.action.setTitle({
      tabId,
      title: saved ? `Saved in Herbarium as “${reply.pages[0].title}”` : "Save to Herbarium",
    });
  } catch {
    /* not connected: no badge */
  }
}

ext.tabs.onUpdated.addListener((tabId, change, tab) => {
  if (change.status === "complete") void refreshBadge(tabId, tab.url);
});
ext.tabs.onActivated.addListener(async ({ tabId }) => {
  try {
    const tab = await ext.tabs.get(tabId);
    void refreshBadge(tabId, tab.url);
  } catch {
    /* tab gone */
  }
});

/* ------------------------------------------------------ context menus */

function createMenus() {
  ext.contextMenus.removeAll(() => {
    ext.contextMenus.create({ id: "save-page", title: "Save page to Herbarium", contexts: ["page", "frame", "image", "link"] });
    ext.contextMenus.create({ id: "save-selection", title: "Save selection to Herbarium", contexts: ["selection"] });
  });
}
ext.runtime.onInstalled.addListener(createMenus);
ext.runtime.onStartup?.addListener(createMenus);

async function notify(tabId, text) {
  try {
    await ext.action.setBadgeText({ tabId, text });
    await ext.action.setBadgeBackgroundColor({ tabId, color: text === "!" ? "#9f2f2d" : "#346538" });
  } catch {
    /* ignore */
  }
}

ext.contextMenus.onClicked.addListener(async (info, tab) => {
  if (!tab || tab.id === undefined) return;
  try {
    await saveSnapshot(tab.id, info.menuItemId === "save-selection" ? "selection" : "page");
    await notify(tab.id, "✓");
  } catch (e) {
    console.error(e);
    await notify(tab.id, "!");
  }
});

ext.commands?.onCommand.addListener(async (command, tab) => {
  if (command !== "save-page") return;
  const target = tab || (await ext.tabs.query({ active: true, currentWindow: true }))[0];
  if (!target) return;
  try {
    await saveSnapshot(target.id, "page");
    await notify(target.id, "✓");
  } catch {
    await notify(target.id, "!");
  }
});

/* ---------------------------------------------- `h` in the address bar */

function escapeXml(s) {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&apos;");
}
const isFirefox = typeof globalThis.browser !== "undefined" && !!globalThis.browser.runtime.getBrowserInfo;

let suggestTimer = null;
ext.omnibox.onInputChanged.addListener((text, suggest) => {
  clearTimeout(suggestTimer);
  suggestTimer = setTimeout(async () => {
    if (!text.trim()) return;
    try {
      const reply = await ask("search", { query: text, limit: 6 }, 8000);
      suggest(
        reply.results.map((r) => ({
          content: `herbarium:${r.id}`,
          description: isFirefox ? `${r.title}${r.folder ? ` — ${r.folder}` : ""}` : `${escapeXml(r.title)}${r.folder ? ` <dim>— ${escapeXml(r.folder)}</dim>` : ""}`,
        })),
      );
    } catch {
      /* not connected */
    }
  }, 150);
});

ext.omnibox.onInputEntered.addListener(async (text) => {
  try {
    if (text.trim() === "today") {
      await ext.tabs.create({ url: ext.runtime.getURL("today.html") });
      return;
    }
    let id = text.startsWith("herbarium:") ? text.slice("herbarium:".length) : null;
    if (!id) {
      const reply = await ask("search", { query: text, limit: 1 }, 8000);
      id = reply.results[0] && reply.results[0].id;
    }
    if (id) await ask("open", { page: id });
  } catch (e) {
    console.error(e);
  }
});

/* ------------------------------------------ the toolbar icon follows the mode */

// Firefox switches the icon itself (`theme_icons` in its manifest). Chrome has
// no such key and a service worker cannot watch prefers-color-scheme, so an
// offscreen page watches it and reports here ("colorScheme").
async function watchColorScheme() {
  if (!ext.offscreen) return;
  try {
    if (await ext.offscreen.hasDocument()) return;
    await ext.offscreen.createDocument({
      url: "offscreen.html",
      reasons: ["MATCH_MEDIA"],
      justification: "Show the light or dark toolbar icon to match the system mode",
    });
  } catch (e) {
    console.error(e);
  }
}
// Each start of the worker (the offscreen page may have been closed meanwhile).
void watchColorScheme();

function showModeIcon(dark) {
  const mode = dark ? "dark" : "light";
  return ext.action.setIcon({
    path: { 16: `icons/${mode}-16.png`, 32: `icons/${mode}-32.png`, 64: `icons/${mode}-64.png` },
  });
}

/* ------------------------------------------------- popup and Today page */

ext.runtime.onMessage.addListener((msg, sender, reply) => {
  // Only the extension's own pages (popup, Today, the offscreen page) drive it.
  if (!sender || sender.id !== ext.runtime.id) return false;
  const run = async () => {
    switch (msg && msg.type) {
      case "colorScheme":
        return await showModeIcon(!!msg.dark);
      case "status":
        return await ask("ping", {}, 10000);
      case "pageInfo":
        return await pageInfo(msg.tabId);
      case "lookup":
        return await ask("lookup", { url: msg.url }, 10000);
      case "savePage":
        return { page: await saveSnapshot(msg.tabId, msg.mode) };
      case "scanArtifacts":
        return await scanArtifacts(msg.tabId);
      case "saveArtifacts":
        return { pages: await saveArtifacts(msg.scan, msg.keys) };
      case "search":
        return await ask("search", { query: msg.query, limit: 8 }, 8000);
      case "today":
        return await ask("today", {}, 15000);
      case "open":
        return await ask("open", msg.review ? { review: true } : { page: msg.page }, 10000);
      default:
        throw new Error("unknown message");
    }
  };
  run().then(
    (value) => reply({ ok: true, value }),
    (error) => reply({ ok: false, error: String((error && error.message) || error) }),
  );
  return true;
});

// For tests and the console: the same operations, callable directly.
globalThis.herbarium = { ask, saveSnapshot, scanArtifacts, saveArtifacts, refreshBadge };
