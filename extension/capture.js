// Functions run inside a tab through `scripting.executeScript`. Each one is
// self-contained (no closures) and written as a `function` expression: the
// browser serialises its source text into the page, where method shorthand
// would not parse. Shared by the background script, which also inlines resources.
"use strict";

globalThis.HerbariumCapture = {
  /** What the tab is: its address, title, AI site and whether text is selected. */
  pageInfo: function pageInfo() {
    const host = location.hostname;
    const site =
      host === "claude.ai" || host.endsWith(".claude.ai")
        ? "claude"
        : host === "chatgpt.com" || host === "chat.openai.com"
          ? "chatgpt"
          : host === "gemini.google.com"
            ? "gemini"
            : null;
    const selection = String(getSelection() || "").trim().length > 0;
    return { url: location.href, title: document.title, site, selection };
  },

  /**
   * The open conversation as the sites' own data export shapes it, fetched
   * with the user's session from the page. Herbarium parses it exactly like
   * a data export, so a page saved here and later from an export is one page.
   */
  fetchConversation: async function fetchConversation(site) {
    try {
      if (site === "claude") {
        const conv = (location.pathname.match(/\/chat\/([0-9a-f-]{8,})/i) || [])[1];
        if (!conv) return { error: "no-conversation" };
        let org = (document.cookie.match(/(?:^|;\s*)lastActiveOrg=([^;]+)/) || [])[1];
        if (!org) {
          const orgs = await (await fetch("/api/organizations", { credentials: "include" })).json();
          org = Array.isArray(orgs) && orgs[0] ? orgs[0].uuid : null;
        }
        if (!org) return { error: "no-organization" };
        const res = await fetch(
          `/api/organizations/${encodeURIComponent(org)}/chat_conversations/${encodeURIComponent(conv)}?tree=True&rendering_mode=messages&render_all_tools=true`,
          { credentials: "include" },
        );
        if (!res.ok) return { error: `http-${res.status}` };
        return { json: JSON.stringify([await res.json()]) };
      }
      if (site === "chatgpt") {
        const conv = (location.pathname.match(/\/c\/([0-9a-f-]{8,})/i) || [])[1];
        if (!conv) return { error: "no-conversation" };
        const session = await (await fetch("/api/auth/session", { credentials: "include" })).json();
        const token = session && session.accessToken;
        if (!token) return { error: "no-session" };
        const res = await fetch(`/backend-api/conversation/${encodeURIComponent(conv)}`, {
          credentials: "include",
          headers: { Authorization: `Bearer ${token}` },
        });
        if (!res.ok) return { error: `http-${res.status}` };
        const data = await res.json();
        if (!data.conversation_id) data.conversation_id = conv;
        return { json: JSON.stringify([data]) };
      }
      return { error: "unsupported-site" };
    } catch (e) {
      return { error: String((e && e.message) || e) };
    }
  },

  /**
   * Whole HTML documents shown as code on the page (any chat, any site), with
   * the last thing the user asked when the site is a known AI chat.
   */
  findCodeDocuments: function findCodeDocuments() {
    const isDocument = (text) => /^\s*(<!doctype html|<html[\s>])/i.test(text) && /<\/html>\s*$/i.test(text.trim());
    const seen = new Set();
    const docs = [];
    for (const el of document.querySelectorAll("pre code, pre, code-block code, .cm-content")) {
      const text = (el.innerText || el.textContent || "").trim();
      if (text.length < 40 || seen.has(text) || !isDocument(text)) continue;
      seen.add(text);
      const title = ((text.match(/<title[^>]*>([\s\S]*?)<\/title>/i) || [])[1] || "").trim();
      docs.push({ html: text, title });
    }
    const userSelectors = [
      '[data-testid="user-message"]',
      '[data-message-author-role="user"]',
      "user-query",
      ".user-query",
    ];
    let prompt = "";
    for (const sel of userSelectors) {
      const all = document.querySelectorAll(sel);
      if (all.length) {
        prompt = (all[all.length - 1].innerText || "").trim().slice(0, 2000);
        break;
      }
    }
    return { docs, prompt };
  },

  /**
   * A static snapshot of the page (`mode: "page"`) or of the selection
   * (`mode: "selection"`): scripts, frames and event handlers removed, every
   * URL made absolute. The background inlines the listed images and
   * stylesheets so the result works without network access.
   */
  snapshot: function snapshot(mode) {
    const abs = (value) => {
      try {
        return new URL(value, document.baseURI).href;
      } catch {
        return value;
      }
    };
    const escapeHtml = (s) =>
      String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
    const largest = (srcset) => {
      let best = null;
      let bestW = -1;
      for (const part of srcset.split(",")) {
        const [url, size] = part.trim().split(/\s+/);
        const w = size ? parseFloat(size) : 1;
        if (url && w > bestW) {
          best = url;
          bestW = w;
        }
      }
      return best;
    };
    const clean = (root) => {
      root
        .querySelectorAll(
          'script, noscript, iframe, frame, frameset, object, embed, template, base, meta[http-equiv], link[rel~="preload"], link[rel~="prefetch"], link[rel~="modulepreload"], link[rel~="manifest"], link[rel~="dns-prefetch"], link[rel~="preconnect"]',
        )
        .forEach((el) => el.remove());
      root.querySelectorAll("picture source").forEach((el) => el.remove());
      for (const el of root.querySelectorAll("*")) {
        for (const attr of [...el.attributes]) {
          const name = attr.name.toLowerCase();
          if (name.startsWith("on")) el.removeAttribute(attr.name);
          else if ((name === "href" || name === "src" || name === "action" || name === "formaction") && /^\s*javascript:/i.test(attr.value)) {
            el.removeAttribute(attr.name);
          }
        }
        const lazy = el.getAttribute("data-src") || el.getAttribute("data-lazy-src");
        if (el.tagName === "IMG" && lazy && (!el.getAttribute("src") || el.getAttribute("src").startsWith("data:"))) {
          el.setAttribute("src", lazy);
        }
        if (el.hasAttribute("srcset")) {
          const best = largest(el.getAttribute("srcset"));
          if (best && el.tagName === "IMG") el.setAttribute("src", best);
          el.removeAttribute("srcset");
        }
        for (const name of ["src", "href", "poster", "action"]) {
          const v = el.getAttribute(name);
          if (v && !v.startsWith("#") && !v.startsWith("data:")) el.setAttribute(name, abs(v));
        }
      }
    };

    let html;
    if (mode === "selection") {
      const sel = getSelection();
      if (!sel || sel.rangeCount === 0 || !String(sel).trim()) return { error: "no-selection" };
      const box = document.createElement("div");
      for (let i = 0; i < sel.rangeCount; i++) box.appendChild(sel.getRangeAt(i).cloneContents());
      clean(box);
      const title = document.title || location.hostname;
      html =
        `<!doctype html>\n<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">` +
        `<title>${escapeHtml(title)}</title><style>` +
        `body{margin:0 auto;max-width:760px;padding:40px 20px;font:17px/1.65 Georgia,serif;color:#2f3437;background:#fff}` +
        `img,video{max-width:100%;height:auto}pre{overflow:auto;padding:12px;background:#f4f2ec;border-radius:6px}` +
        `footer{margin-top:40px;font:13px system-ui,sans-serif;color:#6b6a68}a{color:#346538}` +
        `@media (prefers-color-scheme:dark){body{color:#e6e5e3;background:#202020}pre{background:#2a2a29}a{color:#8cbf91}}` +
        `</style></head><body><article>${box.innerHTML}</article>` +
        `<footer>Saved from <a href="${escapeHtml(location.href)}">${escapeHtml(location.href)}</a></footer></body></html>`;
    } else {
      const clone = document.documentElement.cloneNode(true);
      // Form values and checked state as the user sees them (a clone keeps
      // only the markup's defaults). Both trees still match element for element.
      const liveFields = document.documentElement.querySelectorAll("input, textarea, select");
      const cloneFields = clone.querySelectorAll("input, textarea, select");
      for (let i = 0; i < liveFields.length && i < cloneFields.length; i++) {
        const from = liveFields[i];
        const to = cloneFields[i];
        if (from.tagName === "TEXTAREA") to.textContent = from.value;
        else if (from.tagName === "SELECT") {
          for (let j = 0; j < from.options.length && j < to.options.length; j++) {
            if (from.options[j].selected) to.options[j].setAttribute("selected", "");
            else to.options[j].removeAttribute("selected");
          }
        } else if (from.type === "checkbox" || from.type === "radio") {
          if (from.checked) to.setAttribute("checked", "");
          else to.removeAttribute("checked");
        } else if (from.type !== "password" && from.type !== "file" && from.type !== "hidden") {
          to.setAttribute("value", from.value);
        }
      }
      // Same-origin stylesheets become inline CSS now; others are fetched later.
      const links = clone.querySelectorAll('link[rel~="stylesheet"]');
      const live = [...document.styleSheets];
      for (const link of links) {
        const href = abs(link.getAttribute("href") || "");
        const sheet = live.find((s) => s.href === href);
        let rules = null;
        try {
          rules = sheet ? [...sheet.cssRules].map((r) => r.cssText).join("\n") : null;
        } catch {
          rules = null;
        }
        if (rules !== null) {
          // url(...) in a sheet is relative to the sheet, not to the page.
          rules = rules.replace(/url\(\s*(['"]?)([^'")]+)\1\s*\)/g, (m, q, u) => {
            if (/^(data:|#)/i.test(u)) return m;
            try {
              return `url("${new URL(u, href).href}")`;
            } catch {
              return m;
            }
          });
          const style = document.createElement("style");
          style.textContent = rules;
          link.replaceWith(style);
        }
      }
      clean(clone);
      const head = clone.querySelector("head");
      if (head && !head.querySelector("meta[charset]")) {
        const meta = document.createElement("meta");
        meta.setAttribute("charset", "utf-8");
        head.prepend(meta);
      }
      html = "<!doctype html>\n" + clone.outerHTML;
    }

    const holder = document.createElement("template");
    holder.innerHTML = html;
    const images = new Set();
    const stylesheets = new Set();
    const scan = (root) => {
      for (const el of root.querySelectorAll("img[src], video[poster], input[type=image][src], link[rel~='icon'][href]")) {
        const v = el.getAttribute(el.tagName === "VIDEO" ? "poster" : el.tagName === "LINK" ? "href" : "src");
        if (/^https?:/i.test(v)) images.add(v);
      }
      for (const el of root.querySelectorAll('link[rel~="stylesheet"][href]')) {
        const v = el.getAttribute("href");
        if (/^https?:/i.test(v)) stylesheets.add(v);
      }
    };
    scan(holder.content);
    return {
      html,
      title: document.title,
      url: location.href,
      images: [...images],
      stylesheets: [...stylesheets],
    };
  },
};
