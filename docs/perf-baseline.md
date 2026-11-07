# Performance baseline

The "before" numbers of the performance program: unmodified `main` at d5465f2
plus only the tracing instrumentation (events are disabled at the default
`warn` level, so they cost nothing measurable). Measured with
`cargo bench -p herbarium-core --bench vault -- --warm-up-time 1 --measurement-time 3`
on a 14-thread Linux workstation (NVMe, ext4-style LVM), bench profile = the
shipped release profile (`opt-level = "s"`, LTO, one codegen unit). Other
agents were building at the same time, so treat differences under ~15% as noise.

Every operation goes through `Host::call` as `Caller::Ui` on a synthetic vault
(see [Reproducing](#reproducing)). Times are the **median** of criterion's
samples. Payload bytes are the serialized JSON of the operation's result.

| Benchmark | N = 1000 | N = 10 000 |
| --- | ---: | ---: |
| `vault/open_cold` (no index; reads and parses every page) | 17 856 ms | 207 536 ms ¹ |
| `vault/open_warm` (index exists, nothing changed) | 137 ms | 1 477 ms |
| `vault/rescan_unchanged` (`vault.rescan`) | 138 ms | 1 444 ms |
| `pages.list` (all pages, UI caller) | 169 ms | 2 434 ms |
| `pages.search` common term (`tokio`, about 100% of pages match) | 2 214 ms | 23 672 ms |
| `pages.search` zero-hit term | 34.6 ms | 469 ms |
| `pages.find_by_url` | 126 ms | 1 952 ms |
| `today.summary` | 249 ms | 5 052 ms |
| `review.stats` | 107 ms | 1 820 ms |
| `review.complete` (one page) | 7.9 ms | 18.5 ms |
| `storage.write` 100 KB | 8.8 ms | 20.5 ms |
| `pages.create` (one small page) | 15.1 ms | 60.9 ms |

¹ Median of three single runs, not a criterion sample (one open takes minutes).

Payload sizes:

| Result | N = 1000 | N = 10 000 |
| --- | ---: | ---: |
| `pages.list` (all) | 21 342 043 B (20.4 MB) | 298 634 749 B (285 MB) |
| `pages.search` common term (50-hit default lifted to all) | 21 442 160 B | 299 639 633 B |
| `today.summary` | 684 678 B | 1 273 642 B |
| `review.stats` | 11 936 B | 11 971 B |

What the numbers say: every list-style result carries each page's whole `ext`
(including the 300-800 KB of page state on 5% of pages), so `pages.list`
moves about 21 KB per page; the common-term search and `today.summary` grow
faster than linearly; `find_by_url` and `review.stats` are full scans; a cold
index is dominated by HTML parsing and one SQLite transaction per page.

## Reproducing

```sh
# whole benchmark (generates the vault on first use, reuses it afterwards)
HERB_BENCH_N=1000 HERB_BENCH_DIR=/tmp/herbarium-bench \
  cargo bench --manifest-path desktop/Cargo.toml -p herbarium-core --bench vault \
  -- --warm-up-time 1 --measurement-time 3
# HERB_BENCH_N=10000 for the large vault: about 8 minutes to generate, 2.2 GB
# of disk and over half an hour to run

# a vault for manual testing or e2e/tools/perf.mjs
cargo run --release --manifest-path desktop/Cargo.toml -p herbarium-core \
  --example gen_vault -- 10000 /tmp/perf-vault
```

The generated vault (`desktop/crates/herbarium-core/benches/support/vaultgen.rs`,
deterministic for a given N): HTML of 5-200 KB (mean about 55 KB) with
headings, recall markers, code blocks and `herbarium-app://open/<id>` links to
earlier pages; 3 levels of folders (10 topics x 4 x 3); `tag-0`..`tag-59`; a
source URL on 70% of pages; a review history of 0-30 entries on two thirds of
the pages and 8% of the pages overdue; 5% of the pages with 300-800 KB of page
state in the legacy `ext.storage` sidecar layout; highlights on 10%.

## CI gate

`docs/perf-baseline.json` holds the N = 1000 medians, the calibration median
and the payload sizes. The `perf` job in `ci.yml` runs the benchmark and
`node scripts/perf-check.mjs`, which fails when a median is more than 25%
above the baseline or a payload grew by more than 25%. Medians are first scaled
by the `calibration/json_roundtrip` benchmark so a faster or slower runner does
not trip the gate; the three fsync-bound write benchmarks are shown but never
fail it. After an intended change, refresh the baseline from a run on the
reference machine with `node scripts/perf-check.mjs --update`.

`scripts/bundle-budget.mjs` (run after `pnpm build`, also in CI) fails when
the main JS chunk is larger than `mainJsKB` in `scripts/bundle-budget.json`
(currently 777 KB, 5% over the 739.7 KB it is today).
