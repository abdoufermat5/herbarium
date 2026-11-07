// Compare the criterion results of `cargo bench -p herbarium-core --bench vault`
// (N = 1000) with the committed baseline in docs/perf-baseline.json.
//
//   cargo bench --manifest-path desktop/Cargo.toml -p herbarium-core --bench vault
//   node scripts/perf-check.mjs            # exit 1 when a median is >25% slower
//   node scripts/perf-check.mjs --update   # rewrite the baseline from the last run
//
// Medians are divided by the `calibration` benchmark's median (a fixed CPU
// workload; the three fsync-bound write benchmarks are only shown) before they are compared, so a slower or faster machine than the
// one that recorded the baseline does not trip the gate. Payload sizes are
// deterministic and compared as they are. Improvements never fail; lower the
// baseline with --update once a change lands.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const baselinePath = join(root, "docs", "perf-baseline.json");
const target = process.env.CARGO_TARGET_DIR ?? join(root, "desktop", "target");
const N = 1000;
const TOLERANCE = 0.25;

/** criterion's directory for a benchmark id: `/` splits groups, `.` stays. */
const median = (id) => {
  const dir = id === "calibration/json_roundtrip" ? "calibration_json_roundtrip" : id;
  const file = join(target, "criterion", dir, "new", "estimates.json");
  if (!existsSync(file)) throw new Error(`no result for ${id} (${file}); run the benchmarks first`);
  return JSON.parse(readFileSync(file, "utf8")).median.point_estimate;
};

const IDS = [
  "vault/open_cold",
  "vault/open_warm",
  "vault/rescan_unchanged",
  "ops/pages.list_all",
  "ops/pages.search_common",
  "ops/pages.search_zero_hit",
  "ops/pages.find_by_url",
  "ops/today.summary",
  "ops/review.stats",
  "ops/review.complete",
  "ops/storage.write_100kb",
  "ops/pages.create",
];
// Dominated by fsync, so they follow the runner's disk, not the code: shown,
// never failed on.
const DISK_BOUND = new Set(["ops/review.complete", "ops/storage.write_100kb", "ops/pages.create"]);

const payloadsFile = join(target, "herbarium-bench", `payloads-${N}.json`);
if (!existsSync(payloadsFile)) throw new Error(`no ${payloadsFile}; run the benchmarks with HERB_BENCH_N=${N}`);
const payloads = JSON.parse(readFileSync(payloadsFile, "utf8"));
delete payloads.manual;
if (payloads.pages !== N) throw new Error(`payloads were recorded for ${payloads.pages} pages, expected ${N}`);

const current = {
  calibrationNs: median("calibration/json_roundtrip"),
  medianNs: Object.fromEntries(IDS.map((id) => [id, median(id)])),
  payloadBytes: payloads,
};

if (process.argv.includes("--update")) {
  writeFileSync(baselinePath, JSON.stringify({ n: N, tolerance: TOLERANCE, ...current }, null, 2) + "\n");
  console.log(`wrote ${baselinePath}`);
  process.exit(0);
}

const base = JSON.parse(readFileSync(baselinePath, "utf8"));
const speed = current.calibrationNs / base.calibrationNs; // >1: this machine is slower
const fmt = (ns) => (ns >= 1e6 ? `${(ns / 1e6).toFixed(1)} ms` : `${(ns / 1e3).toFixed(1)} µs`);
let failed = false;
console.log(`machine speed vs baseline machine: x${speed.toFixed(2)} (calibration)`);
for (const id of IDS) {
  const expected = base.medianNs[id] * speed;
  const ratio = current.medianNs[id] / expected;
  const bad = ratio > 1 + (base.tolerance ?? TOLERANCE) && !DISK_BOUND.has(id);
  failed ||= bad;
  console.log(`${bad ? "FAIL" : "ok  "} ${id.padEnd(28)} ${fmt(current.medianNs[id]).padStart(10)}  expected ${fmt(expected).padStart(10)}  x${ratio.toFixed(2)}`);
}
for (const [key, bytes] of Object.entries(current.payloadBytes)) {
  if (key === "pages" || base.payloadBytes[key] === undefined) continue;
  const ratio = bytes / base.payloadBytes[key];
  const bad = ratio > 1 + (base.tolerance ?? TOLERANCE);
  failed ||= bad;
  console.log(`${bad ? "FAIL" : "ok  "} payload ${key.padEnd(20)} ${String(bytes).padStart(10)} B  baseline ${String(base.payloadBytes[key]).padStart(10)} B  x${ratio.toFixed(2)}`);
}
process.exit(failed ? 1 : 0);
