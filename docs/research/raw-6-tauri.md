# Research 6 — Tauri 2 specifics for a GPLv3 desktop DJ-library app

Researched 2026-10-18. Every factual claim cites a URL from the Sources list ([Sn]). Items I could not confirm are marked **UNVERIFIED**.

## 0. Current versions (as of 2026-10-18)

| Thing | Version | Date | Licence | MSRV | Source |
|---|---|---|---|---|---|
| `tauri` stable | **2.12.4** (3.0.0-alpha.5 also exists) | 2026-10-14 | MIT/Apache-2.0 | **Rust 1.90** (3.0 alpha: 1.95) | [S1][S2] |
| `symphonia` | **0.6.1** (0.6.0 was 2026-05-15; 0.5.5 was 2025-10-11) | 2026-08-13 | MPL-2.0 | 1.85 | [S3] |
| `cpal` | 0.18.2 | 2026-08-16 | Apache-2.0 | — | [S4] |
| `rodio` | 0.23.0 | 2026-10-11 | MIT OR Apache-2.0 | — | [S5] |
| `kira` | 0.12.5 | 2026-09-26 | MIT OR Apache-2.0 | — | [S6] |
| `web-audio-api` | 1.7.0 | 2026-08-08 | MIT | — | [S7] |
| `rusqlite` | 0.40.2 | 2026-08-08 | MIT | — | [S8] |
| `rusqlite_migration` | 2.6.0 | 2026-05-28 | Apache-2.0 | — | [S9] |
| `tauri-plugin-fs` | 2.7.1 | — | MIT/Apache-2.0 | — | [S10] |
| `sysinfo` | 0.39.6 | — | MIT | — | [S11] |

Tauri's updater and sql plugins state "Rust 1.90 or later" [S12][S13], matching the core crate's `rust_version`. All licences above are GPLv3-compatible (MPL-2.0 is explicitly GPL-compatible via its secondary-licence clause; MIT/Apache/BSD are permissive).

## 1. Audio playback from Rust

### 1.1 Decoding: Symphonia 0.6
- Pure-Rust demux/decode, MPL-2.0. Codec status per README/docs.rs [S14][S15]:
  - MP3 "Excellent", gapless **yes**, feature `mp3` (or `mpa`), **not default**.
  - FLAC "Excellent", gapless yes, default. PCM/WAV "Excellent", gapless yes, default. Vorbis "Excellent", gapless yes, default.
  - ALAC "Great", gapless yes, feature `alac`, not default. ADPCM "Good", default.
  - **AAC-LC only** ("Great", gapless **no**, feature `aac`, not default). `he-aac` / `he-aac-v2` feature flags exist in the table but have **no status entry**; the README points to the third-party `symphonia-adapter-fdk-aac` (libfdk-aac) for HE-AAC/HE-AACv2 [S14]. Treat native HE-AAC as **UNVERIFIED/unsupported**.
  - Opus and WavPack appear in the table with no status [S14] — **UNVERIFIED** maturity.
  - Formats: WAV, OGG, MKV default; AIFF (`aiff`, gapless yes), ISO/MP4 (`isomp4`, gapless no), CAF — not default. Tags: ID3v1/v2, APE, MP4, RIFF, Vorbis comments [S14].
