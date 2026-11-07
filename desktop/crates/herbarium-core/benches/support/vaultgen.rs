#![allow(dead_code)] // each user (example, bench) needs a different subset
// Synthetic vault generator shared by `examples/gen_vault.rs` and
// `benches/vault.rs`. It builds the vault only through `Host::call`, so it
// keeps working when internals change. The output is deterministic for a
// given page count.
//
// Shape of the vault (roughly a heavy real library):
// - HTML between 5 and 200 KB (skewed towards the small end), a title, headings,
//   recall markers, and links to earlier pages through `herbarium-app://open/<id>`;
// - tags, a folder tree up to three levels deep, a source URL on most pages;
// - a review history of 0-30 entries on about two thirds of the pages, and
//   about 8% of the pages left due for review;
// - about 5% of the pages with 300-800 KB of page state (`storage.write`) and
//   highlights on about 10%.

use std::path::Path;

use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

/// Words used for page text. The first few are deliberately common: the
/// "common term" search benchmark looks for `tokio`.
const WORDS: &[&str] = &[
    "tokio",
    "cargo",
    "rust",
    "async",
    "vector",
    "memory",
    "borrow",
    "lifetime",
    "trait",
    "macro",
    "thread",
    "mutex",
    "channel",
    "future",
    "stream",
    "buffer",
    "parser",
    "index",
    "query",
    "schema",
    "table",
    "column",
    "cursor",
    "socket",
    "packet",
    "kernel",
    "driver",
    "cache",
    "latency",
    "throughput",
    "queue",
    "stack",
    "graph",
    "node",
    "edge",
    "weight",
    "gradient",
    "tensor",
    "matrix",
    "vector",
    "angle",
    "prime",
    "modulo",
    "hash",
    "digest",
    "cipher",
    "token",
    "lexer",
    "grammar",
    "compile",
    "linker",
    "binary",
    "symbol",
    "module",
    "crate",
    "package",
    "release",
    "version",
    "semver",
    "branch",
    "commit",
    "merge",
    "rebase",
    "review",
    "notes",
];

const COLORS: [&str; 4] = ["yellow", "green", "blue", "pink"];

const TOPICS: &[&str] = &[
    "rust", "systems", "web", "data", "ml", "security", "tools", "math", "design", "ops",
];

/// Small deterministic PRNG (xorshift64*), so the vault is reproducible
/// without a `rand` dependency.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub fn word(rng: &mut Rng) -> &'static str {
    WORDS[rng.below(WORDS.len())]
}

fn sentence(rng: &mut Rng, out: &mut String) {
    let n = 6 + rng.below(12);
    for i in 0..n {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(word(rng));
    }
    out.push_str(". ");
}

/// Folder paths: `topic`, `topic/sub-k`, `topic/sub-k/leaf-j`; depth <= 3.
pub fn folders() -> Vec<String> {
    let mut out = Vec::new();
    for (t, topic) in TOPICS.iter().enumerate() {
        out.push(format!("{topic}-{t}"));
        for s in 0..4 {
            out.push(format!("{topic}-{t}/sub-{s}"));
            for l in 0..3 {
                out.push(format!("{topic}-{t}/sub-{s}/leaf-{l}"));
            }
        }
    }
    out
}

/// A complete page of about `target` bytes.
fn html(rng: &mut Rng, index: usize, target: usize, links: &[String]) -> String {
    let mut body = String::with_capacity(target + 512);
    let mut section = 0;
    while body.len() < target {
        section += 1;
        body.push_str(&format!("<h2>Section {section} of note {index}</h2>\n<p>"));
        for _ in 0..(4 + rng.below(6)) {
            sentence(rng, &mut body);
        }
        body.push_str("</p>\n");
        if rng.below(5) == 0 {
            body.push_str("<p data-herbarium-recall=\"What did section ");
            body.push_str(&section.to_string());
            body.push_str(" say?\">");
            sentence(rng, &mut body);
            body.push_str("</p>\n");
        }
        if rng.below(4) == 0 && !links.is_empty() {
            let id = &links[rng.below(links.len())];
            body.push_str(&format!(
                "<p>See also <a href=\"herbarium-app://open/{id}\">related note</a>.</p>\n"
            ));
        }
        if rng.below(12) == 0 {
            body.push_str("<pre><code>fn main() { println!(\"hello\"); }</code></pre>\n");
        }
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Note {index}: {} {}</title>\
         <style>body{{font:16px system-ui;max-width:42rem;margin:2rem auto}}</style></head>\
         <body><h1>Note {index}</h1>\n{body}</body></html>",
        word(rng),
        word(rng)
    )
}

fn call(host: &Host, op: &str, args: Value) -> Value {
    let t = std::time::Instant::now();
    let out = host
        .call(Caller::Ui, op, args)
        .unwrap_or_else(|e| panic!("{op}: {e}"));
    if std::env::var_os("HERB_GEN_TIMING").is_some() {
        eprintln!("gen-timing {op} {:.1}ms", t.elapsed().as_secs_f64() * 1e3);
    }
    out
}

/// A sidecar to patch once its page exists: id, folder, `ext` entries and
/// `(lastReview, intervalMinutes, nextReview)`.
type Patch = (
    String,
    Option<String>,
    serde_json::Map<String, Value>,
    Option<(i64, i64, i64)>,
);

/// Whether `dir` already holds a finished vault of `n` pages.
pub fn is_generated(dir: &Path, n: usize) -> bool {
    std::fs::read_to_string(dir.join(".herbarium-bench-vault"))
        .map(|s| s.trim() == n.to_string())
        .unwrap_or(false)
}

