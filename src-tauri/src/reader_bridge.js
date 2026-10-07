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
