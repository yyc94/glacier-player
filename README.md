# <img src="resources/icon.svg" width="36" align="absmiddle" /> Glacier Player

[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/yyc94/glacier-player/badge)](https://scorecard.dev/viewer/?uri=github.com/yyc94/glacier-player)
[![Codecov](https://codecov.io/gh/yyc94/glacier-player/graph/badge.svg)](https://codecov.io/gh/yyc94/glacier-player)
[![GitHub Release](https://img.shields.io/github/release/yyc94/glacier-player.svg)](https://github.com/yyc94/glacier-player/releases/latest)
![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)
![GitHub Repo stars](https://img.shields.io/github/stars/yyc94/glacier-player)

A COSMIC™ desktop application for the QQ Music music streaming service.
Stream audio, browse your library and artist catalog, and control playback
with a real-time spectrum visualizer and full MPRIS integration.
Official packages include and automatically manage the QQ Music backend; no
separate Python service or container is required.

Builds as either a **panel applet** (popup from the system panel) or a
**standalone window** (regular application) — chosen at compile time
via the `panel-applet` feature flag (enabled by default).

<table align="center">
<tr>
<td align="center"><img src="resources/screenshot_applet.png" alt="Panel applet popup" width="320" /></td>
<td align="center"><img src="resources/screenshot_standalone.png" alt="Standalone mode window" width="600" /></td>
</tr>
<tr>
<td align="center"><sub>Panel Applet mode</sub></td>
<td align="center"><sub>Standalone Window mode</sub></td>
</tr>
</table>

## Features

- **Audio Playback** — Select MP3, FLAC, or Hi-Res quality and play the best
  source available to the signed-in QQ Music account through GStreamer
  (PipeWire/PulseAudio output)
- **Gapless Playback** — The next track is preloaded and decoded ahead
  of time for seamless, gap-free transitions
- **Real-time Spectrum Visualizer** — FFT-based stereo frequency
  display in the now-playing bar, driven by a PCM tap on the audio pipeline
- **MPRIS D-Bus Integration** — Control playback from any MPRIS client
  (playerctl, KDE Connect, desktop media keys, etc.)
- **Library Browsing** — Playlists, albums, favorite tracks, and local history
- **Track Search** — Search QQ Music's song catalog
- **Artist Browsing** — Open an artist's top tracks, albums, and related artists
- **Lyrics** — Time-synced lyrics that highlight the current line (with
  a plain-text fallback when only flat lyrics are available)
- **Play History** — A locally-tracked, searchable list of recently
  played tracks
- **Favorites** — View available account playlists and saved data
- **Shuffle** — Shuffle play for playlists, albums, and favorites
- **Sharing** — Open QQ Music song and album pages or copy them to clipboard
- **Dual Mode** — Builds as a COSMIC panel applet *or* a standalone
  windowed application (`--no-default-features`)
- **Secure Authentication** — QQ or WeChat QR sign-in, with credentials stored in
  the system keyring
- **Persistent Sessions** — Credential refresh across reboots when supported
- **Disk Caching** — Artwork is cached on disk with a configurable size
  limit and LRU eviction; library data (playlists, albums, history,
  lyrics) is cached in an embedded database for instant startup
- **Audio Quality Selection** — Low, High, Lossless, or Hi-Res

## Installation

### Dependencies

Install the required system libraries before building:

```sh
# Fedora / RHEL
sudo dnf install dbus-devel libsecret-devel libxkbcommon-devel \
    gstreamer1-devel gstreamer1-plugins-base-devel

# Ubuntu / Debian
sudo apt install libdbus-1-dev libsecret-1-dev libxkbcommon-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev

# Arch
sudo pacman -S dbus libsecret libxkbcommon gstreamer gst-plugins-base
```

#### Playback codecs (runtime)

Audio playback runs through GStreamer. Install the good/bad/libav plugin sets
to provide the FLAC/AAC/MP3 demuxers and decoders used by QQ Music streams.

```sh
# Fedora / RHEL (avdec_* come from RPM Fusion's gstreamer1-libav)
sudo dnf install gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-libav

# Ubuntu / Debian
sudo apt install gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav

# Arch
sudo pacman -S gst-plugins-good gst-plugins-bad gst-libav
```

### Bundled QQ Music backend

Glacier Player uses the
[QQMusicApi](https://github.com/L-1124/QQMusicApi) Web API internally. Official
Official amd64 DEB packages bundle it as `glacier-qqmusic-api`; the player
starts it on demand, stores its device data under the user's local data
directory, and stops the process when it exits. Users do not install Python or
start a daemon.

A custom remote endpoint can still be selected in Settings for development or
self-hosting. The default `http://127.0.0.1:8080` endpoint is managed by the
application.

### Build & Install

Requires Rust 2024 edition (the repository pins 1.96.0),
[just](https://github.com/casey/just), and
[uv](https://docs.astral.sh/uv/) when building the bundled backend from source.

```sh
git clone https://github.com/yyc94/glacier-player.git
cd glacier-player

# Panel applet (default — installs into the COSMIC panel)
just build-release
just install

# Standalone window application
just build-release-standalone
just install-standalone
```

## Usage

### First-time Setup

1. Click the Glacier Player icon in your panel (or launch the standalone app)
2. Click **Sign in with QQ Music**
3. Scan the displayed QR code in the QQ Music app and wait for confirmation

### Browsing & Playback

- **Collection** — View your playlists, albums, favorite tracks, and local
  history from the main screen
- **Search** — Tap the search icon to find tracks
- **History** — Revisit recently played tracks (searchable)
- **Lyrics** — Open the lyrics view for the current track; synced
  lyrics highlight the active line
- **Now Playing** — Playback controls, seek bar, shuffle, and spectrum
  visualizer
- **MPRIS** — Use media keys or any MPRIS controller (e.g.
  `playerctl play-pause`)
- **Sharing** — Open or copy the QQ Music page for the current track
- **Settings** — Audio quality and account info via
  the gear icon

## Configuration

Configuration is managed through COSMIC's config system. Available
settings:

| Setting | Description | Default |
|---|---|---|
| Audio Quality | Low / High / Lossless / Hi-Res | Hi-Res |
| QQ Music API URL | Advanced backend override; the default is managed automatically | `http://127.0.0.1:8080` |
| Image Cache Limit | Max disk space for cached artwork | 200 MB |

The playback volume is also persisted across restarts.

## Building

A [justfile](./justfile) provides all common workflows:

| Recipe | Description |
|---|---|
| `just` | Build applet with release profile (default) |
| `just build-release` | Build applet with release profile |
| `just build-sidecar` | Build the self-contained QQ Music backend |
| `just build-debug` | Build applet with debug profile |
| `just build-release-standalone` | Build standalone window app (release) |
| `just build-debug-standalone` | Build standalone window app (debug) |
| `just build-vendored` | Build applet with vendored dependencies |
| `just build-vendored-standalone` | Build standalone with vendored dependencies |
| `just run` | Build and run applet (`RUST_BACKTRACE=full`) |
| `just run-debug` | Build and run applet (debug profile) |
| `just run-standalone` | Build and run standalone (release) |
| `just run-standalone-debug` | Build and run standalone (debug) |
| `just check` | Clippy lint check |
| `just test` | Run tests with cargo-nextest (falls back to cargo test) |
| `just test-verbose` | Tests with immediate stdout/stderr output |
| `just coverage` | HTML + LCOV coverage report via cargo-llvm-cov |
| `just coverage-summary` | Text-only coverage summary |
| `just doc` | Build docs (including private items) |
| `just doc-open` | Build docs and open in browser |
| `just bloat-check` | Analyze binary size by crate/function |
| `just stats` | Code statistics via tokei |
| `just install` | Install applet system-wide |
| `just install-debug` | Install applet (debug build) |
| `just install-standalone` | Install standalone app system-wide |
| `just install-standalone-debug` | Install standalone app (debug build) |
| `just uninstall` | Remove installed files |
| `just clean` | `cargo clean` |
| `just clean-dist` | Clean build artifacts and vendored deps |
| `just vendor` | Vendor dependencies for offline builds |
| `just tag <version>` | Bump version, commit, and create git tag |

### Viewing logs

The panel applet logs to **stderr**, which `cosmic-panel` forwards to the
systemd journal tagged `io.github.cosmic-applet-mare:`. Follow them live with:

```sh
journalctl --user _COMM=cosmic-panel -f | grep --line-buffered cosmic-applet-mare
```

Adjust verbosity at runtime from **Settings → Logging** (Error … Trace); the
change applies immediately, no restart.

## Project Structure

```
src/
├── playback/       # GStreamer playback engine, volume/seek controls, PCM spectrum tap, and gapless staging
├── audio/          # FFT spectrum analyzer (fed by the playback PCM tap)
├── music/          # Provider-neutral models, queue state, and MPRIS2 D-Bus interface
├── handlers/       # Message handlers: auth, data loading, navigation, playback, misc (images, sharing, MPRIS, screenshots)
├── views/          # UI views
│   ├── components/ # Reusable components: FadingClip widget, icons, constants, list helpers, row builders
│   ├── visualizer  # Audio spectrum visualizer widget
│   └── *.rs        # Panel, popup, albums, artists, playlists, tracks, track detail, mixes, search, explore, feed, history, lyrics, profiles, settings, auth, share, …
└── *.rs            # App model, state, messages, config, disk caching, image cache, helpers, menu
scripts/
└── build-qqmusic-sidecar.sh # Reproducible build for the bundled backend
```

## Key Dependencies

| Crate | Purpose |
|---|---|
| [libcosmic](https://github.com/pop-os/libcosmic) | COSMIC application framework |
| [QQMusicApi](https://github.com/L-1124/QQMusicApi) | Bundled QQ Music API backend |
| [gstreamer-rs](https://gitlab.freedesktop.org/gstreamer/gstreamer-rs) | Audio playback engine (decode, stream, output, volume, seek, gapless) |
| [rustfft](https://crates.io/crates/rustfft) | FFT for spectrum analysis |
| [zbus](https://crates.io/crates/zbus) | D-Bus / MPRIS2 interface |
| [keyring](https://crates.io/crates/keyring) | System credential storage |
| [reqwest](https://crates.io/crates/reqwest) | HTTP client for API requests |
| [image](https://crates.io/crates/image) | Decoding and processing artwork |

## Acknowledgments

- Built with [libcosmic](https://github.com/pop-os/libcosmic)
- QQ Music API access via [QQMusicApi](https://github.com/L-1124/QQMusicApi)

## Contributing

Bug reports, translations and patches are welcome — see
[CONTRIBUTING.md](./CONTRIBUTING.md) for the setup, the checks CI gates on,
and the commit-message and dependency-licence rules.

## License

Glacier Player is [GPL-3.0-only](LICENSE). The bundled QQMusicApi backend is
[GPL-3.0-or-later](resources/QQMusicApi.LICENSE); release assets include the
exact corresponding source archive.

## Disclaimer

QQ Music is a service of Tencent Music Entertainment Group. This is an
unofficial application and is not affiliated with or endorsed by Tencent or
QQ Music. Use it in accordance with QQ Music's terms and applicable law.