/// Fill the (empty or missing) vault at `dir` with `n` pages.
pub fn generate(dir: &Path, n: usize) {
    std::fs::create_dir_all(dir).unwrap();
    let mut host = Host::new();
    host.open_vault(dir.to_str().unwrap()).unwrap();
    let mut rng = Rng::new(0x9E37_79B9 ^ n as u64);
    let folders = folders();
    let mut ids: Vec<String> = Vec::with_capacity(n);
    let mut patches: Vec<Patch> = Vec::new();
    let started = std::time::Instant::now();

    for i in 0..n {
        // 5 KB .. 200 KB, skewed small: mean about 55 KB.
        let size = 5_000 + (rng.unit().powi(3) * 195_000.0) as usize;
        let links: Vec<String> = (0..3)
            .filter(|_| !ids.is_empty())
            .map(|_| ids[rng.below(ids.len())].clone())
            .collect();
        let folder = (rng.below(10) != 0).then(|| folders[rng.below(folders.len())].clone());
        let tags: Vec<String> = (0..rng.below(4))
            .map(|_| format!("tag-{}", rng.below(60)))
            .collect();
        let mut args = json!({
            "html": html(&mut rng, i, size, &links),
            "folder": folder,
            "tags": tags,
            "note": if rng.below(5) == 0 { format!("note about {}", word(&mut rng)) } else { String::new() },
        });
        if rng.below(10) < 7 {
            args["source"] = json!({
                "url": format!("https://example.com/articles/{i}"),
                "tool": "Claude Code",
                "prompt": format!("explain {} and {}", word(&mut rng), word(&mut rng)),
            });
        }
        let id = call(&host, "pages.create", args)["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Review history, page state and highlights are written into the
        // sidecars in one pass below: doing them through operations costs
        // one fsync each and would make a 10k vault take an hour.
        let mut ext = serde_json::Map::new();
        let now = herbarium_core::time::now_ms();
        let day = 86_400_000i64;
        let mut next_review = None;
        // 0-30 completed reviews on about two thirds of the pages.
        if rng.below(3) != 0 {
            let count = rng.below(31);
            if count > 0 {
                let mut log = Vec::new();
                let mut days = serde_json::Map::new();
                let mut interval = 1440i64;
                for k in 0..count {
                    let at = now - ((count - k) as i64) * 3 * day;
                    let grade = ["again", "hard", "good", "good", "easy"][rng.below(5)];
                    interval = if grade == "again" { 1440 } else { interval * 2 };
                    log.push(json!({ "at": at, "grade": grade, "intervalMinutes": interval }));
                    let midnight = at - at.rem_euclid(day);
                    let entry = days.entry(midnight.to_string()).or_insert(json!(0));
                    *entry = json!(entry.as_i64().unwrap() + 1);
                }
                let last = now - 3 * day;
                next_review = Some((last, interval, last + interval * 60_000));
                ext.insert(
                    "review".into(),
                    json!({
                        "count": count, "log": log, "days": days,
                        "stability": 1.0 + interval as f64 / 1440.0, "difficulty": 5.0
                    }),
                );
            }
        }
        // About 8% of the pages are overdue.
        if rng.below(100) < 8 {
            let last = now - 10 * day;
            next_review = Some((last, 1440, now - 3_600_000));
        }
        // Legacy layout: 300-800 KB of page state in the sidecar, on 5% of pages.
        if rng.below(100) < 5 {
            let target = 300_000 + rng.below(500_000);
            let mut local = serde_json::Map::new();
            let mut total = 0;
            while total < target {
                let value: String = (0..400)
                    .map(|_| word(&mut rng))
                    .collect::<Vec<_>>()
                    .join(" ");
                total += value.len() + 16;
                local.insert(format!("state-{}", local.len()), json!(value));
            }
            ext.insert(
                "storage".into(),
                json!({ "local": local, "personal": {}, "shared": {} }),
            );
        }
        // Highlights on about 10% of pages.
        if rng.below(10) == 0 {
            let list: Vec<Value> = (0..(1 + rng.below(4)))
                .map(|k| {
                    json!({
                        "id": format!("h{k}"),
                        "quote": format!("{} {} {}", word(&mut rng), word(&mut rng), word(&mut rng)),
                        "prefix": "", "suffix": "",
                        "color": COLORS[rng.below(4)],
                        "note": "worth remembering",
                        "at": now - (k as i64) * day,
                    })
                })
                .collect();
            ext.insert("highlights".into(), json!(list));
        }
        if !ext.is_empty() || next_review.is_some() {
            patches.push((id.clone(), folder.clone(), ext, next_review));
        }
        ids.push(id);
        if (i + 1) % 500 == 0 {
            eprintln!(
                "gen_vault: {}/{n} pages ({:.0}s)",
                i + 1,
                started.elapsed().as_secs_f64()
            );
        }
    }

    // Patch the sidecars (mtime pushed forward so the rescan notices), then
    // index them all at once.
    for (id, folder, ext, review) in &patches {
        let mut sidecar = dir.to_path_buf();
        if let Some(f) = folder {
            sidecar.push(f);
        }
        sidecar.push(format!("{id}.json"));
        let mut value: Value =
            serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
        for (k, v) in ext {
            value["ext"][k] = v.clone();
        }
        if let Some((last, interval, next)) = review {
            value["lastReview"] = json!(last);
            value["intervalMinutes"] = json!(interval);
            value["nextReview"] = json!(next);
        }
        std::fs::write(&sidecar, serde_json::to_string(&value).unwrap()).unwrap();
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&sidecar)
            .unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10))
            .unwrap();
    }
    call(&host, "vault.rescan", json!({}));
    drop(host);
    std::fs::write(dir.join(".herbarium-bench-vault"), n.to_string()).unwrap();
    eprintln!(
        "gen_vault: {n} pages in {} ({:.0}s)",
        dir.display(),
        started.elapsed().as_secs_f64()
    );
}
