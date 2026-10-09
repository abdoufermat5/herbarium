#!/usr/bin/env node
// Usage: pnpm release <patch|minor|major|x.y.z[-pre]>
// Bumps package.json + desktop/Cargo.toml (tauri.conf.json follows package.json),
// rolls CHANGELOG.md [Unreleased] into the new version,
// refreshes Cargo.lock, then commits and tags. Push with: git push --follow-tags
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/;
const REPO = "https://github.com/abdoufermat5/herbarium";
const FILES = ["package.json", "desktop/Cargo.toml", "desktop/Cargo.lock", "CHANGELOG.md"];

const fail = (msg) => {
  console.error(msg);
  process.exit(1);
};

const arg = process.argv[2];
if (!arg) fail("Usage: pnpm release <patch|minor|major|x.y.z[-pre]>");

const run = (cmd, args) => execFileSync(cmd, args, { stdio: "inherit" });
const out = (cmd, args) => execFileSync(cmd, args, { encoding: "utf8" }).trim();
const tagExists = (tag) => {
  try {
    execFileSync("git", ["rev-parse", "-q", "--verify", `refs/tags/${tag}`], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
};

/** Compares two semver strings by SemVer 2.0 precedence; returns -1, 0 or 1. */
function compareSemver(a, b) {
  const [, ...pa] = a.match(SEMVER);
  const [, ...pb] = b.match(SEMVER);
  for (let i = 0; i < 3; i++) {
    const d = Number(pa[i]) - Number(pb[i]);
    if (d) return Math.sign(d);
  }
  const [preA, preB] = [pa[3], pb[3]];
  if (preA === preB) return 0;
  if (preA === undefined) return 1; // a release outranks its prereleases
  if (preB === undefined) return -1;
  const ia = preA.split(".");
  const ib = preB.split(".");
  for (let i = 0; i < Math.min(ia.length, ib.length); i++) {
    const [x, y] = [ia[i], ib[i]];
    if (x === y) continue;
    const [nx, ny] = [/^\d+$/.test(x), /^\d+$/.test(y)];
    if (nx && ny) return Math.sign(Number(x) - Number(y));
    if (nx) return -1; // numeric identifiers rank below alphanumeric ones
    if (ny) return 1;
    return x < y ? -1 : 1;
  }
  return Math.sign(ia.length - ib.length);
}

if (out("git", ["status", "--porcelain"])) fail("Working tree is not clean; commit or stash first.");

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
const current = pkg.version;
if (!SEMVER.test(current)) fail(`Current version is not semver: ${current}`);

const allTags = out("git", ["tag", "-l", "v*"])
  .split("\n")
  .map((t) => t.trim())
  .filter((t) => t.startsWith("v") && SEMVER.test(t.slice(1)));

const highestTag =
  allTags.length > 0
    ? allTags
        .map((t) => t.slice(1))
        .sort(compareSemver)
        .pop()
    : null;

const base = highestTag && compareSemver(highestTag, current) > 0 ? highestTag : current;
const [, maj, min, pat] = base.match(SEMVER) ?? [];

const next =
  arg === "major" ? `${+maj + 1}.0.0`
  : arg === "minor" ? `${maj}.${+min + 1}.0`
  : arg === "patch" ? `${maj}.${min}.${+pat + 1}`
  : arg.replace(/^v/, "");
if (!SEMVER.test(next)) fail(`Not a valid semver version: ${next}`);
if (tagExists(`v${next}`)) fail(`Tag v${next} already exists.`);

// Monotonicity check against highest existing tag:
if (highestTag && compareSemver(next, highestTag) <= 0) {
  fail(`Refusing to release ${next}: it must be greater than highest existing tag v${highestTag}.`);
}

// The current version may be released as-is only if it was never tagged (first release).
const currentReleased = tagExists(`v${current}`);
const cmp = compareSemver(next, current);
if (cmp < 0 || (cmp === 0 && currentReleased)) {
  fail(`Refusing to release ${next}: it must be greater than the current version ${current}.`);
}

// Validate the changelog before touching any file.
const log = readFileSync("CHANGELOG.md", "utf8");
const unreleased = log.match(/## \[Unreleased\]\r?\n([\s\S]*?)(?=\r?\n## \[|\r?\n\[Unreleased\]:)/);
if (!unreleased || !unreleased[1].trim()) {
  fail("CHANGELOG.md [Unreleased] is empty; document the changes first.");
}
if (!/^\[Unreleased\]: .*$/m.test(log)) fail("CHANGELOG.md has no [Unreleased]: link line.");

// The previous release is the newest tag strictly before next;
// with no tag at all this is the first release.
const prevTag =
  allTags
    .filter((t) => compareSemver(t.slice(1), next) < 0)
    .sort((a, b) => compareSemver(a.slice(1), b.slice(1)))
    .pop() ?? (currentReleased ? `v${current}` : null);
pkg.version = next;
writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");

const cargo = readFileSync("desktop/Cargo.toml", "utf8");
writeFileSync(
  "desktop/Cargo.toml",
  cargo.replace(/^version = ".*"$/m, `version = "${next}"`),
);

// Refresh Cargo.lock for the new crate version. Prefer the network (the offline index may
// lack entries), fall back to --offline, and roll the bump back if both fail.
const cargoUpdate = ["update", "--manifest-path", "desktop/Cargo.toml", "--workspace"];
try {
  run("cargo", cargoUpdate);
} catch {
  console.warn("cargo update failed; retrying with --offline");
  try {
    run("cargo", [...cargoUpdate, "--offline"]);
  } catch {
    run("git", ["checkout", "--", ...FILES]);
    fail("Could not refresh desktop/Cargo.lock (online or offline); version bump reverted.");
  }
}

// Roll the [Unreleased] changelog section into the new version.
const date = new Date().toISOString().slice(0, 10);
const versionLink = prevTag
  ? `${REPO}/compare/${prevTag}...v${next}`
  : `${REPO}/releases/tag/v${next}`;
writeFileSync(
  "CHANGELOG.md",
  log
    .replace(/## \[Unreleased\]\r?\n/, `## [Unreleased]\n\n## [${next}] - ${date}\n`)
    .replace(
      /^\[Unreleased\]: .*$/m,
      `[Unreleased]: ${REPO}/compare/v${next}...HEAD\n[${next}]: ${versionLink}`,
    ),
);

run("git", ["add", ...FILES]);
run("git", ["commit", "-m", `chore(release): v${next}`]);
run("git", ["tag", "-a", `v${next}`, "-m", `v${next}`]);
console.log(`\nTagged v${next}. Publish with: git push --follow-tags`);