- Royalty note from README: by default only "royalty-free open standard codecs and formats" are enabled; MP3/AAC/ALAC require opt-in feature flags [S14]. MP3's patents are reported expired (2017) while AAC remains patent-encumbered [S16] — a GPLv3 project usually ships them anyway (as other FOSS players do), but note it in the licence README. 
- **Gapless in 0.6 moved to the decoder**: `AudioDecoderOptions { gapless: bool }` — "When enabled, the decoder will trim any delay or padding frames. Default: true" [S17]. (In 0.5 it was `FormatOptions::enable_gapless`, default false [S18].) 0.6 `FormatOptions` now has `prebuild_seek_index`, `seek_index_fill_period_ms` (default 1000 ms), `external_data` [S19].
- Seeking: `FormatReader::seek(SeekMode::{Accurate,Coarse}, SeekTo::{Time,TimeStamp}) -> SeekedTo`. Accurate lands at-or-before the target, coarse may land after; granularity is per-packet so decode forward to the exact frame; **reset decoders after a seek** [S20]. For MP3 (no index) set `prebuild_seek_index` or accept progressive index build [S19].
- **Alternative full-coverage decoder**: `ffmpeg-next`/`ffmpeg-sys-next` (crate wrappers; FFmpeg itself is LGPL-2.1+ or GPL depending on configure flags). ffmpeg-next has `build` (compile FFmpeg from source) and `static` features; Windows static links need extra system libs in your own `build.rs` [S21]. GPL FFmpeg is licence-compatible with a GPLv3 app, but bundling means shipping ~tens of MB of shared libs per platform and matching FFmpeg majors in CI. The crate-level licence metadata for ffmpeg-next/ffmpeg-sys-next is inconsistent across aggregators (WTFPL vs MIT/Apache reported) — **UNVERIFIED**; check each crate's LICENSE file.

### 1.2 Output back-ends
| Option | Licence | Model | Device select | Seek/gapless | Notes |
|---|---|---|---|---|---|
| **cpal** 0.18 | Apache-2.0 | Pull callback on a dedicated high-priority OS thread (CoreAudio/WASAPI; ALSA gets a thread per stream); `build_output_stream(config, data_cb, err_cb, timeout)`; streams start paused until `play()` [S22] | `host.devices()`, `default_output_device()` (returns `Option`) [S22] | You implement seeking yourself (it is just a sample sink) | `BufferSize::Fixed(n)` requests callback size; `SupportedBufferSize` gives device min/max [S22]. Optional JACK/PipeWire/PulseAudio/ASIO features [S22]. |
| **rodio** 0.23 | MIT/Apache | Background thread mixes `Source`s and writes to the cpal device; `DeviceSinkBuilder::open_default_sink()` → `MixerDeviceSink`; `Player::connect_new(&mixer)` plays appended sources sequentially [S23] | Re-exports cpal `Device`; build sink from a device (feature `playback`) [S23] | `Source::try_seek(Duration)`; returns `SeekError::NotSupported` if an underlying source can't seek; saturates at end when duration known [S24] | Uses **Symphonia by default** (`symphonia-all` or per-codec `symphonia-mp3/aac/isomp4/...`); default decoders flac, mp3, mp4, vorbis, wav. Alternative MIT decoders (minimp3, claxon, hound, lewton) if you want to avoid MPL [S23]. |
| **kira** 0.12 | MIT/Apache | Game-oriented; cpal backend (default feature), `StaticSoundData` (whole file in RAM, cheap clone) vs `StreamingSoundData` [S25] | Not documented on the front page — **UNVERIFIED** | Not documented on front page — **UNVERIFIED** | Symphonia feature default; mp3/ogg/flac/wav on by default [S25]. Good for cue-point/loop/tween effects, overkill for a preview player. |
| **web-audio-api** 1.7 | MIT | Pure-Rust Web Audio API for non-browser contexts [S7] | (cpal underneath) | Graph-based | Niche; useful only if you want the same API on both sides. |
| **WebView `<audio>`/Web Audio** | n/a | Decoding/output done by the OS webview | Browser default device only (`setSinkId` support varies — UNVERIFIED per engine) | Native seek, no sample-accurate gapless | Serve files via Tauri's **asset protocol** (`app.security.assetProtocol.enable` + `scope`) [S26]. Codec support is **engine-dependent** (see §6). |

