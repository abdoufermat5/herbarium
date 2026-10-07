// Share cards: a page as a small rich card to paste into a note, a chat or an
// email: its first screen as a picture, its title, a line of its text, and a
// link to where it is published. Plain text goes along for apps that take
// no HTML.

import type { PreviewDigest } from "./types";

export interface CardInput {
  title: string;
  /** Where the page can be read; null when it is not published. */
  url: string | null;
  /** The page's HTML, for a line of its text. */
  html: string;
  /** PNG data URL of the page's first screen, if one could be drawn. */
  image: string | null;
}

function escape(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

const ENTITIES: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " };

/** The first `max` characters of the page's readable text, cut at a word. */
export function excerpt(html: string, max = 160): string {
  const text = html
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/<(script|style|noscript|template|title|nav|header|svg)\b[\s\S]*?<\/\1\s*>/gi, " ")
    .replace(/<[^>]*>/g, " ")
    .replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e: string) => {
      if (e[0] === "#") {
        const code = e[1].toLowerCase() === "x" ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10);
        return Number.isFinite(code) && code > 0 && code < 0x110000 ? String.fromCodePoint(code) : " ";
      }
      return ENTITIES[e.toLowerCase()] ?? m;
    })
    .replace(/\s+/g, " ")
    .trim();
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  const space = cut.lastIndexOf(" ");
  return `${(space > max * 0.6 ? cut.slice(0, space) : cut).replace(/[\s,;:.]+$/, "")}…`;
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "";
  }
}

/** The card as HTML (inline styles only: mail and chat apps drop the rest) and as text. */
export function shareCard(input: CardInput): { html: string; text: string } {
  const title = input.title.trim() || "Untitled page";
  const blurb = excerpt(input.html);
  const link = input.url ? escape(input.url) : null;
  const wrap = (inner: string, style: string) =>
    link ? `<a href="${link}" style="${style}">${inner}</a>` : `<span style="${style}">${inner}</span>`;
  const picture = input.image
    ? wrap(
        `<img src="${input.image}" alt="" width="440" style="display:block;width:100%;max-width:440px;border:0">`,
        "display:block;text-decoration:none",
      )
    : "";
  const footer = [input.url ? hostOf(input.url) : null, "Herbarium"].filter(Boolean).join(" · ");
  const html =
    `<div style="max-width:440px;border:1px solid #d9d7d0;border-radius:10px;overflow:hidden;font-family:system-ui,-apple-system,'Segoe UI',sans-serif;background:#ffffff">` +
    picture +
    `<div style="padding:12px 14px">` +
    wrap(escape(title), "display:block;font-size:16px;font-weight:600;line-height:1.3;color:#1f2328;text-decoration:none") +
    (blurb ? `<div style="margin-top:4px;font-size:13px;line-height:1.45;color:#57606a">${escape(blurb)}</div>` : "") +
    `<div style="margin-top:8px;font-size:12px;color:#8b8a86">${escape(footer)}</div>` +
    `</div></div>`;
  const text = [title, blurb, input.url].filter(Boolean).join("\n");
  return { html, text };
}

/** Draw a preview digest onto a canvas, as the library's miniatures do, and
 *  return it as a PNG data URL (null if the image cannot be made). */
export async function digestPng(digest: PreviewDigest, width = 880, aspect = 0.52): Promise<string | null> {
  try {
    const scale = width / digest.w;
    const h = Math.min(digest.h, digest.w * aspect);
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(width);
    canvas.height = Math.round(h * scale);
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.scale(scale, scale);
    ctx.fillStyle = digest.bg;
    ctx.fillRect(0, 0, digest.w, h);
    const round = (x: number, y: number, w: number, hh: number, r: number) => {
      ctx.beginPath();
      if (typeof ctx.roundRect === "function") ctx.roundRect(x, y, w, hh, Math.min(r, w / 2, hh / 2));
      else ctx.rect(x, y, w, hh);
      ctx.fill();
    };
    for (const b of digest.blocks) {
      if (b.y >= h) continue;
      if (b.k === "box") {
        ctx.fillStyle = b.c ?? "transparent";
        round(b.x, b.y, b.w, b.h, b.r ?? 0);
      } else if (b.k === "img") {
        ctx.fillStyle = "rgba(128, 128, 128, 0.15)";
        round(b.x, b.y, b.w, b.h, b.r ?? 0);
      } else {
        const size = b.s ?? 14;
        const lh = Math.max(size, b.lh ?? size * 1.4);
        const count = Math.max(1, Math.min(12, Math.round(b.h / lh)));
        const bar = Math.max(2, size * 0.5);
        ctx.globalAlpha = 0.55;
        ctx.fillStyle = b.c ?? "#000";
        for (let i = 0; i < count; i++) {
          const w = count > 1 && i === count - 1 ? b.w * 0.6 : b.w;
          round(b.x, b.y + i * lh + (lh - bar) / 2, w, bar, bar / 2);
        }
        ctx.globalAlpha = 1;
      }
    }
    if (digest.image) {
      const img = new Image();
      img.src = digest.image.src;
      await img.decode();
      const { x, y, w, h: ih } = digest.image;
      // Cover the box, as the miniature's `slice` does.
      const s = Math.max(w / img.naturalWidth, ih / img.naturalHeight);
      const sw = w / s;
      const sh = ih / s;
      ctx.drawImage(img, (img.naturalWidth - sw) / 2, (img.naturalHeight - sh) / 2, sw, sh, x, y, w, ih);
    }
    return canvas.toDataURL("image/png");
  } catch (e) {
    console.error("share card image:", e);
    return null;
  }
}
