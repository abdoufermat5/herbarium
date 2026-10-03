// Scroll-entry reveal: elements fade up the first time they enter the viewport.
// One shared IntersectionObserver; elements entering together cascade 80ms apart.
// Styling lives in app.css (`.reveal` / `.revealed`). Uses an animation rather than
// a transition so components keep their own hover transitions.

const STAGGER_MS = 80;
const MAX_STAGGER = 8;

let observer: IntersectionObserver | null = null;

function getObserver(): IntersectionObserver {
  observer ??= new IntersectionObserver(
    (entries) => {
      let k = 0;
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        const el = entry.target as HTMLElement;
        el.style.animationDelay = `${Math.min(k++, MAX_STAGGER) * STAGGER_MS}ms`;
        el.classList.add("revealed");
        observer!.unobserve(el);
      }
    },
    { rootMargin: "0px 0px -24px 0px" },
  );
  return observer;
}

export function reveal(node: HTMLElement) {
  node.classList.add("reveal");
  getObserver().observe(node);
  return {
    destroy() {
      observer?.unobserve(node);
    },
  };
}
