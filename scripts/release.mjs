// Bumps the app version everywhere it lives, writes CHANGELOG.md, commits and tags it
// (docs/releasing.md). Usage: npm run release -- X.Y.Z[-N] [--push] [--notes-only]
// Without --push it stops after tagging, so the commit can be checked first.
// --notes-only marks the tag so release.yml skips the checks and builds (D41).
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

// The Windows MSI bundler takes only a numeric pre-release and caps each part (D41).
const VERSION = /^(\d+)\.(\d+)\.(\d+)(?:-(\d+))?$/;
const MSI_LIMITS = [255, 255, 65535, 65535];
// release.yml looks for this in the tag message; keep the two in step.
const NOTES_ONLY_MARK = "[notes only]";

const run = (cmd, args, opts = {}) =>
  (
    execFileSync(cmd, args, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"], ...opts }) ??
    ""
  ).trim();

function fail(message) {
  console.error(`release: ${message}`);
  process.exit(1);
}

// [major, minor, patch, pre]; a release sorts after all of its pre-releases.
function parseVersion(v) {
  const m = VERSION.exec(v);
  if (!m) return null;
  return [Number(m[1]), Number(m[2]), Number(m[3]), m[4] === undefined ? Infinity : Number(m[4])];
}

function isNewer(next, current) {
  const [a, b] = [parseVersion(next), parseVersion(current)];
  for (let i = 0; i < 4; i++) if (a[i] !== b[i]) return a[i] > b[i];
  return false;
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
const notesOnly = args.includes("--notes-only");
// A mistyped --notes-only would otherwise start a full, billed build.
const unknown = args.filter((a) => a.startsWith("--") && a !== "--push" && a !== "--notes-only");
if (unknown.length > 0) fail(`unknown option ${unknown.join(", ")}`);

const parsed = next && parseVersion(next);
if (!parsed)
  fail("usage: npm run release -- X.Y.Z[-N] [--push] [--notes-only] (pre-release N is a number)");
if (parsed.some((part, i) => part !== Infinity && part > MSI_LIMITS[i])) {
  fail(`${next} is too large for the Windows installer (max 255.255.65535-65535)`);
}
for (const [tool, toolArgs] of [
  ["cargo", ["--version"]],
  ["npm", ["--version"]],
  ["npx", ["--no-install", "git-cliff", "--version"]],
]) {
  try {
    run(tool, toolArgs);
  } catch {
    fail(`${tool === "npx" ? "git-cliff (run npm ci)" : tool} is not available`);
  }
}
if (run("git", ["status", "--porcelain"]) !== "") fail("working tree is not clean");
const branch = run("git", ["branch", "--show-current"]);
if (branch !== "main") fail(`releases are cut from main, not '${branch}'`);
if (run("git", ["tag", "--list", `v${next}`]) !== "") fail(`tag v${next} already exists`);

const confPath = "src-tauri/tauri.conf.json";
const confText = readFileSync(confPath, "utf8");
const current = JSON.parse(confText).version;
if (!parseVersion(current)) fail(`current version ${current} in ${confPath} is not X.Y.Z[-N]`);
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
// Rebuilt from the whole history (cliff.toml), so editing a commit message fixes its entry.
run("npx", ["--no-install", "git-cliff", "--tag", `v${next}`, "--output", "CHANGELOG.md"]);

const files = [
  "Cargo.toml",
  "Cargo.lock",
  "package.json",
  "package-lock.json",
  confPath,
  "CHANGELOG.md",
];
run("git", ["add", ...files]);
run("git", ["commit", "-m", `chore(release): v${next}`]);
const tagMessage = notesOnly ? `reks v${next} ${NOTES_ONLY_MARK}` : `reks v${next}`;
run("git", ["tag", "-a", `v${next}`, "-m", tagMessage]);
console.log(`Committed and tagged v${next} (was ${current})${notesOnly ? ", notes only" : ""}.`);

if (push) {
  // Atomic, so the tag never lands without the commit it points at.
  run("git", ["push", "--atomic", "origin", "main", `v${next}`], { stdio: "inherit" });
  console.log("Pushed. The Release workflow builds a draft release from the tag.");
} else {
  console.log(`Check the commit and CHANGELOG.md, then: git push --atomic origin main v${next}`);
}
