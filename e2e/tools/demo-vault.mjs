// The vault the README demo is recorded in: a small library of pages an AI
// tool might have made, in five folders, linked to each other. Pages follow
// the reader's theme and are self-contained (no network).
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const link = (id, text) => `<a href="herbarium-app://open/${id}">${text}</a>`;

function page({ title, kicker, accent, lead, body, see = [] }) {
  const related = see.length
    ? `<footer><span>See also</span>${see.map(([id, text]) => link(id, text)).join("")}</footer>`
    : "";
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title}</title>
<style>
:root{color-scheme:light dark;--ink:#22262a;--soft:#5b6168;--line:#e7e5df;--card:#faf9f6;--bg:#fff;--accent:${accent}}
@media (prefers-color-scheme:dark){:root{--ink:#ecebe8;--soft:#a9a7a2;--line:#33332f;--card:#252523;--bg:#1b1b1a}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:17px/1.65 Georgia,"Iowan Old Style",serif}
main{max-width:760px;margin:0 auto;padding:44px 28px 64px}
.kicker{font:600 12px/1 system-ui,sans-serif;letter-spacing:.12em;text-transform:uppercase;color:var(--accent)}
h1{font-size:40px;line-height:1.1;margin:10px 0 12px;letter-spacing:-.02em}
h2{font-size:22px;margin:34px 0 10px}
.lead{font-size:19px;color:var(--soft);margin:0 0 26px}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:12px;margin:18px 0}
.card{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:14px 16px}
.card b{display:block;font:600 14px system-ui,sans-serif;color:var(--accent);margin-bottom:4px}
.card p{margin:0;font-size:15px;color:var(--soft)}
code,.mono{font:14px ui-monospace,Menlo,monospace;background:var(--card);border:1px solid var(--line);border-radius:6px;padding:1px 6px}
pre{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:14px 16px;overflow:auto;font:14px/1.5 ui-monospace,Menlo,monospace}
[data-herbarium-recall]{border-left:3px solid var(--accent);padding:6px 0 6px 14px;margin:10px 0}
table{width:100%;border-collapse:collapse;font:15px system-ui,sans-serif}td,th{padding:8px 6px;border-bottom:1px solid var(--line);text-align:left}
th{font-weight:600;color:var(--soft);font-size:12px;letter-spacing:.06em;text-transform:uppercase}
button{font:600 14px system-ui,sans-serif;background:var(--accent);color:#fff;border:0;border-radius:999px;padding:8px 16px;cursor:pointer}
input[type=range]{width:100%;accent-color:var(--accent)}
.bar{height:10px;border-radius:999px;background:var(--accent);transition:width .2s}
footer{margin-top:44px;padding-top:16px;border-top:1px solid var(--line);display:flex;gap:10px;flex-wrap:wrap;align-items:center;font:14px system-ui,sans-serif}
footer span{color:var(--soft);margin-right:4px}footer a{color:var(--accent);text-decoration:none;border:1px solid var(--line);border-radius:999px;padding:4px 12px}
a{color:var(--accent)}
</style></head><body><main>
<div class="kicker">${kicker}</div><h1>${title}</h1><p class="lead">${lead}</p>
${body}
${related}
</main></body></html>`;
}

const cs = "#3b82c4";
const rust = "#c4622d";
const bio = "#2f8f5b";
const hist = "#8a5cc2";
const design = "#c2457a";

export const PAGES = [
  {
    folder: "computer-science",
    id: "big-o",
    tags: ["algorithms"],
    title: "Big-O at a Glance",
    kicker: "Algorithms · Explainer",
    accent: cs,
    lead: "Drag the input size and watch how the number of steps grows for each complexity class.",
    body: `<p><b class="mono">n = <span id="n">16</span></b></p><input id="size" type="range" min="1" max="64" value="16">
<table><thead><tr><th>Complexity</th><th style="width:55%">Growth</th><th>Steps</th></tr></thead><tbody id="rows"></tbody></table>
<script>
const fns=[["O(1)",()=>1],["O(log n)",n=>Math.ceil(Math.log2(n+1))],["O(n)",n=>n],["O(n log n)",n=>Math.ceil(n*Math.log2(n+1))],["O(n²)",n=>n*n]];
const draw=()=>{const n=+size.value;document.getElementById("n").textContent=n;const max=64*64;
rows.innerHTML=fns.map(([k,f])=>{const v=f(n);return '<tr><td class="mono">'+k+'</td><td><div class="bar" style="width:'+Math.max(2,Math.sqrt(v/max)*100)+'%"></div></td><td>'+v+'</td></tr>'}).join("")};
size.oninput=draw;draw();
</script>
<h2>Why it matters</h2><p>Constant factors fade as inputs grow; the shape of the curve is what decides whether a program still answers in time.</p>`,
    see: [["binary-search", "Binary search"], ["http-caching", "HTTP caching"]],
  },
  {
    folder: "computer-science",
    id: "binary-search",
    tags: ["algorithms"],
    title: "Binary Search, Step by Step",
    kicker: "Algorithms · Walkthrough",
    accent: cs,
    lead: "Halve the search space at every step: a sorted list of a million items takes at most twenty looks.",
    body: `<div class="grid"><div class="card"><b>1 · Look in the middle</b><p>Compare the target with the middle item.</p></div>
<div class="card"><b>2 · Drop a half</b><p>Too small? Keep the right half. Too big? Keep the left.</p></div>
<div class="card"><b>3 · Repeat</b><p>Stop when you find it, or when nothing is left.</p></div></div>
<pre>fn search(xs: &amp;[i32], t: i32) -&gt; Option&lt;usize&gt; {
    let (mut lo, mut hi) = (0, xs.len());
    while lo &lt; hi {
        let mid = lo + (hi - lo) / 2;
        match xs[mid].cmp(&amp;t) {
            Less =&gt; lo = mid + 1,
            Greater =&gt; hi = mid,
            Equal =&gt; return Some(mid),
        }
    }
    None
}</pre>`,
    see: [["big-o", "Big-O"], ["ownership", "Rust ownership"]],
  },
  {
    folder: "computer-science",
    id: "tcp-handshake",
    tags: ["networking"],
    title: "The TCP Three-Way Handshake",
    kicker: "Networking · Protocols",
    accent: cs,
    lead: "Before a single byte of data moves, both sides agree on sequence numbers in three steps.",
    body: `<div class="grid"><div class="card"><b>SYN</b><p>The client picks a random initial sequence number.</p></div>
<div class="card"><b>SYN-ACK</b><p>The server acknowledges it and sends its own.</p></div>
<div class="card"><b>ACK</b><p>Both sides are now ESTABLISHED.</p></div></div>
<p data-herbarium-recall="Why are initial sequence numbers random?">So that segments from an old connection, or from an attacker guessing numbers, are not mistaken for this one.</p>`,
    see: [["dns", "DNS"], ["http-caching", "HTTP caching"]],
  },
  {
    folder: "computer-science",
    id: "dns",
    tags: ["networking"],
    title: "How DNS Resolves a Name",
    kicker: "Networking · Explainer",
    accent: cs,
    lead: "From herbarium.app to an IP address: a recursive resolver walks down from the root, caching as it goes.",
    body: `<table><thead><tr><th>Step</th><th>Asks</th><th>Learns</th></tr></thead><tbody>
<tr><td>1</td><td>Root server</td><td>Who serves <span class="mono">.app</span></td></tr>
<tr><td>2</td><td>TLD server</td><td>Who serves <span class="mono">herbarium.app</span></td></tr>
<tr><td>3</td><td>Authoritative server</td><td>The A / AAAA record</td></tr></tbody></table>
<p data-herbarium-recall="What makes the second lookup fast?">Every answer carries a TTL; resolvers cache it until it expires.</p>`,
    see: [["tcp-handshake", "TCP handshake"]],
  },
  {
    folder: "computer-science",
    id: "http-caching",
    tags: ["web"],
    title: "HTTP Caching Cheat Sheet",
    kicker: "Web · Cheat sheet",
    accent: cs,
    lead: "The headers that decide whether a browser asks again, revalidates, or uses what it has.",
    body: `<div class="grid"><div class="card"><b>Cache-Control: max-age</b><p>Fresh for N seconds, no request at all.</p></div>
<div class="card"><b>ETag / If-None-Match</b><p>Revalidate; a 304 sends no body.</p></div>
<div class="card"><b>no-store</b><p>Never keep a copy.</p></div><div class="card"><b>immutable</b><p>Never revalidate while fresh.</p></div></div>`,
    see: [["dns", "DNS"], ["big-o", "Big-O"]],
  },
  {
    folder: "rust",
    id: "ownership",
    tags: ["rust", "memory"],
    title: "Rust Ownership in Five Rules",
    kicker: "Rust · Core ideas",
    accent: rust,
    lead: "Every value has one owner. Everything else about borrowing follows from that.",
    body: `<h2>The rules</h2>
<div class="grid"><div class="card"><b>One owner</b><p>When the owner goes out of scope, the value is dropped.</p></div>
<div class="card"><b>Moves</b><p>Assigning or passing a value moves it; the old name can no longer be used.</p></div>
<div class="card"><b>Shared borrows</b><p>Any number of <span class="mono">&amp;T</span> at once.</p></div>
<div class="card"><b>Exclusive borrows</b><p>Exactly one <span class="mono">&amp;mut T</span>, and no shared ones.</p></div></div>
<p>Borrows can never outlive the value they point to: the compiler checks this with lifetimes, so there are no dangling pointers and no data races.</p>
<h2>Check yourself</h2>
<p data-herbarium-recall="What happens to a String after it is passed to a function by value?">It is moved: the function owns it now and drops it at the end, unless it gives it back.</p>
<p data-herbarium-recall="Why can't you hold &amp;mut T and &amp;T at the same time?">A writer next to readers could change data under them; one writer or many readers keeps every read consistent.</p>`,
    see: [["lifetimes", "Lifetimes"], ["traits", "Traits"]],
  },
  {
    folder: "rust",
    id: "lifetimes",
    tags: ["rust"],
    title: "Lifetimes Without Tears",
    kicker: "Rust · Explainer",
    accent: rust,
    lead: "A lifetime is a name for how long a borrow is valid, so the compiler can check it.",
    body: `<pre>fn longest&lt;'a&gt;(x: &amp;'a str, y: &amp;'a str) -&gt; &amp;'a str {
    if x.len() &gt; y.len() { x } else { y }
}</pre><p>The result lives as long as the shorter of the two inputs: that is all <span class="mono">'a</span> says.</p>`,
    see: [["ownership", "Ownership"]],
  },
  {
    folder: "rust",
    id: "traits",
    tags: ["rust"],
    title: "Traits vs Interfaces",
    kicker: "Rust · Comparison",
    accent: rust,
    lead: "Traits look like interfaces, but can be implemented for types you did not write.",
    body: `<table><thead><tr><th></th><th>Trait</th><th>Interface</th></tr></thead><tbody>
<tr><td>Default methods</td><td>Yes</td><td>Often</td></tr><tr><td>Implement for foreign types</td><td>Yes</td><td>No</td></tr>
<tr><td>Static dispatch</td><td>By default</td><td>Rarely</td></tr></tbody></table>`,
    see: [["ownership", "Ownership"], ["binary-search", "Binary search"]],
  },
  {
    folder: "biology",
    id: "photosynthesis",
    tags: ["biology", "science"],
    title: "Photosynthesis in One Page",
    kicker: "Biology · Explainer",
    accent: bio,
    lead: "Plants turn light into chemical energy. Chlorophyll captures sunlight and drives two linked stages of reactions.",
    body: `<p class="card" style="text-align:center;font:600 20px ui-monospace,monospace">6 CO₂ + 6 H₂O + light → C₆H₁₂O₆ + 6 O₂</p>
<div class="grid"><div class="card"><b>Light reactions</b><p>In the thylakoids, light splits water, releasing oxygen and charging ATP and NADPH.</p></div>
<div class="card"><b>Calvin cycle</b><p>In the stroma, ATP and NADPH fix carbon dioxide into sugar.</p></div></div>
<p data-herbarium-recall="Where does the oxygen plants release come from?">From water molecules split in the light reactions, not from CO₂.</p>`,
    see: [["enzymes", "Enzymes"], ["mitosis", "Mitosis"]],
  },
  {
    folder: "biology",
    id: "mitosis",
    tags: ["biology"],
    title: "Mitosis: The Four Phases",
    kicker: "Biology · Cell division",
    accent: bio,
    lead: "One cell becomes two identical ones: prophase, metaphase, anaphase, telophase.",
    body: `<div class="grid"><div class="card"><b>Prophase</b><p>Chromosomes condense; the spindle forms.</p></div>
<div class="card"><b>Metaphase</b><p>Chromosomes line up in the middle.</p></div><div class="card"><b>Anaphase</b><p>Sister chromatids are pulled apart.</p></div>
<div class="card"><b>Telophase</b><p>Two nuclei form; the cell splits.</p></div></div>`,
    see: [["photosynthesis", "Photosynthesis"]],
  },
  {
    folder: "biology",
    id: "enzymes",
    tags: ["biology", "science"],
    title: "How Enzymes Work",
    kicker: "Biology · Explainer",
    accent: bio,
    lead: "Enzymes lower the activation energy of a reaction without being used up.",
    body: `<p data-herbarium-recall="What happens to an enzyme at a very high temperature?">It denatures: its shape, and so its active site, falls apart.</p>`,
    see: [["photosynthesis", "Photosynthesis"]],
  },
  {
    folder: "history",
    id: "french-revolution",
    tags: ["history"],
    title: "The French Revolution: A Timeline",
    kicker: "History · Timeline",
    accent: hist,
    lead: "Ten years that ended a monarchy and remade Europe.",
    body: `<table><tbody><tr><td class="mono">1789</td><td>Estates-General; storming of the Bastille</td></tr>
<tr><td class="mono">1792</td><td>The Republic is proclaimed</td></tr><tr><td class="mono">1793</td><td>The Terror begins</td></tr>
<tr><td class="mono">1799</td><td>Napoleon's coup of 18 Brumaire</td></tr></tbody></table>`,
    see: [["industrial-revolution", "Industrial Revolution"]],
  },
  {
    folder: "history",
    id: "industrial-revolution",
    tags: ["history"],
    title: "The Industrial Revolution",
    kicker: "History · Explainer",
    accent: hist,
    lead: "Steam, iron and the factory moved work out of homes and into cities.",
    body: `<div class="grid"><div class="card"><b>1712</b><p>Newcomen's steam engine pumps mines.</p></div><div class="card"><b>1769</b><p>Watt's separate condenser.</p></div>
<div class="card"><b>1830</b><p>The Liverpool–Manchester railway opens.</p></div></div>`,
    see: [["french-revolution", "French Revolution"], ["big-o", "Big-O (a different revolution)"]],
  },
  {
    folder: "design",
    id: "color-contrast",
    tags: ["design", "accessibility"],
    title: "Colour Contrast, Explained",
    kicker: "Design · Accessibility",
    accent: design,
    lead: "Text needs a contrast ratio of at least 4.5:1 against its background to be readable by most people.",
    body: `<div class="grid"><div class="card"><b>4.5 : 1</b><p>Body text (WCAG AA).</p></div><div class="card"><b>3 : 1</b><p>Large text and UI components.</p></div>
<div class="card"><b>7 : 1</b><p>Body text (AAA).</p></div></div>`,
    see: [["css-grid", "CSS grid"]],
  },
  {
    folder: "design",
    id: "css-grid",
    tags: ["design", "web"],
    title: "CSS Grid Playground",
    kicker: "Design · Playground",
    accent: design,
    lead: "Change the columns and watch the cards reflow.",
    body: `<p><button id="less">− column</button> <button id="more">+ column</button></p>
<div id="g" class="grid" style="grid-template-columns:repeat(3,1fr)">${Array.from({ length: 6 }, (_, i) => `<div class="card"><b>Card ${i + 1}</b><p>Auto-placed.</p></div>`).join("")}</div>
<script>let c=3;const set=()=>g.style.gridTemplateColumns='repeat('+c+',1fr)';more.onclick=()=>{c=Math.min(6,c+1);set()};less.onclick=()=>{c=Math.max(1,c-1);set()};</script>`,
    see: [["color-contrast", "Colour contrast"], ["http-caching", "HTTP caching"]],
  },
];

/** Write the pages (HTML only; the app indexes them on open). */
export function writeDemoVault(dir) {
  for (const p of PAGES) {
    mkdirSync(join(dir, p.folder), { recursive: true });
    writeFileSync(join(dir, p.folder, `${p.id}.html`), page(p));
  }
}

/** The quiz the fake AI service "writes" when the demo remixes a page. */
export function remixedWithQuiz(p) {
  return page({
    ...p,
    body:
      p.body +
      `<h2>Quiz</h2>
<p data-herbarium-recall="Who owns a value after it is assigned to another variable?">The new variable: the value moved, and the old name can no longer be used.</p>
<p data-herbarium-recall="How many &amp;mut borrows of a value can exist at once?">Exactly one, and no shared borrows alongside it.</p>
<p data-herbarium-recall="When is a value dropped?">When its owner goes out of scope.</p>`,
  });
}
