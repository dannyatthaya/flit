# Changelog

The publish workflow copies the section for the tag being released into the GitHub release notes.

## Unreleased

### Improved

- Lower background CPU use: while neither the window nor the mini-player is on screen, Flit no longer polls the player; it reacts to play/pause/track changes and refreshes as soon as you open the mini-player.
- The queue is only re-read when it changes, instead of every second.
- The mini-player's webview is created the first time you open it and closed after 10 minutes hidden, saving a web renderer's worth of memory.
- Windows: the YouTube Music webview is asked to trim its memory while the window is hidden or minimized.
- Updates are downloaded when you click **Update** instead of in advance, so a pending update no longer sits in memory (up to ~100 MB for the Linux AppImage).
- The app is about 0.4 MB smaller: icon source files are no longer bundled.

## 0.2.0

> **Updating from 0.1.0:** 0.1.0 never checks for updates, so download and install 0.2.0 from this release page once. From 0.2.0 on, Flit updates itself.

### New

- **Auto-update.** Flit checks GitHub Releases in the background, downloads a signed update and offers **Restart to update** in the mini-player. Nothing installs until you click it. You can also check manually in the mini-player's settings.
- **Pin the mini-player** to keep it open; otherwise it hides when you click elsewhere.
- **Links that open a new tab** (help pages, artist sites) open in your default browser.
- **Back button:** if the window leaves YouTube Music (for example during Google sign-in), a **← YouTube Music** button takes you back.
- **Link box** in the mini-player's settings accepts youtube.com, youtu.be, Shorts and links without `https://`.

### Improved

- The mini-player stays in sync (progress, play/pause, track changes) while the YouTube Music window is hidden in the tray.
- Volume in the mini-player matches YouTube Music's own volume.
- Queue: tapping a track plays that track, even if the queue changed in the meantime.
- The mini-player opens next to the tray and grows away from the taskbar; Alt+F4 no longer breaks it.
- Less CPU and memory use: state is only sent when it changes, and album art only once per track.
- macOS: clicking the Dock icon brings the window back.

### Fixed

- **Like** could press **Dislike**, and **Shuffle**/**Repeat** could press buttons elsewhere on the page.
- Repeat mode was sometimes shown wrongly.
- Launch at startup was switched on without asking. It's now off until you turn it on, and starts Flit in the tray.

### Security

- The YouTube Music page can no longer ask Flit to open arbitrary files or links on your computer.
- The mini-player only accepts validated data from the page and runs under a strict content security policy.
