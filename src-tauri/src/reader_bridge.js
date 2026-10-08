// Reader bridge, injected into every served page after the storage shim. The
// page runs in a sandboxed frame the reader cannot reach into, so the two talk
// by message: the page reports its headings (for the table of contents) and
// follows the reader's "go to heading" and zoom requests. Only messages from
// the reader (the parent window) are obeyed.
(function () {
  var MAX_HEADINGS = 200;
  var MAX_TEXT = 120;
  var SELECTOR = "h1, h2, h3";

  function headings() {
    var out = [];
    var els = document.querySelectorAll(SELECTOR);
    for (var i = 0; i < els.length && out.length < MAX_HEADINGS; i++) {
      var el = els[i];
      // Recall covers live inside answers; a heading's own text is enough.
      var text = (el.textContent || "").replace(/\s+/g, " ").trim().slice(0, MAX_TEXT);
      if (text) out.push({ level: Number(el.tagName.charAt(1)), text: text, index: i });
    }
    return out;
  }

  /** Alpha of a computed colour (0 for transparent). */
  function alpha(c) {
    var m = /rgba\([^)]*,\s*([\d.]+)\)/.exec(c);
    return m ? parseFloat(m[1]) : c && c.indexOf("rgb") === 0 ? 1 : 0;
  }

  /**
   * A layout digest of the first screen, for the library's miniature: boxes
   * with a background, text runs, media, and one small image when its
   * pixels can be read.
   */
  function digest() {
    var W = document.documentElement.clientWidth || innerWidth || 1024;
    var H = Math.min(innerHeight || 768, 900);
    var root = document.body || document.documentElement;
    var bg = getComputedStyle(root).backgroundColor;
    if (!alpha(bg)) bg = getComputedStyle(document.documentElement).backgroundColor;
    if (!alpha(bg)) bg = "rgb(255, 255, 255)";
    var boxes = [];
    var texts = [];
    var media = [];
    var image = null;
    var els = root.querySelectorAll("*");
    for (var i = 0; i < els.length && i < 4000; i++) {
      var el = els[i];
      if (el.closest && el.closest(".herbarium-recall-cover")) continue;
      var r = el.getBoundingClientRect();
      if (r.width < 3 || r.height < 3 || r.bottom < 0 || r.top > H || r.right < 0 || r.left > W) continue;
      var cs = getComputedStyle(el);
      if (cs.visibility === "hidden" || parseFloat(cs.opacity) === 0) continue;
      var rect = { x: r.left, y: r.top, w: r.width, h: r.height };
      var tag = el.tagName;
      if (tag === "IMG" || tag === "SVG" || tag === "svg" || tag === "CANVAS" || tag === "VIDEO") {
        media.push({ k: "img", x: rect.x, y: rect.y, w: rect.w, h: rect.h, r: parseFloat(cs.borderTopLeftRadius) || 0 });
        if (!image && rect.w >= 48 && rect.h >= 48) {
          try {
            var canvas = document.createElement("canvas");
            var scale = Math.min(1, 240 / rect.w);
            canvas.width = Math.max(1, Math.round(rect.w * scale));
            canvas.height = Math.max(1, Math.round(rect.h * scale));
            var ctx = canvas.getContext("2d");
            if (tag === "CANVAS") ctx.drawImage(el, 0, 0, canvas.width, canvas.height);
            else if (tag === "IMG" && el.complete && el.naturalWidth) ctx.drawImage(el, 0, 0, canvas.width, canvas.height);
            else canvas = null;
            if (canvas) image = { x: rect.x, y: rect.y, w: rect.w, h: rect.h, src: canvas.toDataURL("image/jpeg", 0.7) };
          } catch (e) {
            image = null; // a cross-origin image taints the canvas
          }
        }
        continue;
      }
      if (alpha(cs.backgroundColor) > 0.05 && rect.w * rect.h > 400) {
        boxes.push({ k: "box", x: rect.x, y: rect.y, w: rect.w, h: rect.h, c: cs.backgroundColor, r: parseFloat(cs.borderTopLeftRadius) || 0 });
      }
      var hasText = false;
      for (var n = el.firstChild; n; n = n.nextSibling) {
        if (n.nodeType === 3 && /\S/.test(n.nodeValue)) {
          hasText = true;
          break;
        }
      }
      if (hasText) {
        var range = document.createRange();
        range.selectNodeContents(el);
        var tr = range.getBoundingClientRect();
        if (tr.width >= 2 && tr.height >= 2) {
          texts.push({
            k: "text",
            x: tr.left,
            y: tr.top,
            w: tr.width,
            h: tr.height,
            c: cs.color,
            s: parseFloat(cs.fontSize) || 16,
            lh: parseFloat(cs.lineHeight) || (parseFloat(cs.fontSize) || 16) * 1.4,
          });
        }
      }
    }
    boxes.sort(function (a, b) { return b.w * b.h - a.w * a.h; });
    var blocks = boxes.slice(0, 40).concat(media.slice(0, 20), texts.slice(0, 60));
    return { w: W, h: H, bg: bg, blocks: blocks, image: image && image.src.length < 80000 ? image : null };
  }

  /* ---------------------------------------------------------- highlights */

  var HL = "herbarium-hl";
  var SKIP = { SCRIPT: 1, STYLE: 1, NOSCRIPT: 1, TEMPLATE: 1, TEXTAREA: 1 };

  /** Every visible text node of the page, with its offset in the joined text. */
  function textIndex() {
    var nodes = [];
    var text = "";
    var root = document.body || document.documentElement;
    var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode: function (n) {
        for (var p = n.parentNode; p && p !== root; p = p.parentNode) {
          if (SKIP[p.nodeName] || (p.classList && p.classList.contains("herbarium-recall-cover"))) {
            return NodeFilter.FILTER_REJECT;
          }
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    for (var n = walker.nextNode(); n; n = walker.nextNode()) {
      nodes.push({ node: n, start: text.length });
      text += n.nodeValue;
    }
    return { nodes: nodes, text: text };
  }

  /** The joined-text offset of a range boundary. */
  function offsetOf(index, container, offset) {
    for (var i = 0; i < index.nodes.length; i++) {
      var entry = index.nodes[i];
      if (entry.node === container) return entry.start + offset;
    }
    // An element boundary: the first text node at or after it.
    var probe = document.createRange();
    probe.setStart(container, offset);
    for (var j = 0; j < index.nodes.length; j++) {
      if (probe.comparePoint(index.nodes[j].node, 0) >= 0) return index.nodes[j].start;
    }
    return index.text.length;
  }

  var selectionTimer = null;
  function reportSelection() {
    clearTimeout(selectionTimer);
    selectionTimer = setTimeout(function () {
      var sel = getSelection();
      var msg = { type: "herbarium:selection", text: "" };
      if (sel && sel.rangeCount && !sel.isCollapsed) {
        var range = sel.getRangeAt(0);
        var index = textIndex();
        var start = offsetOf(index, range.startContainer, range.startOffset);
        var end = offsetOf(index, range.endContainer, range.endOffset);
        var quote = index.text.slice(start, end);
        var trimmedStart = start + (quote.length - quote.replace(/^\s+/, "").length);
        var trimmedEnd = end - (quote.length - quote.replace(/\s+$/, "").length);
        if (trimmedEnd > trimmedStart && trimmedEnd - trimmedStart <= 2000) {
          var r = range.getBoundingClientRect();
          msg = {
            type: "herbarium:selection",
            text: index.text.slice(trimmedStart, trimmedEnd),
            prefix: index.text.slice(Math.max(0, trimmedStart - 32), trimmedStart),
            suffix: index.text.slice(trimmedEnd, trimmedEnd + 32),
            rect: { x: r.left, y: r.top, w: r.width, h: r.height },
          };
        }
      }
      try {
        window.parent.postMessage(msg, "*");
      } catch (e) {
        /* the reader is gone */
      }
    }, 60);
  }
  document.addEventListener("mouseup", reportSelection);
  document.addEventListener("keyup", reportSelection);
  document.addEventListener("touchend", reportSelection);

  function unwrapHighlights() {
    var marks = document.querySelectorAll("mark." + HL);
    for (var i = 0; i < marks.length; i++) {
      var m = marks[i];
      var parent = m.parentNode;
      while (m.firstChild) parent.insertBefore(m.firstChild, m);
      parent.removeChild(m);
      parent.normalize();
    }
  }

  /** Characters `a` and `b` share at their ends (`fromEnd`) or starts. */
  function shared(a, b, fromEnd) {
    var n = 0;
    while (n < a.length && n < b.length) {
      var ca = fromEnd ? a.charAt(a.length - 1 - n) : a.charAt(n);
      var cb = fromEnd ? b.charAt(b.length - 1 - n) : b.charAt(n);
      if (ca !== cb) break;
      n++;
    }
    return n;
  }

  /** Where `h.quote` best fits, using its prefix and suffix to choose. */
  function locate(text, h) {
    var best = -1;
    var bestScore = -1;
    for (var at = text.indexOf(h.quote); at >= 0; at = text.indexOf(h.quote, at + 1)) {
      var score =
        shared(text.slice(Math.max(0, at - 64), at), h.prefix || "", true) +
        shared(text.slice(at + h.quote.length, at + h.quote.length + 64), h.suffix || "", false);
      if (score > bestScore) {
        best = at;
        bestScore = score;
      }
    }
    return best;
  }

  function wrap(index, start, end, h) {
    var parts = [];
    for (var i = 0; i < index.nodes.length; i++) {
      var e = index.nodes[i];
      var nStart = e.start;
      var nEnd = e.start + e.node.nodeValue.length;
      if (nEnd <= start || nStart >= end) continue;
      parts.push({ node: e.node, from: Math.max(start, nStart) - nStart, to: Math.min(end, nEnd) - nStart });
    }
    for (var j = 0; j < parts.length; j++) {
      var p = parts[j];
      var node = p.node;
      if (p.to < node.nodeValue.length) node.splitText(p.to);
      if (p.from > 0) node = node.splitText(p.from);
      var mark = document.createElement("mark");
      mark.className = HL;
      mark.setAttribute("data-hl", h.id);
      mark.setAttribute("data-color", h.color || "yellow");
      node.parentNode.insertBefore(mark, node);
      mark.appendChild(node);
    }
  }

  function applyHighlights(items) {
    if (!document.getElementById("herbarium-hl-style")) {
      var style = document.createElement("style");
      style.id = "herbarium-hl-style";
      style.textContent =
        "mark." + HL + "{color:inherit;border-radius:2px;cursor:pointer;padding:0;box-decoration-break:clone;-webkit-box-decoration-break:clone}" +
        "mark." + HL + "[data-color=yellow]{background:rgba(250,204,21,.42)}" +
        "mark." + HL + "[data-color=green]{background:rgba(74,222,128,.38)}" +
        "mark." + HL + "[data-color=blue]{background:rgba(96,165,250,.38)}" +
        "mark." + HL + "[data-color=pink]{background:rgba(244,114,182,.38)}" +
        "mark." + HL + ".flash{outline:2px solid currentColor}";
      (document.head || document.documentElement).appendChild(style);
    }
    unwrapHighlights();
    var found = [];
    for (var i = 0; i < items.length; i++) {
      var h = items[i];
      if (!h || typeof h.quote !== "string" || !h.quote) continue;
      var index = textIndex();
      var at = locate(index.text, h);
      if (at < 0) continue;
      wrap(index, at, at + h.quote.length, h);
      found.push(h.id);
    }
    window.parent.postMessage({ type: "herbarium:highlights-applied", found: found }, "*");
  }

  document.addEventListener("click", function (e) {
    var mark = e.target && e.target.closest ? e.target.closest("mark." + HL) : null;
    if (!mark || String(getSelection() || "").length) return;
    window.parent.postMessage({ type: "herbarium:highlight-click", id: mark.getAttribute("data-hl") }, "*");
  });

  // Links to other pages and to the web. Some engines block a frame's own
  // navigation before the app can see it, so a plain click would do nothing:
  // the reader follows them instead (opening the page, or offering the web
  // link). Clicks the page handles itself are left alone.
  window.addEventListener("click", function (e) {
    if (e.defaultPrevented || e.button !== 0) return;
    var a = e.target && e.target.closest ? e.target.closest("a[href]") : null;
    if (!a) return;
    var href = String(a.href || "");
    if (!/^(herbarium-app:|https?:)/i.test(href) || href.length > 4096) return;
    e.preventDefault();
    window.parent.postMessage({ type: "herbarium:link", href: href }, "*");
  });

  var lastReport = "";
  function report() {
    var items = headings();
    var key = JSON.stringify(items);
    if (key === lastReport) return;
    lastReport = key;
    try {
      window.parent.postMessage({ type: "herbarium:headings", items: items }, "*");
    } catch (e) {
      /* the reader is gone */
    }
  }

  window.addEventListener("message", function (event) {
    if (event.source !== window.parent) return;
    var data = event.data;
    if (!data || typeof data !== "object") return;
    if (data.type === "herbarium:goto" && typeof data.index === "number") {
      var el = document.querySelectorAll(SELECTOR)[data.index];
      if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
    } else if (data.type === "herbarium:zoom" && typeof data.zoom === "number") {
      var zoom = Math.min(2, Math.max(0.5, data.zoom));
      document.documentElement.style.zoom = zoom === 1 ? "" : String(zoom);
    } else if (data.type === "herbarium:highlights" && Array.isArray(data.items)) {
      applyHighlights(data.items);
    } else if (data.type === "herbarium:highlight-scroll" && typeof data.id === "string") {
      var mark = document.querySelector('mark.' + HL + '[data-hl="' + data.id.replace(/[^A-Za-z0-9_-]/g, "") + '"]');
      if (mark) {
        mark.scrollIntoView({ behavior: "smooth", block: "center" });
        mark.classList.add("flash");
        setTimeout(function () { mark.classList.remove("flash"); }, 1200);
      }
    } else if (data.type === "herbarium:digest") {
      var d = null;
      try {
        d = digest();
      } catch (e) {
        d = null;
      }
      window.parent.postMessage({ type: "herbarium:digest", digest: d }, "*");
    } else if (data.type === "herbarium:hello") {
      lastReport = "";
      report();
    }
  });

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", report);
  } else {
    report();
  }
  // Pages often build their content in script; report again once settled.
  window.addEventListener("load", function () {
    report();
    setTimeout(report, 800);
  });
})();
