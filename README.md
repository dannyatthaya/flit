# Flit

> Fast, private YouTube Music desktop client — Tauri 2 + Svelte. Native webview, tiny RAM footprint, tray mini-player. No Electron, no telemetry.

Flit wraps **music.youtube.com** in a native OS webview (via [Tauri 2](https://v2.tauri.app/)) and adds the things the web app lacks on the desktop: a system-tray mini-player, minimize-to-tray, single-instance handling, and auto-updates — while staying small, fast, and private.

> **Status: early development.** The core shell works (native window, tray, minimize-to-tray, single-instance) and the tray mini-player drives playback. See [Roadmap](#roadmap).

## Why Flit

- **Lightweight & fast.** Uses the OS's native webview — no bundled Chromium, no Electron. The goal is for the app's own overhead (excluding the YouTube Music page) to stay in the low tens of MB, with state polling that backs off to ~1 Hz only while something is playing or a window is visible.
- **Private by design.** Zero analytics, zero telemetry, zero crash-reporting. No backend, no account system, no OAuth. The app only ever contacts **YouTube/Google** (the music page + album art) and **GitHub** (for updates). All settings stay local.
- **Least privilege.** The remote YouTube Music origin is granted only Tauri's `core:default` — no custom commands, no clipboard, no filesystem access.

## Features

- 🎵 YouTube Music in a native window (your login persists, isolated from your system browser)
- 🖥️ Minimize-to-tray instead of quitting; single-instance enforcement
- 🎛️ System-tray mini-player: play/pause, next/previous, seek, volume, queue, like/shuffle/repeat
- 🚀 Auto-update from GitHub Releases *(in progress)*
- ⚙️ Optional launch-at-startup
- 🌍 Cross-platform: Windows and Linux (macOS kept buildable)

## Privacy & performance

Flit is built so these claims stay true to the code:

- **No telemetry / analytics / crash-reporting** of any kind.
- **No app-operated backend.** Network traffic only goes to YouTube/Google (music + `i.ytimg.com` album art, handled by the webview) and `github.com` (the updater).
- **Local data only:** small user-settings files in the OS app-data directory. No tracking IDs, no fingerprint.
- **Native webview** keeps the memory footprint far below a typical Electron music app.

## Build from source

**Requirements**
- [Rust](https://www.rust-lang.org/tools/install) 1.70+ and Cargo
- [Node.js](https://nodejs.org/) 18+ and [Bun](https://bun.sh/) (this project uses Bun as its package manager)
- **Linux only:** `libwebkit2gtk-4.1-dev` and `libayatana-appindicator3-dev` (plus the usual build tooling)

```bash
# install JS dependencies
bun install

# run in development
bun run tauri dev

# produce a release build / installers
bun run tauri build
```

## Auto-update

Releases are published from GitHub Releases and verified with a [minisign](https://jedisct1.github.io/minisign/) key. Generate your signing keypair with Tauri's built-in signer:

```bash
bun run tauri signer generate -w "$HOME/.tauri/flit.key"
```

Put the **public** key in `tauri.conf.json` (`plugins.updater.pubkey`) and keep the **private** key secret (it's git-ignored). For CI signing, set the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets.

## Releasing

Two paths:

- **Full multi-OS release (recommended):** push a `vX.Y.Z` tag — `.github/workflows/publish.yml` builds Windows, Linux, and macOS, signs them with the CI secrets, and drafts a GitHub Release with the updater `latest.json`.
- **Local (Windows) release from `.env`:** `pwsh scripts/release.ps1 -Version X.Y.Z -Publish` reads the signing key/password from a local `.env`, builds a signed installer, and drafts a release with `latest.json`. (The auto-updater only serves **published**, non-draft releases.)

> **CSP:** the app ships with `app.security.csp = null` because the `main` window loads the remote YouTube Music site, whose own scripts/styles would be blocked by a strict policy. The remote origin is still confined to `core:default` (no custom commands/clipboard/filesystem), and the only trusted local surface is the popup.

## Roadmap

- [x] **Phase 0** — Scaffold (Tauri 2 + SvelteKit, plugins, release profile)
- [x] **Phase 1** — Core shell (YTM window, minimize-to-tray, single-instance, tray menu)
- [x] **Phase 2** — Injection bridge (`window.__flit__` player control + `flit-state` events)
- [x] **Phase 3** — Tray popup UI (seekable controls, queue, like/shuffle/repeat state)
- [x] **Phase 5** — Settings, persistence, autostart
- [x] **Phase 6** — Updater config + CI + docs
- [x] **Phase 7** — Verification (privacy/performance audit)

> Discord Rich Presence was considered (Phase 4) but **dropped** to keep the app lean.

## License

MIT
