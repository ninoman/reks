# Bug reports and feedback

The **Report a problem or idea** button in the app header (speech bubble)
opens a dialog for a bug, a feature request or general feedback. Error
messages have a **Report** link that opens it as a bug with the error
filled in. That covers inline errors
([`ErrorMessage`](../src/components/ErrorMessage.tsx)), error toasts and the
"reks closed unexpectedly" notice. Why it works this way: D34 in
[`decisions.md`](decisions.md).

## What is sent

Only what the dialog shows, and only when the user presses **Send**:

- the title, the description, and for bugs the steps and the expected result
- the error the report was started from, if any
- the email, if the user gave one
- **diagnostics**, if ticked (on by default for bugs):
  - app version, OS and architecture
  - library schema version, track count and playlist count
  - the end of the newest log file (at most 300 lines). Every failed command
  and every uncaught UI error is logged, so this shows what failed just
  before the report.
  - the newest crash file from the last week

In diagnostics, the home folder becomes `~` and each library folder
becomes `<library-root-N>`, so most paths show only file names below them.
**Show what's sent** previews the exact text.

Logs and crash files stay in the app log dir until a report is sent:

- macOS: `~/Library/Logs/org.reks.app/`
- Windows: `%LOCALAPPDATA%\org.reks.app\logs\`
- Linux: `~/.local/share/org.reks.app/logs/`

The app keeps 5 days of logs (`reks.<date>.log`) and the 5 newest crash
files (`crash-<utc>.txt`).

If sending fails, or the build has no relay, **Save to file…** writes the
same report as a text file to send by other means.

## Flow

```
app ──POST /report──▶ feedback relay (Cloudflare Worker) ──GraphQL──▶ private Projects board (draft item)
```

- App: [`ReportDialog`](../src/components/ReportDialog.tsx) and
  [`store/report.ts`](../src/store/report.ts).
- Commands: [`report.rs`](../src-tauri/src/report.rs).
- Rendering and redaction:
  [`reks_core::diagnostics`](../crates/reks-core/src/diagnostics.rs).
- Relay: [`services/feedback-relay`](../services/feedback-relay). It
  validates the body (kind, title ≤ 256 characters, body ≤ 65,000 characters,
  `X-Reks-Version` header) and rate-limits to 3 reports a minute per IP.
  Then it adds a draft item to the board set in `wrangler.toml`
  (`PROJECT_OWNER`, `PROJECT_NUMBER`) and sets two single-select fields:
  - **Kind**: `Bug`, `Feature` or `Feedback` (GitHub reserves "Type")
  - **Status**: `Needs triage`

  Field and option names match without case. A field or option the board
  lacks is skipped, and the item is still added.

## One-time setup (maintainer)

1. On the board (<https://github.com/users/ninoman/projects/1>, private):
   - add a single-select field **Kind** with options `Bug`, `Feature` and `Feedback`
   - add a `Needs triage` option to **Status**
2. Create a **classic** personal access token with only the `project`
   scope. Fine-grained tokens didn't support boards owned by a user account
   when this was written (2026-10); if GitHub's token page now offers
   **Projects: Read and write** for your account, use that instead. Either
   way the token can edit boards but not repo code.
3. Deploy the relay:
   ```bash
   cd services/feedback-relay && npm ci && npx wrangler secret put GITHUB_TOKEN && npm run deploy
   ```
4. In the `reks` repo settings, add the Actions **variable**
   `REKS_FEEDBACK_URL` set to `https://<worker>.workers.dev/report`. Release
   builds pick it up (`release.yml`). Local builds get the same URL from
   [`.cargo/config.toml`](../.cargo/config.toml); update both if the relay moves.

Local test: run `npx wrangler dev` with `GITHUB_TOKEN` in `.dev.vars`
(point `PROJECT_NUMBER` at a scratch board if you like). Then run the app with
`REKS_FEEDBACK_URL=http://localhost:8787/report npm run tauri dev` (a set
variable overrides `.cargo/config.toml`). Without it, dev builds send to the
real board.

## Triage

1. Filter the board by `status:"Needs triage"`.
2. Reproduce the report, then archive it (duplicate or noise) or convert it
   to an issue (**Convert to issue**, then pick the repo).
3. Move it out of Needs triage.
4. Before converting to an issue in the public repo, remove the tester's
   email and anything personal from the body.
