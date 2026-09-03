# QQ Music Backend Design

## Goal

`glacier-player` is a COSMIC panel applet or standalone window that uses
QQMusicApi as its catalog, authentication, lyrics, and stream-URL service.
The application must not depend on WQM or a legacy service SDK. QQMusicApi is
the only catalog, authentication, lyrics, and stream provider.

The playback engine remains local to glacier-player:

```text
glacier-player (Rust/COSMIC, sidecar lifecycle owner)
        |
        | HTTP + JSON + QQ credential cookies
        v
glacier-qqmusic-api (bundled QQMusicApi executable)
        |
        v
QQ Music
```

## QQMusicApi Contract

The bundled service listens on `http://127.0.0.1:8080`. Before the first API
request, the Rust client probes that endpoint and starts the sibling
`glacier-qqmusic-api` executable when needed. It reuses an already healthy
service, retries once after a connection loss, and terminates a child process
that it owns when the application exits. The base URL remains a user setting
so a remote service can be used without rebuilding the app.

Every response uses this envelope:

```json
{"code": 0, "msg": "ok", "data": {}}
```

`code == 0` is success. Non-zero codes and HTTP errors are normalized into the
client error type. Authenticated requests send QQ credentials as cookies;
`musicid` and `musickey` are required, while the other credential fields are
forwarded when available.

Core routes used by the first implementation:

| Capability | Route |
| --- | --- |
| Search tracks | `GET /search/search_by_type?keyword=...&search_type=0&num=...&page=...` |
| Song detail | `GET /song/{id-or-mid}/detail` |
| Song stream URL | `GET /song/{mid}/url?file_type=...&song_type=...&media_mid=...` |
| Lyrics | `GET /song/{id-or-mid}/lyric?trans=true` |
| Playlist detail | `GET /songlist/{id}/detail` |
| User playlists | `GET /user/{uin}/created_songlists` and `/user/{euin}/fav/songlists` |
| Favorite songs and albums | `GET /user/{euin}/fav/songs` and `/user/{euin}/fav/albums` |
| Album detail and songs | `GET /album/{mid}/detail` and `/album/{mid}/songs` |
| Singer detail | `GET /singer/{mid}/info`, `/songs`, `/albums`, and `/similar` |
| QR login | `GET /login/qrcode/{login_type}` and `/login/qrcode/{login_type}/status` |
| Session check | `GET /login/check_expired` |
| Session refresh | `GET /login/refresh_credential` |

The complete service route set also exposes recommendations, top charts, MV
URLs, and additional account data. Each is enabled in the player only after
its Web contract and UI behavior have been tested.

## Client Boundary

`QQMusicClient` owns only HTTP concerns:

- base URL and timeout handling;
- credential cookie construction and persistence payloads;
- response envelope validation and error normalization;
- conversion from QQMusicApi JSON to provider-neutral domain models;
- stream URL resolution and expiry metadata.

`QqMusicSidecar` owns only the bundled process lifecycle, health checks, and
its local device-data directory. Custom non-default endpoints bypass sidecar
management.

The client does not own the queue, GStreamer, COSMIC view state, or MPRIS.
`AppModel` remains the owner of those concerns.

Domain models must not expose QQMusicApi response objects. IDs are strings so
QQ numeric IDs and MIDs can coexist. A QQ stream is represented as a direct
URL.

## Authentication

The login view uses QQ's QR flow:

1. request a QR image and identifier;
2. display the QR image;
3. poll the QR status route until confirmed, expired, or refused;
4. persist the returned credential and refresh it through the API when needed.

The app must never log credential cookies or stream URLs.

## Playback

The existing GStreamer `MediaPlayer` stays in place. A successful QQ URL
resolution returns a direct `http(s)` URI and starts the same audio pipeline.
Quality mapping is:

| UI setting | QQ request |
| --- | --- |
| Low | `13` (`MP3_128`) |
| High | `12` (`MP3_320`) |
| Lossless | `7` (`FLAC`) |
| Hi-Res | `1` (`MASTER`) |

The playback URL is short-lived. The client records its expiry and playback
handlers resolve a fresh URL when starting or retrying a track. Queue advance,
seek, volume, gapless staging, and MPRIS continue to be local behaviors.

## Feature Policy

The first usable QQ build supports QR login, track search, song/album/singer
detail, user/public playlists, favorite-song and favorite-album reads, lyrics,
direct playback, queue controls, artwork, history, and MPRIS.

Provider features without a QQMusicApi equivalent are not emulated:

- Mixes, Explore, Feed, and provider play attribution are removed from the
  primary navigation until QQMusicApi equivalents are mapped;
- credits are hidden until QQMusicApi exposes an equivalent;
- favorite and follow writes stay hidden until stable Web routes are available;
- video playback stays disabled until QQ MV URL mapping is tested;
- sharing uses QQ song/album URLs directly.

## Migration Stages

1. Add the API base URL and provider-neutral HTTP/error primitives.
2. Implement and test `QQMusicClient` for envelope parsing, credentials,
   search, song detail, lyrics, playlists, and stream URLs.
3. Replace the legacy client ownership in `AppModel` and data handlers.
4. Replace OAuth messages/views with QR login and polling.
5. Remove legacy handlers, play reporter, dependencies, and copy.
6. Bundle QQMusicApi as a self-contained sidecar and manage it on demand.
7. Run unit tests, offline HTTP contract tests, and both applet/standalone
   builds.

No WQM process, socket, command protocol, system Python installation, or
user-managed daemon is part of this design. The frozen Python runtime is an
implementation detail of the bundled `glacier-qqmusic-api` executable.

## Build Dependency Policy

The normal build keeps `libcosmic` managed by Cargo from its upstream Git
repository. `thirdparty/libcosmic` is a checked-out fallback for offline or
credential-constrained builds only; it is not the default dependency source.
Cargo has no transparent remote-then-path fallback, so selecting the local
copy is an explicit build configuration change rather than an automatic
runtime behavior.

The repository pins Rust `1.96.0` in `rust-toolchain.toml`. This is required by
the current remote dependency graph (notably `kstring` pulled by libcosmic).
The sidecar build pins QQMusicApi commit `108617f`, PyInstaller `6.16.0`, and
PyInstaller hooks `2026.7`. Release builds run natively on x86_64 and aarch64,
include the upstream license, and attach the exact corresponding source
archive. The QQMusicApi endpoint remains configurable at runtime; only the
default `http://127.0.0.1:8080` endpoint is managed automatically.

## Implementation Status

The current fork has completed the HTTP client, envelope/error handling,
credential cookies and keyring persistence, QR login polling, API URL setting,
track-search, playlist, album, singer, and favorites-read conversion, lyrics
conversion, and direct stream URL resolution. `AppModel` now owns
`QqMusicAppClient`; its existing queue, GStreamer pipeline, artwork cache,
history, and MPRIS paths are reused.
Official packages include the backend sidecar, so installed users do not need
to provision or start QQMusicApi separately.
The former service module, client dependency, redirect handler, and provider
specific tests have been removed. The UI branding and login flow now use
Glacier Player and QQ Music terminology throughout.

Mixes, Explore, Feed, favorites/follow writes, credits, and videos return empty
or unsupported results until QQMusicApi routes for those features are mapped
and tested. They do not silently fall back to WQM or another provider.
