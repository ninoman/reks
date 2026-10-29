# Releasing

Why CI is shaped this way: [D31](decisions.md#d31-ci-budget-linux-on-prs-all-oses-at-release).
Build and signing policy: [D17](decisions.md#d17-distribution-and-signing-outside-app-stores).

## What runs when

| Trigger | Workflow | Runs on |
|---|---|---|
| PR opened / pushed (not draft; not docs-only) | [`ci.yml`](../.github/workflows/ci.yml) | Linux |
| PR that changes a manifest, lockfile or `deny.toml` | [`licenses.yml`](../.github/workflows/licenses.yml) | Linux |
| Tag `vX.Y.Z` pushed, or Release dispatched on it | [`release.yml`](../.github/workflows/release.yml) | version check, then CI on Linux + macOS + Windows and the licence gate, then the bundle builds |
| By hand: Cut release | [`cut-release.yml`](../.github/workflows/cut-release.yml) | Linux: bump, changelog, tag, then dispatches `release.yml` |
| By hand (Actions tab > Run workflow) | `ci.yml` (tick `all_os` for the 3-OS matrix), `licenses.yml`, `keyfinder-parity.yml` | as chosen |

Pushes to `main` run nothing. Run the checks locally before merging
(`npm run lint && npm test && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`).

## Versioning

One [SemVer](https://semver.org) version for the whole app ([D41](decisions.md#d41-versioning-one-semver-for-the-app-changelog-from-commits)).
It describes what users rely on, not the Rust API:

| Bump | When | Commit |
|---|---|---|
| Major | An older reks can no longer open the library (any schema migration), or a feature, CLI command, import or export target is removed | `feat(core)!: ...` or a `BREAKING CHANGE:` footer |
| Minor | New features, a new export target, a new analyser version | `feat: ...` |
| Patch | Fixes only, including export fixes that keep the format | `fix: ...`, `perf: ...` |

While the version is 0.x, a breaking change bumps the minor version instead. 1.0.0 waits
for a legacy USB export with a logged CDJ test in [`hw-test-log.md`](hw-test-log.md).

Pre-releases are `X.Y.Z-N`, `N` a number (`0.3.0-1`, `0.3.0-2`, then `0.3.0`). Windows
MSI builds fail on `-beta.1`, and on parts above 255.255.65535-65535.

## Cutting a release

From GitHub: Actions > **Cut release** > Run workflow, on `main`, with the version
(`0.2.0`, no `v`). [`cut-release.yml`](../.github/workflows/cut-release.yml) runs the
script below with `--push` as `github-actions[bot]`, then starts `release.yml` on the
new tag (a tag pushed with the workflow token starts nothing by itself). You do not get
to read the changelog before the tag exists; it is in the run summary and in the draft
release, and you can still edit the draft's notes before publishing. Untick **build** for
a notes-only release (below).

Or from a clean local `main` that has everything you want to ship (after `npm ci`):

```sh
npm run release -- 0.2.0
git push --atomic origin main v0.2.0
```

`npm run release -- X.Y.Z` ([`scripts/release.mjs`](../scripts/release.mjs)) refuses a
dirty tree, a branch other than `main`, an existing tag, or a version that is not newer
or does not fit the rules above. It sets the version in `Cargo.toml` (workspace and
internal crate deps), `Cargo.lock`, `package.json`, `package-lock.json` and
`src-tauri/tauri.conf.json`, rewrites [`CHANGELOG.md`](../CHANGELOG.md) with
[git-cliff](https://git-cliff.org) ([`cliff.toml`](../cliff.toml)), commits
`chore(release): vX.Y.Z` and creates an annotated tag. Add `--push` to push in the same step.

The changelog lists breaking changes, features, fixes and speed-ups from commit subjects;
tests, docs, chores and merges are left out. Each section covers the commits since the
previous tag, so `0.3.0` after `0.3.0-1` lists only what came after the pre-release.
Read the new section before pushing. To
reword an entry, `git commit --amend` the release commit's `CHANGELOG.md` and move the
tag with `git tag -fa vX.Y.Z` (only before it is pushed).

The tag starts `release.yml`. It fails at once if `CHANGELOG.md` has no section for the
tag's version. If every check passes, it builds macOS arm64 and x86_64, Windows and
Linux, and attaches the bundles to a **draft pre-release** whose notes are that section.
Review it on GitHub and publish it by hand.

### Notes-only releases

To mark a version without spending build minutes, untick **build** in Cut release, or
run `npm run release -- X.Y.Z --notes-only`. The version, changelog and tag are the same,
but the tag message carries `[notes only]`, so `release.yml` skips the checks and builds
(about a Linux minute instead of 500–600 billed minutes) and drafts a pre-release with the
changelog section and no installers. To get installers later, cut the next version with
a build; a tag is never rebuilt.

If a check fails, fix it on `main`, then cut the next patch version. Do not move a tag
that has been pushed.

## Local builds

To build a macOS DMG on your own machine, without CI:

```sh
npm ci
npm run bundle:dmg
```

The DMG is written to `target/release/bundle/dmg/` (this is the workspace `target/` at the
repo root). Tauri deletes the intermediate `reks.app` afterwards; to keep it, run
`npm run tauri build -- --bundles app,dmg`. The DMG is built for your Mac's architecture only. For a universal build, run
`rustup target add x86_64-apple-darwin aarch64-apple-darwin` once, then
`npm run bundle:dmg -- --target universal-apple-darwin`; the output goes under
`target/universal-apple-darwin/release/`. To bundle for the host OS's other formats, run
`npm run tauri build`.

Every build sends in-app reports to the live relay; the URL comes from `.cargo/config.toml` ([feedback.md](feedback.md)).

## Unsigned builds

Until the Apple and Windows signing accounts exist (D17), builds are unsigned. On macOS
`bundle.macOS.signingIdentity` is `"-"` in `tauri.conf.json`, which ad-hoc signs the whole
bundle. Without it the app is only linker-signed, and a downloaded copy fails with
"reks is damaged" instead of offering Privacy & Security > **Open Anyway**. Check with
`codesign --verify --deep --strict reks.app`. A real identity in `APPLE_SIGNING_IDENTITY`
overrides it later.

## Cost of a release

The 3-OS checks plus four builds come to roughly 500–600 billed minutes
(two of the builds and one of the check jobs are on macOS, which is billed at 10×). Check
usage under Settings > Billing.
