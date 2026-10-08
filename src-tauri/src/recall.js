// Recall mode, injected after the storage shim when the reader asks for it
// (`?recall=1`, during review sessions or when the user turns on "Quiz me").
// Elements marked `data-herbarium-recall` are hidden behind a button until the
// user reveals them; a non-empty attribute value is shown as the question.
(function () {
  var ATTR = "data-herbarium-recall";
  var SHOWN = "data-herbarium-shown";
  // Elements that cannot hold the cover button get it as their next sibling.
  var VOID = { IMG: 1, INPUT: 1, BR: 1, HR: 1, EMBED: 1, AREA: 1, WBR: 1, SOURCE: 1, TRACK: 1 };

  // Hide answers before first paint; the covers arrive once the DOM is parsed.
  var style = document.createElement("style");
  style.textContent =
    "[" + ATTR + "]:not([" + SHOWN + "]){visibility:hidden!important;position:relative}" +
    ".herbarium-recall-cover{visibility:visible!important;position:absolute;inset:0;z-index:2147483647;" +
    "display:flex;align-items:center;justify-content:center;gap:6px;min-height:2.2em;margin:0;padding:6px 12px;" +
    "border:1px dashed #8a8780;border-radius:8px;background:#f4f2ec;color:#2f3437;" +
    "font:500 13px/1.3 system-ui,-apple-system,'Segoe UI',sans-serif;text-align:center;cursor:pointer}" +
    ".herbarium-recall-cover.after{position:static;display:inline-flex;margin:4px 0}" +
    ".herbarium-recall-cover:hover{background:#ebe8df}" +
    ".herbarium-recall-cover:focus-visible{outline:2px solid #2f3437;outline-offset:2px}" +
    "@media (prefers-color-scheme:dark){.herbarium-recall-cover{background:#2a2b2d;color:#e8e6e1;border-color:#6b6a68}" +
    ".herbarium-recall-cover:hover{background:#333436}}";
  (document.head || document.documentElement).appendChild(style);

  function reveal(el, cover) {
    el.setAttribute(SHOWN, "");
    if (cover.parentNode) cover.parentNode.removeChild(cover);
  }

  function cover(el) {
    if (el.hasAttribute(SHOWN) || el.__herbariumCover) return;
    var question = (el.getAttribute(ATTR) || "").trim();
    var btn = document.createElement("button");
    btn.type = "button";
    btn.className = "herbarium-recall-cover";
    // textContent, never HTML: the question is page content, shown as text.
    btn.textContent = question ? question + " — show answer" : "Show answer";
    btn.addEventListener("click", function (e) {
      e.preventDefault();
      e.stopPropagation();
      reveal(el, btn);
    });
    el.__herbariumCover = btn;
    if (VOID[el.tagName]) {
      btn.className += " after";
      el.insertAdjacentElement("afterend", btn);
    } else {
      el.insertBefore(btn, el.firstChild);
    }
  }

  function coverAll() {
    var els = document.querySelectorAll("[" + ATTR + "]");
    for (var i = 0; i < els.length; i++) cover(els[i]);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", coverAll);
  } else {
    coverAll();
  }
})();
