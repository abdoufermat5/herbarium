// Baseline benchmarks over a synthetic vault, through `Host::call` only.
//
//   cargo bench -p herbarium-core --bench vault -- --warm-up-time 1 --measurement-time 3
//   HERB_BENCH_N=10000 cargo bench -p herbarium-core --bench vault
//
// Environment: `HERB_BENCH_N` page count (default 1000), `HERB_BENCH_DIR`
// directory holding the generated vaults (default: the system temp dir;
// a vault is generated once and reused). Payload sizes are written to
// `<target>/herbarium-bench/payloads-<N>.json` for `scripts/perf-check.mjs`.

#[path = "support/vaultgen.rs"]
mod vaultgen;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

fn pages() -> usize {
    std::env::var("HERB_BENCH_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000)
}

fn vault_dir(n: usize) -> PathBuf {
    let base = std::env::var_os("HERB_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join(format!("herbarium-bench-vault-{n}"))
}

fn ensure_vault(n: usize) -> PathBuf {
    let dir = vault_dir(n);
    if !vaultgen::is_generated(&dir, n) {
        let _ = std::fs::remove_dir_all(&dir);
        vaultgen::generate(&dir, n);
    }
    dir
}

fn open(dir: &std::path::Path) -> Host {
    let mut host = Host::new();
    host.open_vault(dir.to_str().unwrap()).unwrap();
    host
}

fn run(host: &Host, op: &str, args: Value) -> Value {
    host.call(Caller::Ui, op, args)
        .unwrap_or_else(|e| panic!("{op}: {e}"))
}

fn remove_index(dir: &std::path::Path) {
    for name in ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"] {
        let _ = std::fs::remove_file(dir.join(".herbarium").join(name));
    }
}

fn payload_bytes(host: &Host, op: &str, args: Value) -> usize {
    serde_json::to_vec(&run(host, op, args)).unwrap().len()
}

/// Fixed CPU workload: `scripts/perf-check.mjs` divides by it to compare runs
/// made on machines of different speed.
fn calibration(c: &mut Criterion) {
    let doc: Value = json!({ "a": (0..2000).map(|i| format!("value-{i}")).collect::<Vec<_>>() });
    c.bench_function("calibration/json_roundtrip", |b| {
        b.iter(|| {
            let s = serde_json::to_string(black_box(&doc)).unwrap();
            black_box(serde_json::from_str::<Value>(&s).unwrap())
        })
    });
}

fn vault(c: &mut Criterion) {
    let n = pages();
    let dir = ensure_vault(n);
    let heavy = n >= 5000;

    // Cold open: no index, every page is read and parsed. A big vault takes
    // minutes per open, more than criterion's ten samples can afford, so it is
    // timed by hand (median of three) and reported in the payloads file.
    let mut manual = serde_json::Map::new();
    let cold = || {
        remove_index(&dir);
        let t = Instant::now();
        let host = open(&dir);
        let elapsed = t.elapsed();
        drop(host);
        elapsed
    };
    if heavy {
        let mut runs: Vec<Duration> = (0..3).map(|_| cold()).collect();
        runs.sort();
        manual.insert(
            "vault/open_cold_ms".into(),
            json!(runs[1].as_secs_f64() * 1e3),
        );
    }
    {
        let mut g = c.benchmark_group("vault");
        g.sample_size(10);
        if !heavy {
            g.bench_function("open_cold", |b| {
                b.iter_custom(|iters| (0..iters).map(|_| cold()).sum())
            });
        }
        // Leave an index behind for everything that follows.
        drop(open(&dir));
        g.bench_function("open_warm", |b| {
            b.iter(|| {
                let host = open(&dir);
                black_box(host.vault_path().is_some())
            })
        });
        g.bench_function("rescan_unchanged", |b| {
            let host = open(&dir);
            b.iter(|| black_box(run(&host, "vault.rescan", json!({}))))
        });
        g.finish();
    }

    let host = open(&dir);
    let mut g = c.benchmark_group("ops");
    if heavy {
        g.sample_size(20);
    }

    g.bench_function("pages.list_all", |b| {
        b.iter(|| black_box(serde_json::to_vec(&run(&host, "pages.list", json!({}))).unwrap()))
    });
    g.bench_function("pages.search_common", |b| {
        b.iter(|| black_box(run(&host, "pages.search", json!({ "query": "tokio" }))))
    });
    g.bench_function("pages.search_zero_hit", |b| {
        b.iter(|| {
            black_box(run(
                &host,
                "pages.search",
                json!({ "query": "zzxqnomatch" }),
            ))
        })
    });
    g.bench_function("pages.find_by_url", |b| {
        let url = format!("https://example.com/articles/{}", n / 2);
        b.iter(|| black_box(run(&host, "pages.find_by_url", json!({ "url": url }))))
    });
    g.bench_function("today.summary", |b| {
        b.iter(|| black_box(run(&host, "today.summary", json!({}))))
    });
    g.bench_function("review.stats", |b| {
        b.iter(|| black_box(run(&host, "review.stats", json!({}))))
    });

    let target = run(&host, "pages.list", json!({ "limit": 1 }))[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    g.bench_function("review.complete", |b| {
        b.iter(|| {
            black_box(run(
                &host,
                "review.complete",
                json!({ "id": target, "grade": "good" }),
            ))
        })
    });
    g.bench_function("storage.write_100kb", |b| {
        let value = "x".repeat(100_000);
        b.iter(|| {
            black_box(run(
                &host,
                "storage.write",
                json!({ "id": target, "changes": [{ "area": "local", "key": "bench", "value": value }] }),
            ))
        })
    });
    // Only the create is timed; the page is trashed and purged afterwards so
    // the vault does not grow while criterion iterates.
    g.bench_function("pages.create", |b| {
        let html = "<!doctype html><title>Bench page</title><h1>Bench</h1><p>tokio</p>";
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let t = Instant::now();
                let page = run(
                    &host,
                    "pages.create",
                    json!({ "html": html, "tags": ["bench"] }),
                );
                total += t.elapsed();
                let id = page["id"].as_str().unwrap();
                run(&host, "pages.delete", json!({ "id": id }));
                run(&host, "pages.purge", json!({ "id": id }));
            }
            total
        })
    });
    g.finish();

    let payloads = json!({
        "pages": n,
        "pages.list_all": payload_bytes(&host, "pages.list", json!({})),
        "pages.search_common": payload_bytes(&host, "pages.search", json!({ "query": "tokio" })),
        "pages.search_all": payload_bytes(&host, "pages.search", json!({ "query": "" , "limit": 100000 })),
        "today.summary": payload_bytes(&host, "today.summary", json!({})),
        "review.stats": payload_bytes(&host, "review.stats", json!({})),
        "manual": manual,
    });
    let out = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"))
        .join("herbarium-bench");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join(format!("payloads-{n}.json")),
        serde_json::to_string_pretty(&payloads).unwrap(),
    )
    .unwrap();
    eprintln!("payload bytes: {payloads}");
}

criterion_group!(benches, calibration, vault);
criterion_main!(benches);
