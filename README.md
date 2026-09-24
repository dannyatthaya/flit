# Flit

> Fast, private YouTube Music desktop client — Tauri 2 + Svelte. Native webview, tiny RAM footprint, tray mini-player. No Electron, no telemetry.

Flit wraps **music.youtube.com** in a native OS webview (via [Tauri 2](https://v2.tauri.app/)) and adds the things the web app lacks on the desktop: a system-tray mini-player, minimize-to-tray, single-instance handling, and auto-updates — while staying small, fast, and private.

> **Status: early development.** The core shell works (native window, tray, minimize-to-tray, single-instance) and the tray mini-player drives playback. See [Roadmap](#roadmap).

## Why Flit

- **Lightweight & fast.** Uses the OS's native webview — no bundled Chromium, no Electron. The goal is for the app's own overhead (excluding the YouTube Music page) to stay in the low tens of MB. Player state is sampled every 1 s while playing, every 2 s while paused, and every 5 s while the YouTube Music window is hidden. It is only sent when something changed, the album art and queue only when they change, and nothing is forwarded to the mini-player while it is closed.
- **Private by design.** Zero analytics, zero telemetry, zero crash-reporting. No backend, no account system, no OAuth. The app only ever contacts **YouTube/Google** (the music page + album art) and **GitHub** (for updates). All settings stay local.
- **Least privilege.** The remote YouTube Music origin may only emit events (`core:event:allow-emit`) — no custom commands, no window APIs, no clipboard, no filesystem access. Everything it reports is validated in Rust before it reaches the mini-player.

## Features

- 🎵 YouTube Music in a native window (your login persists, isolated from your system browser)
- 🖥️ Minimize-to-tray instead of quitting; single-instance enforcement
- 🎛️ System-tray mini-player: play/pause, next/previous, seek, volume, queue, like/shuffle/repeat. It hides when you click elsewhere; pin it to keep it open
- 🔗 Links that open a new tab (help pages, artist sites) open in your default browser; if the window ever leaves YouTube Music (for example during Google sign-in), a **← YouTube Music** button takes you back
- 🚀 Auto-update from GitHub Releases (signed, one-click restart)
- ⚙️ Optional launch-at-startup (off by default; starts in the tray)
- 🌍 Cross-platform: Windows and Linux (macOS kept buildable)

> **Linux tray note:** most Linux tray hosts don't report clicks on the icon, so open the mini-player from the tray icon's menu (**Open widget**) or the small button Flit adds to the YouTube Music page. GNOME needs the AppIndicator extension to show the tray icon at all.

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

**Checks** (the same ones CI runs on every pull request):

```bash
bun run check && bun run build
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Auto-update

Release builds check `latest.json` on GitHub Releases 15 seconds after launch and then every 6 hours. When a newer version exists, Flit downloads it in the background and verifies its [minisign](https://jedisct1.github.io/minisign/) signature against the public key in `tauri.conf.json`. It then shows **Restart to update** in the mini-player, plus a notice in the YouTube Music window. Nothing is installed until you click it. You can also check manually under the mini-player's settings. Development builds never check automatically.

On Linux, in-place updates work for the AppImage and .deb/.rpm bundles; other installs should update through their package source.

Generate your signing keypair with Tauri's built-in signer:

```bash
bun run tauri signer generate -w "$HOME/.tauri/flit.key"
```

Put the **public** key in `tauri.conf.json` (`plugins.updater.pubkey`) and keep the **private** key secret (it's git-ignored). For CI signing, set the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets.

## Releasing

Two paths:

- **Full multi-OS release (recommended):** push a `vX.Y.Z` tag — `.github/workflows/publish.yml` builds Windows, Linux, and macOS, signs them with the CI secrets, and drafts a GitHub Release with the updater `latest.json`.
- **Local (Windows) release from `.env`:** `pwsh scripts/release.ps1 -Version X.Y.Z -Publish` reads the signing key/password from a local `.env`, builds a signed installer, and drafts a release with `latest.json`. (The auto-updater only serves **published**, non-draft releases.) Its `latest.json` lists Windows only, so once it's published, Linux and macOS installs find no build for their platform and stay on their current version until the next full release from CI.

> **CSP:** Tauri's `app.security.csp` applies only to the app's own bundled pages — here, the mini-player popup — and never to the remote YouTube Music site. The popup ships with a strict policy: scripts from the bundle only, images only from the bundle, `data:` and YouTube/Google image hosts, and no network access besides Tauri IPC. The remote origin can only emit events, and the popup receives player state over an IPC channel that the remote page cannot write to.

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
