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
