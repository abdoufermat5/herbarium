#!/usr/bin/env node
// Usage: pnpm release <patch|minor|major|x.y.z[-pre]>
// Bumps package.json + src-tauri/Cargo.toml (tauri.conf.json follows package.json),
// refreshes Cargo.lock, then commits and tags. Push with: git push --follow-tags
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(-[0-9A-Za-z.-]+)?$/;
const arg = process.argv[2];
if (!arg) {
  console.error("Usage: pnpm release <patch|minor|major|x.y.z>");
  process.exit(1);
}

const run = (cmd, args) => execFileSync(cmd, args, { stdio: "inherit" });
const out = (cmd, args) => execFileSync(cmd, args, { encoding: "utf8" }).trim();

if (out("git", ["status", "--porcelain"])) {
  console.error("Working tree is not clean; commit or stash first.");
  process.exit(1);
}

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
const [, maj, min, pat] = pkg.version.match(SEMVER) ?? [];
if (!maj) throw new Error(`Current version is not semver: ${pkg.version}`);

const next =
  arg === "major" ? `${+maj + 1}.0.0`
  : arg === "minor" ? `${maj}.${+min + 1}.0`
  : arg === "patch" ? `${maj}.${min}.${+pat + 1}`
  : arg.replace(/^v/, "");
if (!SEMVER.test(next)) throw new Error(`Not a valid semver version: ${next}`);

pkg.version = next;
writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");

const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
writeFileSync(
  "src-tauri/Cargo.toml",
  cargo.replace(/^version = ".*"$/m, `version = "${next}"`),
);
run("cargo", ["update", "--manifest-path", "src-tauri/Cargo.toml", "--workspace", "--offline"]);

run("git", ["add", "package.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"]);
run("git", ["commit", "-m", `chore(release): v${next}`]);
run("git", ["tag", "-a", `v${next}`, "-m", `v${next}`]);
console.log(`\nTagged v${next}. Publish with: git push --follow-tags`);
