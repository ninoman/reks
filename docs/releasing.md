# Releasing

Why CI is shaped this way: [D31](decisions.md#d31-ci-budget-linux-on-prs-all-oses-at-release).
Build and signing policy: [D17](decisions.md#d17-distribution-and-signing-outside-app-stores).

## What runs when

| Trigger | Workflow | Runs on |
|---|---|---|
| PR opened / pushed (not draft; not docs-only) | [`ci.yml`](../.github/workflows/ci.yml) | Linux |
| PR that changes a manifest, lockfile or `deny.toml` | [`licenses.yml`](../.github/workflows/licenses.yml) | Linux |
| Tag `vX.Y.Z` pushed | [`release.yml`](../.github/workflows/release.yml) | version check, then CI on Linux + macOS + Windows and the licence gate, then the bundle builds |
| By hand (Actions tab > Run workflow) | `ci.yml` (tick `all_os` for the 3-OS matrix), `licenses.yml`, `keyfinder-parity.yml` | as chosen |

Pushes to `main` run nothing. Run the checks locally before merging
(`npm run lint && npm test && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`).

## Cutting a release

From a clean `main` that has everything you want to ship:

```sh
npm run release -- 0.2.0
git push --atomic origin main v0.2.0
```

`npm run release -- X.Y.Z` ([`scripts/release.mjs`](../scripts/release.mjs)) refuses a
dirty tree, a branch other than `main`, an existing tag, or a version that is not newer.
It sets the version in `Cargo.toml` (workspace and internal crate deps), `Cargo.lock`,
`package.json`, `package-lock.json` and `src-tauri/tauri.conf.json`, commits
`chore(release): vX.Y.Z` and creates an annotated tag. Add `--push` to push in the same step.

The tag starts `release.yml`. If every check passes, it builds macOS arm64 and x86_64,
Windows and Linux, and attaches the bundles to a **draft pre-release**. Review it on
GitHub and publish it by hand.

If a check fails, fix it on `main`, then cut the next patch version. Do not move a tag
that has been pushed.

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
