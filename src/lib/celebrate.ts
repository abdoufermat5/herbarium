// A short burst of petals for earned moments (a finished review session, a
// streak milestone). Plain DOM and the Web Animations API: no dependencies,
// transform and opacity only, gone after ~1.2s, never blocks input.

export interface CelebrateOrigin {
  /** Viewport coordinates in CSS pixels. */
  x: number;
  y: number;
}

const TONES = ["sage", "rose", "amber", "sky"] as const;
const PETALS = 12;
const DURATION_MS = 1100;
const SVG_NS = "http://www.w3.org/2000/svg";
// A teardrop pointing up, drawn in a 14 x 20 box.
const PETAL_PATH = "M7 1 C11.5 6 12.5 12 7 19 C1.5 12 2.5 6 7 1 Z";

function petal(tone: (typeof TONES)[number]): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 14 20");
  svg.setAttribute("width", "14");
  svg.setAttribute("height", "20");
  svg.setAttribute("aria-hidden", "true");
  svg.style.cssText = "position:absolute;left:-7px;top:-10px;overflow:visible;will-change:transform,opacity;";
  const path = document.createElementNS(SVG_NS, "path");
  path.setAttribute("d", PETAL_PATH);
  path.setAttribute("fill", `var(--c-${tone}-soft)`);
  path.setAttribute("stroke", `var(--c-${tone})`);
  path.setAttribute("stroke-width", "1.2");
  path.setAttribute("stroke-linejoin", "round");
  svg.append(path);
  return svg;
}

/**
 * Scatter petals outward from `origin` (default: just above the middle of the
 * window). Does nothing under reduced motion. Resolves once the petals are gone.
 */
export function celebrate(origin?: CelebrateOrigin): Promise<void> {
  if (typeof document === "undefined" || matchMedia("(prefers-reduced-motion: reduce)").matches) {
    return Promise.resolve();
  }

  const x = origin?.x ?? window.innerWidth / 2;
  const y = origin?.y ?? window.innerHeight * 0.4;

  const layer = document.createElement("div");
  layer.setAttribute("aria-hidden", "true");
  layer.style.cssText =
    "position:fixed;inset:0;overflow:hidden;pointer-events:none;z-index:var(--z-overlay, 70);";

  const animations: Animation[] = [];
  for (let i = 0; i < PETALS; i++) {
    const holder = document.createElement("div");
    holder.style.cssText = `position:absolute;left:${x}px;top:${y}px;width:0;height:0;pointer-events:none;`;
    holder.append(petal(TONES[i % TONES.length]));
    layer.append(holder);

    // Evenly fanned with a little jitter, so the burst reads as a bloom.
    const angle = (i / PETALS) * Math.PI * 2 + (Math.random() - 0.5) * 0.5;
    const distance = 70 + Math.random() * 90;
    const dx = Math.cos(angle) * distance;
    // Petals drift down a touch as they fade, like falling.
    const dy = Math.sin(angle) * distance + 26;
    const spin = (Math.random() - 0.5) * 220;
    const heading = (angle * 180) / Math.PI + 90;
    const start = `translate(0px, 0px) rotate(${heading}deg) scale(0.3)`;
    const mid = `translate(${dx * 0.7}px, ${dy * 0.6}px) rotate(${heading + spin * 0.6}deg) scale(1)`;
    const end = `translate(${dx}px, ${dy}px) rotate(${heading + spin}deg) scale(0.85)`;
    animations.push(
      holder.animate(
        [
          { transform: start, opacity: 0 },
          { transform: mid, opacity: 1, offset: 0.35 },
          { transform: end, opacity: 0 },
        ],
        { duration: DURATION_MS - Math.random() * 200, easing: "cubic-bezier(0.16, 1, 0.3, 1)", fill: "forwards" },
      ),
    );
  }
  document.body.append(layer);

  return Promise.allSettled(animations.map((a) => a.finished)).then(() => layer.remove());
}
