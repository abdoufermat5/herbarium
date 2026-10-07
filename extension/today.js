// The Today page: what Herbarium suggests for today, opened from the popup
// or with `h today` in the address bar. Clicking a page opens it in the app.
"use strict";

const ext = globalThis.browser ?? globalThis.chrome;
const $ = (id) => document.getElementById(id);

async function bg(message) {
  const reply = await ext.runtime.sendMessage(message);
  if (!reply || !reply.ok) throw new Error((reply && reply.error) || "No answer");
  return reply.value;
}

function fmtDate(ms) {
  return new Date(ms).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

function item(page, sub) {
  const li = document.createElement("li");
  const btn = document.createElement("button");
  const title = document.createElement("span");
  title.className = "ellipsis";
  title.textContent = page.title;
  btn.append(title);
  if (sub) {
    const s = document.createElement("span");
    s.className = "sub";
    s.textContent = sub;
    btn.append(s);
  }
  btn.addEventListener("click", () => bg({ type: "open", page: page.id }).catch(showError));
  li.append(btn);
  return li;
}

function fill(id, pages, sub) {
  const list = $(id);
  list.replaceChildren(...pages.map((p) => item(p, sub(p))));
  return pages.length > 0;
}

function showError(e) {
  const el = $("error");
  el.hidden = false;
  el.textContent = `${e.message}. Is the Herbarium app installed and connected (Settings → Browser)?`;
}

async function load() {
  $("date").textContent = new Date().toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" });
  const hour = new Date().getHours();
  $("greeting").textContent = hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
  try {
    const { today } = await bg({ type: "today" });
    $("content").hidden = false;
    $("due-count").textContent = String(today.dueTotal);
    $("due-label").textContent = today.dueTotal === 1 ? "page due" : "pages due";
    $("streak").textContent = today.streak > 0 ? `${today.streak}-day review streak` : `${today.totalPages} pages in your vault`;
    $("review").disabled = today.dueTotal === 0;
    $("review").addEventListener("click", () => bg({ type: "open", review: true }).catch(showError));
    fill("due", today.due, (p) => p.folder || "");
    const cont = today.continue.map((c) => ({ ...c.page, sub: `${c.pathName} · ${c.position + 1} of ${c.total}` }));
    $("continue-card").hidden = !fill("continue", cont, (p) => p.sub);
    $("rediscover-card").hidden = !fill("rediscover", today.rediscover ? [today.rediscover] : [], (p) => `Saved ${fmtDate(p.createdAt)}`);
    $("onthisday-card").hidden = !fill("onthisday", today.onThisDay, (p) => fmtDate(p.createdAt));
    fill("recent", today.recent, (p) => fmtDate(p.createdAt));
  } catch (e) {
    showError(e);
  }
}

let timer = null;
$("search").addEventListener("input", () => {
  clearTimeout(timer);
  const query = $("search").value.trim();
  timer = setTimeout(async () => {
    const list = $("results");
    list.hidden = !query;
    if (!query) return;
    try {
      const { results } = await bg({ type: "search", query });
      list.replaceChildren(...results.map((r) => item(r, r.folder || "")));
    } catch (e) {
      showError(e);
    }
  }, 150);
});

void load();
