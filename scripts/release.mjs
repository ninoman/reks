// Bumps the app version everywhere it lives, commits and tags it (docs/releasing.md).
// Usage: npm run release -- X.Y.Z [--push]
// Without --push it stops after tagging, so the commit can be checked first.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

const run = (cmd, args, opts = {}) =>
  (
    execFileSync(cmd, args, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"], ...opts }) ??
    ""
  ).trim();

function fail(message) {
  console.error(`release: ${message}`);
  process.exit(1);
}

// Compares release versions only; a pre-release tag counts as lower than its release.
function isNewer(next, current) {
  const parse = (v) => v.split("-")[0].split(".").map(Number);
  const [a, b] = [parse(next), parse(current)];
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] > b[i];
  return current.includes("-") && !next.includes("-");
}

// Root Cargo.toml: `[workspace.package] version` and the internal crates' path deps.
// Third-party deps that happen to share the version are left alone.
function bumpCargoToml(text, from, to) {
  return text
    .split("\n")
    .map((line) => {
      const isWorkspaceVersion = line === `version = "${from}"`;
      const isInternalDep = line.includes("path = ") && line.includes(`version = "${from}"`);
      return isWorkspaceVersion || isInternalDep ? line.replace(`"${from}"`, `"${to}"`) : line;
    })
    .join("\n");
}

const args = process.argv.slice(2);
const next = args.find((a) => !a.startsWith("--"));
const push = args.includes("--push");

if (!next || !SEMVER.test(next)) fail("usage: npm run release -- X.Y.Z [--push]");
for (const tool of ["cargo", "npm"]) {
  try {
    run(tool, ["--version"]);
  } catch {
    fail(`${tool} is not on PATH`);
  }
}
if (run("git", ["status", "--porcelain"]) !== "") fail("working tree is not clean");
const branch = run("git", ["branch", "--show-current"]);
if (branch !== "main") fail(`releases are cut from main, not '${branch}'`);
if (run("git", ["tag", "--list", `v${next}`]) !== "") fail(`tag v${next} already exists`);

const confPath = "src-tauri/tauri.conf.json";
const confText = readFileSync(confPath, "utf8");
const current = JSON.parse(confText).version;
if (!isNewer(next, current)) fail(`${next} is not newer than ${current}`);

const cargo = readFileSync("Cargo.toml", "utf8");
const bumped = bumpCargoToml(cargo, current, next);
if (bumped === cargo) fail(`no version = "${current}" found in Cargo.toml`);
writeFileSync("Cargo.toml", bumped);

// Edits the one line, so the file keeps its own formatting.
writeFileSync(confPath, confText.replace(`"version": "${current}"`, `"version": "${next}"`));

// package.json + package-lock.json, no git side effects.
run("npm", ["version", next, "--no-git-tag-version", "--allow-same-version"]);
// Rewrites the workspace crates' entries in Cargo.lock; no registry access needed.
run("cargo", ["update", "--workspace", "--offline"]);

const files = ["Cargo.toml", "Cargo.lock", "package.json", "package-lock.json", confPath];
run("git", ["add", ...files]);
run("git", ["commit", "-m", `chore(release): v${next}`]);
run("git", ["tag", "-a", `v${next}`, "-m", `reks v${next}`]);
console.log(`Committed and tagged v${next} (was ${current}).`);

if (push) {
  // Atomic, so the tag never lands without the commit it points at.
  run("git", ["push", "--atomic", "origin", "main", `v${next}`], { stdio: "inherit" });
  console.log("Pushed. The Release workflow builds a draft release from the tag.");
} else {
  console.log(`Check the commit, then: git push --atomic origin main v${next}`);
}
