// The toolbar popup: connection status, "already saved", the AI pages of
// the current chat, save page / selection, and a quick search.
"use strict";

const ext = globalThis.browser ?? globalThis.chrome;
const $ = (id) => document.getElementById(id);

/** Ask the background script; resolves with its value or throws its error. */
async function bg(message) {
  const reply = await ext.runtime.sendMessage(message);
  if (!reply || !reply.ok) throw new Error((reply && reply.error) || "No answer");
  return reply.value;
}

function say(text, isError = false) {
  const el = $("message");
  el.textContent = text;
  el.classList.toggle("error", isError);
}

function fmtSize(bytes) {
  return bytes > 1024 * 1024 ? `${(bytes / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

let tab = null;
let scan = null;
const chosen = new Set();

function renderArtifacts() {
  const list = $("artifact-list");
  list.replaceChildren();
  const candidates = scan ? scan.candidates : [];
  $("artifacts-empty").hidden = candidates.length > 0;
  for (const c of candidates) {
    const li = document.createElement("li");
    li.classList.toggle("done", !!c.alreadyImported);
    const label = document.createElement("label");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = !!c.alreadyImported || chosen.has(c.key);
    box.disabled = !!c.alreadyImported;
    box.addEventListener("change", () => {
      if (box.checked) chosen.add(c.key);
      else chosen.delete(c.key);
      $("artifacts-save").disabled = chosen.size === 0;
    });
    const text = document.createElement("span");
    const title = document.createElement("span");
    title.className = "ellipsis";
    title.textContent = c.title;
    const sub = document.createElement("span");
    sub.className = "sub";
    sub.textContent = [c.tool, fmtSize(c.bytes || 0), c.alreadyImported ? "already saved" : null].filter(Boolean).join(" · ");
    text.append(title, sub);
    label.append(box, text);
    li.append(label);
    list.append(li);
  }
  $("artifacts-save").disabled = chosen.size === 0;
}

async function loadArtifacts() {
  $("artifacts").hidden = false;
  $("artifacts-title").textContent = "Looking for pages in this chat…";
  try {
    scan = await bg({ type: "scanArtifacts", tabId: tab.id });
    for (const c of scan.candidates) if (!c.alreadyImported) chosen.add(c.key);
    $("artifacts-title").textContent = scan.candidates.length === 1 ? "1 page in this chat" : `${scan.candidates.length} pages in this chat`;
    renderArtifacts();
  } catch (e) {
    $("artifacts-title").textContent = "Pages in this chat";
    say(e.message, true);
  }
}

async function showSaved(url) {
  try {
    const found = await bg({ type: "lookup", url });
    if (found.pages.length > 0) {
      $("saved").hidden = false;
      const btn = $("saved-open");
      btn.textContent = `“${found.pages[0].title}”`;
      btn.onclick = () => openPage(found.pages[0].id);
    }
  } catch {
    /* ignore */
  }
}

async function openPage(id) {
  try {
    await bg({ type: "open", page: id });
    window.close();
  } catch (e) {
    say(e.message, true);
  }
}

async function savePage(mode) {
  say(mode === "selection" ? "Saving the selection…" : "Saving the page…");
  try {
    const { page } = await bg({ type: "savePage", tabId: tab.id, mode });
    say(`Saved “${page.title}” to ${page.folder || "your vault"}.`);
    await showSaved(tab.url);
  } catch (e) {
    say(e.message, true);
  }
}

async function saveArtifacts() {
  if (!scan || chosen.size === 0) return;
  $("artifacts-save").disabled = true;
  say("Saving…");
  try {
    const { pages } = await bg({ type: "saveArtifacts", scan, keys: [...chosen] });
    say(pages.length === 1 ? `Saved “${pages[0].title}”.` : `Saved ${pages.length} pages.`);
    for (const c of scan.candidates) if (chosen.has(c.key)) c.alreadyImported = true;
    chosen.clear();
    renderArtifacts();
  } catch (e) {
    say(e.message, true);
    $("artifacts-save").disabled = false;
  }
}

let searchTimer = null;
function onSearch() {
  clearTimeout(searchTimer);
  const query = $("search").value.trim();
  searchTimer = setTimeout(async () => {
    const list = $("results");
    list.replaceChildren();
    if (!query) return;
    try {
      const { results } = await bg({ type: "search", query });
      for (const r of results) {
        const li = document.createElement("li");
        const btn = document.createElement("button");
        const title = document.createElement("span");
        title.className = "ellipsis";
        title.textContent = r.title;
        btn.append(title);
        if (r.folder) {
          const folder = document.createElement("span");
          folder.className = "folder";
          folder.textContent = r.folder;
          btn.append(folder);
        }
        btn.addEventListener("click", () => openPage(r.id));
        li.append(btn);
        list.append(li);
      }
      if (results.length === 0) {
        const li = document.createElement("li");
        li.className = "muted";
        li.textContent = "Nothing found";
        list.append(li);
      }
    } catch (e) {
      say(e.message, true);
    }
  }, 150);
}

async function init() {
  [tab] = await ext.tabs.query({ active: true, currentWindow: true });
  try {
    const status = await bg({ type: "status" });
    $("status").textContent = `${status.vault} · ${status.pages} pages`;
  } catch (e) {
    $("status").textContent = "Not connected";
    $("setup").hidden = false;
    $("setup-error").textContent = e.message;
    return;
  }
  $("main").hidden = false;
  $("save-page").addEventListener("click", () => savePage("page"));
  $("save-selection").addEventListener("click", () => savePage("selection"));
  $("artifacts-save").addEventListener("click", saveArtifacts);
  $("artifacts-all").addEventListener("click", () => {
    if (!scan) return;
    const fresh = scan.candidates.filter((c) => !c.alreadyImported);
    const all = fresh.every((c) => chosen.has(c.key));
    for (const c of fresh) {
      if (all) chosen.delete(c.key);
      else chosen.add(c.key);
    }
    renderArtifacts();
  });
  $("search").addEventListener("input", onSearch);
  $("open-today").addEventListener("click", () => {
    ext.tabs.create({ url: ext.runtime.getURL("today.html") });
    window.close();
  });
  $("open-review").addEventListener("click", async () => {
    try {
      await bg({ type: "open", review: true });
      window.close();
    } catch (e) {
      say(e.message, true);
    }
  });

  if (!tab || !/^https?:/i.test(tab.url || "")) {
    $("save-page").disabled = true;
    say("Open a web page to save it.");
    return;
  }
  void showSaved(tab.url);
  try {
    const info = await bg({ type: "pageInfo", tabId: tab.id });
    $("save-selection").disabled = !info.selection;
    if (info.site) await loadArtifacts();
  } catch {
    // Pages the browser protects (stores, settings) cannot be read.
    $("save-page").disabled = true;
    say("The browser does not let extensions read this page.");
  }
}

void init();