**Recommendation**: decode with Symphonia on a `tauri::async_runtime::spawn_blocking` worker (Tauri's runtime is Tokio; `spawn_blocking` is the documented place for heavy file I/O / CPU work [S27]), push f32 interleaved frames into a lock-free SPSC ring buffer, and have the cpal callback drain it. rodio gives you that for free (its background thread + `Player`) if you don't need sample-accurate cueing; drop to raw cpal if you need DJ-style instant cue/seek with a resampler. `#[tauri::command(async)]` or `async fn` commands run off the main thread (async commands are spawned via `async_runtime::spawn`) [S28]. Keep the `cpal::Stream` alive in managed state (`app.manage(Mutex<Player>)`); it is `!Send` on some hosts, so own it on a dedicated thread and talk to it via channels.

## 2. File-system access to removable drives

- **Scope only governs the fs plugin's JS commands.** Permissions alone don't grant a scope; calls outside an `allow` scope fail with `forbidden path`; `deny` beats `allow`; patterns like `$HOME/**/*` vs `$APPDATA/*` (direct children only) [S29]. The plugin's own docs say for Rust-side file work "use `std::fs` or `tokio::fs` directly" — Rust commands are **not** scope-restricted [S29]. Globs for removable volumes would be e.g. `{"path":"/Volumes/**"}` (macOS), `{"path":"/media/**"}` and `{"path":"/run/media/**"}` (Linux), `{"path":"D:\\**"}` etc. (Windows) — syntax from the same page; the concrete removable-volume examples are mine, not from the docs.
- Dotfiles: on Unix, dot-components need an explicit glob after the dot unless `plugins.fs.requireLiteralLeadingDot=false` [S29] — relevant for `._*` AppleDouble files on FAT32 sticks.
- **Runtime scope changes from Rust**: `tauri_plugin_fs::FsExt::fs_scope()` / `try_fs_scope()` return `tauri::scope::fs::Scope`; `Scope::allow_directory(path, recursive)`, `allow_file(path)`, `is_allowed(path)`; forbidden rules always win [S30][S31]. `Manager::asset_protocol_scope()` returns the same `Scope` type (feature `protocol-asset`) [S32] — use it to whitelist a picked folder for `<audio src="asset://...">` or `convertFileSrc()`.
- **Dialog plugin**: JS `open()` docs: "The selected paths are added to the filesystem and asset protocol scopes." "Note that the scope change is not persisted, so the values are cleared when the application is restarted." Persist with `tauri-plugin-persisted-scope`; and "When security is more important than the easy of use of this API, prefer writing a dedicated command instead." [S33]. Folder picker: `open({directory:true})`; returns native paths on desktop; folder picking unsupported on iOS/Android [S34].
- **Enumerating volumes**: `sysinfo::Disks::new_with_refreshed_list()` → `Disk::{name, mount_point, file_system (e.g. "EXT4","NTFS"), kind, is_removable, is_read_only, total_space, available_space}` [S11]. `file_system()` is how you detect FAT32/exFAT (strings like `msdos`/`exfat` on macOS, `FAT32`/`exFAT` on Windows — exact strings **UNVERIFIED**, test per OS). Windows-native: `windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(PCWSTR) -> u32` (DRIVE_REMOVABLE) [S35]; feature flag `Win32_Storage_FileSystem` expected — **UNVERIFIED** (not shown on the docs page). macOS: `diskutil list -plist` / `diskutil info -plist <dev>` (shell out).
- **Eject/unmount**: macOS `diskutil eject <mountpoint>` or `diskutil unmountDisk` (shell out; general knowledge, no doc fetched). Windows community guidance: flush → `FSCTL_LOCK_VOLUME` → `FSCTL_DISMOUNT_VOLUME` → `IOCTL_STORAGE_EJECT_MEDIA` on a handle opened `GENERIC_READ|GENERIC_WRITE` (needs admin for flush), or device-level `CM_Request_Device_Eject` (works for standard users); `DeviceIoControl` may return TRUE even when eject failed [S36]. Simplest robust path: close all handles, `sync`, then tell the user to eject (or shell `diskutil`/PowerShell `(New-Object -comObject Shell.Application).NameSpace(17).ParseName("D:").InvokeVerb("Eject")`).
- **macOS TCC**: access to removable volumes is gated by the `kTCCServiceSystemPolicyRemovableVolumes` service; Apple's reset doc maps `tccutil reset SystemPolicyRemovableVolumes` to it [S37]. Add `NSRemovableVolumesUsageDescription` to Info.plist (string explaining why) — without it, developers report a silent "you don't have permission" error instead of a prompt [S37]. Tauri: create `src-tauri/Info.plist` with extra keys; it is merged with the CLI-generated plist [S38]. The user-visible toggle appears under Privacy & Security ▸ Files and Folders ("Removable Volumes") — **UNVERIFIED** from Apple docs (search found only developer reports).

## 3. SQLite from Rust

- **rusqlite** (MIT) with `features=["bundled"]` compiles SQLite in — no system lib, consistent version across OSes [S8]. Migrations: `rusqlite_migration` (Apache-2.0) tracks schema version via `PRAGMA user_version` instead of a table [S9]. Enable WAL with `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;` on open (SQLite standard practice; not re-verified here).
- **tauri-plugin-sql**: wraps **sqlx** (SQLite/MySQL/Postgres via feature), exposes `Database.load('sqlite:test.db')` to JS (path relative to the AppConfig dir), has a Rust-side `Migration {version, description, sql, kind: MigrationKind::Up}` + `add_migrations()` API, each migration set runs in a transaction; but it documents **no Rust-side query API** [S13]. If the backend owns the DB, the plugin adds nothing — skip it and use rusqlite (or sqlx) directly from commands. (Community confirms rusqlite-based plugins as the common backend-owned choice [S39].)
- **Where to store**: `app.path().app_data_dir()` → `data_dir/<bundle_identifier>`: macOS `~/Library/Application Support/<id>`, Windows `%APPDATA%` (FOLDERID_RoamingAppData), Linux `$XDG_DATA_HOME` (~/.local/share) [S40]. Prefer `app_local_data_dir()` on Windows if you don't want roaming profiles to sync a large DB (FOLDERID_LocalAppData) [S40]. Waveform caches → `app_cache_dir()` [S40].
- Threading: rusqlite `Connection` is `!Sync`; keep one writer connection behind a `Mutex` in managed state (or a small pool), and run queries in `spawn_blocking` [S27].

## 4. Signing & notarization outside app stores

### macOS
- **Developer ID Application** cert is the outside-App-Store cert; only the Account Holder can create it; a free Apple account can build but **cannot notarize**, and the app shows as unverified [S41]. Apple Developer Program: **$99/year** [S42].
- Tauri bundler env vars: `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`; notarization via Apple ID (`APPLE_ID`, `APPLE_PASSWORD` app-specific, `APPLE_TEAM_ID`) or App Store Connect API key (`APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH`); `--skip-stapling` available [S41]. Hardened runtime is **on by default** (`bundle.macOS.hardenedRuntime: true`), entitlements via `bundle.macOS.entitlements` path, `signingIdentity`, `minimumSystemVersion` default 10.13, `providerShortName` [S26][S38].
- Entitlements: audio **output** needs none; `com.apple.security.device.audio-input` is the microphone entitlement (needed only if you ever record). Apple's hardened-runtime page did not load through the fetcher — this statement is general knowledge, **UNVERIFIED** against the live page [S43].
- Notarization requirements (Apple): Developer ID signature, hardened runtime, secure timestamp, no `get-task-allow`, submitted with `xcrun notarytool` and stapled [S44] — Tauri's bundler performs these steps when the env vars are set [S41].
- **Ad-hoc** (`signingIdentity: "-"`) is possible and required on Apple Silicon (all downloaded binaries must carry *some* signature), but it "does not stop macOS from requiring users to approve the app in Privacy & Security" [S41].
- **Gatekeeper behaviour**: Apple (Aug 2024): in Sequoia "users will no longer be able to Control-click to override Gatekeeper"; they must go to System Settings ▸ Privacy & Security [S45]. Apple's support doc now documents only the "Open Anyway" flow (attempt launch → Privacy & Security → Open Anyway) [S46]. Third-party guides say macOS 26 Tahoe (26.2+/26.5) tightened this further and that `spctl --master-disable` no longer reliably exposes "Anywhere"; `xattr -dr com.apple.quarantine` still reportedly works but is a security downgrade — all **UNVERIFIED** (no Apple source) [S47]. Practical conclusion: for a public download, pay the $99 and notarize; unsigned builds are only viable for devs/testers.
- Universal binaries: Tauri's own CI example builds `aarch64-apple-darwin` and `x86_64-apple-darwin` separately and does not use `universal-apple-darwin` [S48]; the updater's `latest.json` keys are `darwin-aarch64` / `darwin-x86_64` by default, and a universal build needs a custom target key that matches [S12].

### Windows
- Tauri config `bundle.windows.{certificateThumbprint, digestAlgorithm (sha256), timestampUrl}` drives `signtool`; `signCommand` ("%1" = file) lets you plug any signer and is required when cross-compiling from Linux/macOS [S49]. Docs include recipes for Azure Key Vault via `relic` and for **Azure Artifact Signing** (formerly Trusted Signing) via `cargo install artifact-signing-cli` + `AZURE_CLIENT_ID/SECRET/TENANT_ID` [S49].
- **Since 2024 EV certs no longer get instant SmartScreen reputation**; OV and EV build reputation the same way; keep signing with the same cert so reputation carries over; you can submit to Microsoft for review [S49][S50].
- **Azure Artifact Signing**: Public Trust available to organisations in US, CA, EU, UK, AU, NZ, JP, KR, SG, CH, NO, IL; **individual developers only in US/Canada** (identity via government ID + Microsoft Authenticator Verified ID; billing account must be type Individual) [S51]. Requires a paid Azure subscription (no free/trial) [S50]. Basic SKU = 5,000 sigs/month, Premium = 100,000; the pricing page currently shows "$-" (use the calculator) — the oft-quoted **$9.99/mo Basic is UNVERIFIED** today [S52]. It does **not** issue EV certs [S50]. The "3-year verifiable history" rule appeared in 2025 preview communications [S53]; the current quickstart lists only country restrictions, so treat the 3-year rule as **UNVERIFIED/possibly lifted**.
- **SignPath Foundation** (free for OSS): OSI licence, no commercial dual-licensing, binaries must come from a verifiable automated CI build from the project's own repo, MFA for all members, defined author/reviewer/approver roles, every signing request manually approved, project page must carry a "Code signing policy" section with the text "Free code signing provided by SignPath.io, certificate by SignPath Foundation" [S54][S55]. Cert is issued in SignPath Foundation's name (OV/EV not stated) [S54]. Fits a GPLv3 donation-funded project well; integrate via `signCommand` or sign in a post-build CI step.
- **Certum**: cloud (SimplySign) Standard code signing "from €49" and EV "from €209" on the official shop; an "Open Source Code Signing" SKU (~$49.99) is reported by third parties only — **UNVERIFIED** [S56].

### Linux
- Tauri 2 bundles AppImage/deb/rpm. AppImage via linuxdeploy; build on the oldest glibc you support that has WebKitGTK 4.1 (Ubuntu 22.04 / Debian 12) [S57]. `bundle.linux.appimage.bundleMediaFramework=true` adds GStreamer for HTML5 audio/video (+15–35 MB; fully supported only on Ubuntu build hosts; "ugly" plugins may be hard to distribute) [S26][S57]. If you play audio from Rust (cpal), you don't need this; you do need ALSA/PulseAudio/PipeWire client libs present on the host (cpal's ALSA backend) — standard on desktops.
- Build deps: `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev` [S58].

## 5. Updater + CI

- `tauri-plugin-updater` (Rust 1.90+): `tauri signer generate -w ~/.tauri/app.key` creates the keypair; the docs don't name the scheme (minisign-compatible per the CLI output — **UNVERIFIED** here). Set `TAURI_SIGNING_PRIVATE_KEY` (+ `_PASSWORD`) at build time (`.env` files don't work); `bundle.createUpdaterArtifacts: true`; `plugins.updater.pubkey` + `endpoints` (HTTPS enforced; `{{target}}`, `{{arch}}`, `{{current_version}}` placeholders) [S12].
- Artifacts: macOS `.app.tar.gz` + `.sig`; Windows `.msi`/`.exe` + `.sig` (installMode `passive` default); **Linux: only AppImage is updatable** (+`.sig`) — deb/rpm users update via package manager [S12].
- Static `latest.json`: `version`, `notes`, `pub_date`, `platforms.{linux-x86_64,windows-x86_64,darwin-x86_64,darwin-aarch64}.{url,signature}`; whole file validated before version check; host it at `https://github.com/<o>/<r>/releases/latest/download/latest.json` [S12].
- **tauri-action@v1** (`tagName: app-v__VERSION__`, `releaseName`, `releaseBody`, `releaseDraft`, `prerelease`, `args: --target ...`) generates and uploads `latest.json` (`uploadUpdaterJson` default true; `updaterJsonPreferNsis`) [S59]. Official matrix: `macos-latest` ×2 targets, `ubuntu-22.04`, `ubuntu-22.04-arm` (public repos), `windows-latest`; only `GITHUB_TOKEN` shown — add the APPLE_*/TAURI_SIGNING_* secrets yourself [S48]. ARM AppImages must be built on ARM runners [S57].

## 6. Gotchas

- **IPC cost**: command return values are JSON-serialised; for waveforms/PCM return `tauri::ipc::Response::new(Vec<u8>)` (arrives as ArrayBuffer) [S28]. **`Channel<Vec<u8>>` is still JSON** (array of numbers); to stream raw bytes use `Channel<InvokeResponseBody>` and send `InvokeResponseBody::Raw(bytes)` [S60].
- **Progress**: use `tauri::ipc::Channel<T>` (ordered, fast, scoped to the invocation) for scan/analysis progress; the event system (`emit`) is JSON-only, evaluates JS directly, may reorder rapid events and is "not designed for low latency or high throughput" [S61].
- **Threading**: Tauri's runtime is Tokio; `async_runtime::{spawn, spawn_blocking, block_on, handle}`; non-async commands run on the main thread unless `#[tauri::command(async)]` [S27][S28]. Borrowed args (`&str`, `State<'_, T>`) in async commands need owned types or a `Result` return [S28].
- **WebViews**: Windows = WebView2 (Chromium, self-updating, installer bootstraps it; included since Win10 1803); macOS = WKWebView (updates only with the OS); Linux = webkit2gtk (version per distro) [S62][S58]. Tauri's page does not discuss codecs. Inference: WebView2 ≈ Edge/Chromium codec set (AAC, MP3, FLAC, WAV reported for Edge) — **UNVERIFIED for WebView2 specifically** [S16]; WKWebView ≈ Safari (MP3/AAC/ALAC/FLAC in M4A/MP4 — general knowledge, **UNVERIFIED**); webkit2gtk plays only what the host's GStreamer plugins decode — AAC needs gst-libav/fdk-aac and users report failures [S63]. This is the strongest argument for doing preview playback in Rust (Symphonia + cpal) rather than `<audio>`: one codec path on all three OSes.
- **Multi-window**: `app.windows[]` in config, each needs a unique `label`; or `WebviewWindowBuilder` at runtime [S26]. Commands receive `tauri::WebviewWindow`/`AppHandle` params to know who called [S28].
- **PATH**: GUI apps on macOS/Linux don't inherit shell `$PATH` (matters if you shell out to `diskutil`/`ffmpeg`); use `fix-path-env-rs` [S38][S57].
- MSI builds need the VBSCRIPT optional Windows feature on the build host [S58].

## Sources
- [S1] https://github.com/tauri-apps/tauri/releases
- [S2] https://crates.io/api/v1/crates/tauri
- [S3] https://crates.io/api/v1/crates/symphonia
- [S4] https://crates.io/api/v1/crates/cpal
- [S5] https://crates.io/api/v1/crates/rodio
- [S6] https://crates.io/api/v1/crates/kira
- [S7] https://crates.io/api/v1/crates/web-audio-api
- [S8] https://crates.io/api/v1/crates/rusqlite
- [S9] https://crates.io/api/v1/crates/rusqlite_migration
- [S10] https://docs.rs/tauri-plugin-fs/latest/tauri_plugin_fs/
- [S11] https://docs.rs/sysinfo/latest/sysinfo/struct.Disk.html
- [S12] https://v2.tauri.app/plugin/updater/
- [S13] https://v2.tauri.app/plugin/sql/
- [S14] https://github.com/pdeljanov/Symphonia
- [S15] https://docs.rs/symphonia/0.6.1/symphonia/
- [S16] https://www.construct.net/en/blogs/ashleys-blog-2/edge-supports-open-media-953
- [S17] https://docs.rs/symphonia-core/latest/symphonia_core/codecs/audio/struct.AudioDecoderOptions.html
- [S18] https://docs.rs/symphonia/^0.5
- [S19] https://docs.rs/symphonia-core/latest/symphonia_core/formats/struct.FormatOptions.html
- [S20] https://docs.rs/symphonia-core/latest/symphonia_core/formats/trait.FormatReader.html
- [S21] https://docs.rs/crate/ffmpeg-sys-next/9.0.0/source/README.md
- [S22] https://docs.rs/cpal/latest/cpal/
- [S23] https://docs.rs/rodio/latest/rodio/
- [S24] https://docs.rs/rodio/latest/rodio/source/trait.Source.html
- [S25] https://docs.rs/kira/latest/kira/
- [S26] https://v2.tauri.app/reference/config/
- [S27] https://docs.rs/tauri/latest/tauri/async_runtime/index.html
- [S28] https://v2.tauri.app/develop/calling-rust/
- [S29] https://v2.tauri.app/plugin/file-system/
- [S30] https://docs.rs/tauri-plugin-fs/latest/tauri_plugin_fs/trait.FsExt.html
- [S31] https://docs.rs/tauri/latest/tauri/scope/fs/struct.Scope.html
- [S32] https://docs.rs/tauri/latest/tauri/trait.Manager.html
- [S33] https://v2.tauri.app/reference/javascript/dialog/
- [S34] https://v2.tauri.app/plugin/dialog/
- [S35] https://docs.rs/windows-sys/latest/windows_sys/Win32/Storage/FileSystem/fn.GetDriveTypeW.html
- [S36] https://api.codeproject.com/Articles/375916/How-to-Prepare-a-USB-Drive-for-Safe-Removal
- [S37] https://developer.apple.com/documentation/xcode/resetting-access-to-protected-resources-in-macos.md (plus https://forum.xojo.com/t/app-would-like-to-access-files-dialog/51934)
- [S38] https://v2.tauri.app/distribute/macos-application-bundle/
- [S39] https://github.com/mattchine/tauri-plugin-sql
- [S40] https://docs.rs/tauri/latest/tauri/path/struct.PathResolver.html
- [S41] https://v2.tauri.app/distribute/sign/macos/
- [S42] https://developer.apple.com/programs/
- [S43] https://developer.apple.com/documentation/security/hardened-runtime (body did not load)
- [S44] https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution
- [S45] https://developer.apple.com/news/?id=saqachfa
- [S46] https://support.apple.com/en-us/102445
- [S47] https://macos-tahoe.com/blog/app-is-damaged-cant-be-opened-macos-tahoe-gatekeeper-quarantine-fix-2026/ (third-party)
- [S48] https://v2.tauri.app/distribute/pipelines/github/
- [S49] https://v2.tauri.app/distribute/sign/windows/
- [S50] https://learn.microsoft.com/en-us/azure/artifact-signing/faq
- [S51] https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart
- [S52] https://azure.microsoft.com/en-us/pricing/details/artifact-signing/
- [S53] https://techcommunity.microsoft.com/blog/microsoftsecurityandcompliance/-/4273554
- [S54] https://signpath.org/
- [S55] https://signpath.org/terms
- [S56] https://shop.certum.eu/code-signing.html
- [S57] https://v2.tauri.app/distribute/appimage/
- [S58] https://v2.tauri.app/start/prerequisites/
- [S59] https://github.com/tauri-apps/tauri-action
- [S60] https://docs.rs/tauri/latest/tauri/ipc/struct.Channel.html
- [S61] https://v2.tauri.app/develop/calling-frontend/
- [S62] https://v2.tauri.app/reference/webview-versions/
- [S63] https://forum.manjaro.org/t/webkit2gtk-doesnt-work-well-with-m3u8-streams/168103
