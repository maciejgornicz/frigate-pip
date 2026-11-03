# Contributing to FrigatePiP

Thank you for helping! Bug reports, translations, documentation fixes and code are all welcome. By contributing you agree that your contribution is licensed under the [GNU GPL v3.0](LICENSE), like the rest of the project.

## Reporting a bug or asking for a feature

Open an [issue](https://github.com/maciejgornicz/frigate-pip/issues) and include:

- the FrigatePiP version (first line of the log: `FrigatePiP x.y.z - start`), the Frigate version and your system: the Windows version, or for the Linux console version the distribution and the output (`framebuffer`, `terminal` or `tmux`);
- what you did, what you expected and what happened;
- the relevant part of the log — Windows: **tray menu → Show log** or `%APPDATA%\FrigatePiP\frigate-pip.log`; Linux: `~/.config/frigate-pip/frigate-pip.log` — and, for connection problems, the output of **Test connection** (Linux: `frigate-pip --test-connection`).

The log contains your Frigate address and camera names — remove anything you do not want to share. Please check existing issues first.

## Development setup

- [Rust](https://rustup.rs) stable, 1.87 or later.
- **Windows application:** Windows 10 or 11, 64-bit, with either the MSVC (`x86_64-pc-windows-msvc`, default) or the GNU (`x86_64-pc-windows-gnu`) toolchain. Its dependencies are the [`windows`](https://crates.io/crates/windows) crate and [`schannel`](https://crates.io/crates/schannel) for HTTPS (the system's TLS); no C compiler is needed. Both stay on versions that link through import libraries, so the GNU toolchain needs no `dlltool`. Optional: [Inno Setup 6](https://jrsoftware.org/isinfo.php) to build the installer.
- **Linux version:** Linux (or WSL on Windows) with `ffmpeg`; its dependencies are the `libc` and `fontdue` crates and, for HTTPS, `rustls` with `ring` and `webpki-roots`; `ring` needs a C compiler (`gcc`, e.g. from `build-essential`). Optional: `musl-tools` for a static binary and [cargo-deb](https://crates.io/crates/cargo-deb) for the Debian package — see [Testing the Linux version](#testing-the-linux-version).
- Optional: a Frigate server with go2rtc to try things for real.

The Windows application:

```
cargo build --release          # target\release\frigate-pip.exe
cargo test --release           # unit tests, no network or Frigate needed
cargo fmt                      # formatting (settings in rustfmt.toml)
cargo clippy --release --all-targets
```

Run the freshly built exe directly. Only one copy runs at a time, so exit the installed FrigatePiP first (`frigate-pip.exe --quit`); the development build uses the same settings file in `%APPDATA%\FrigatePiP`.

### Tests that need real hardware or a Frigate server

Some tests are marked `#[ignore]`: they need Media Foundation decoders (Windows) or a live go2rtc server. Point them at your own server with environment variables (Windows `cmd` syntax; on Linux use `export`):

```
set FRIGATE_RTSP=rtsp://192.168.1.10:8554
set FRIGATE_STREAMS=front_door_sub,garden
cargo test --release -- --ignored --nocapture
```

`FRIGATE_STREAMS` is a comma-separated list of go2rtc stream names. Without these variables the live tests print a hint and pass. Never commit real addresses, camera names, credentials or footage.

`live_connection` runs the whole **Test connection** against a Frigate server — set `FRIGATE_URL` (e.g. `https://192.168.1.10:8971`) and, for port 8971, `FRIGATE_USER` and `FRIGATE_PASSWORD` (a user with the viewer role is enough); `FRIGATE_RTSP_TEMPLATE` replaces `rtsp://{host}:8554/{stream}`. A self-signed certificate is remembered in `certificates.txt` in the settings folder, so set `APPDATA` (Windows) or `XDG_CONFIG_HOME` (Linux) to a scratch folder to keep your own settings folder untouched.

### Testing Windows-only behaviour

To try the behaviour of Windows "N" editions without Media Foundation, start the app with the environment variable `FRIGATEPIP_NO_MF=1`: live video then falls back to MJPEG.

To try the update path, start the installed app with `FRIGATEPIP_FAKE_VERSION=1.0.0`: it then treats the latest GitHub release as newer and offers to install it (**Settings → Connection → Check now**).

### Testing the Linux version

Build and test on Linux (WSL works well: it has a terminal with Sixel in Windows Terminal, and X11/Wayland/PulseAudio through WSLg):

```
cargo build --release
./target/release/frigate-pip --output terminal --test front_door
```

There is no framebuffer in WSL, but a file can stand in for one; the screen can then be turned into a picture with ffmpeg:

```
touch /tmp/fakefb
./target/release/frigate-pip --output framebuffer --framebuffer /tmp/fakefb --framebuffer-size 1280x720x32 --test front_door
ffmpeg -f rawvideo -pix_fmt bgra -s 1280x720 -i /tmp/fakefb screen.png
```

Note that the window's background is put back when FrigatePiP exits, so copy the file while it runs. `FRIGATEPIP_FFMPEG_LOG=/tmp/ff.log` collects ffmpeg's messages; `FRIGATEPIP_AUDIO_OUT=wav:/tmp/a.wav` records the camera audio to a file instead of playing it.

Desktop windows can be tested without a desktop on a virtual X server: `Xvfb :5 -screen 0 1600x900x24 -ac &`, then `DISPLAY=:5 ./target/release/frigate-pip --output window --test front_door`; a picture of the screen: `ffmpeg -f x11grab -video_size 1600x900 -i :5 -frames:v 1 screen.png`. Under WSLg the windows are created in the right place, but their picture does not appear on the Windows screen, so use Xvfb there.

The tray icon can be checked without a panel: start a session bus with `dbus-launch`, run the desktop windows with that `DBUS_SESSION_BUS_ADDRESS`, and call them like a panel does, e.g. `busctl --user call <name> /MenuBar com.canonical.dbusmenu GetLayout iias 0 10 0` or `… Event isvu 53 clicked i 0 0`.

tmux popups need a tmux client: start tmux in a terminal, then `./target/release/frigate-pip --output tmux --test front_door` from another tmux window.

The static binary and the Debian package, as CI builds them (the package's contents are set in `[package.metadata.deb]` in `Cargo.toml` and in `packaging/`):

```
cargo build --release --target x86_64-unknown-linux-musl
cargo deb --target x86_64-unknown-linux-musl --no-build
```

### Building the Windows installer

```
cargo build --release
"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" /DAppVersion=1.0.0 installer\frigate-pip.iss
```

The installer is written to `target\installer`.

## Project layout

Shared by both versions:

| Path | Contents |
|---|---|
| `src/main.rs` | Entry point: modules per platform; Windows: command line, single instance, start-up. |
| `src/config.rs` | Settings in `config.ini` (INI parser that keeps comments, defaults). |
| `src/view.rs` | Window decisions: where a new window goes (replacing), its caption, which stream it plays. |
| `src/frigate.rs` | Event listener (WebSocket or MQTT), camera list from `/api/config`, connection test. |
| `src/filter.rs` | Decides whether a Frigate `events` / `reviews` message opens a window. |
| `src/rtsp.rs`, `src/rtp.rs` | RTSP client (TCP interleaved, Basic/Digest auth), SDP and RTP depacketizing. |
| `src/audio.rs` | Camera audio: AAC / G.711 depacketizing and decoding; WASAPI playback on Windows. |
| `src/hub.rs` | Prebuffer: persistent RTSP connections that keep the last GOP; the Windows player and the Linux ffmpeg pipeline subscribe to it. |
| `src/ws.rs`, `src/mqtt.rs`, `src/net.rs` | Minimal WebSocket, MQTT and HTTP clients. |
| `src/json.rs`, `src/util.rs` | JSON parser, small utilities and logging. |
| `src/i18n/` | Translations: `mod.rs` (the `Strings` struct and language selection) and one file per language. |

Windows application:

| Path | Contents |
|---|---|
| `src/app.rs` | UI thread: tray icon, menu, preview windows and their placement, settings logic. |
| `src/settings.rs` | The settings window (Win32 controls, tooltips, live preview, connection test output). |
| `src/player.rs` | Preview window thread: snapshot first, then live video (RTSP, MJPEG or snapshots). |
| `src/render.rs` | Drawing: Direct3D 11 swap chain, video processor, Direct2D/DirectWrite caption, GDI quick paint. |
| `src/decoder.rs` | H.264 / H.265 decoding with Media Foundation (hardware or software). |
| `src/h264.rs` | Rewrites the H.264 SPS so the Windows decoder outputs frames without delay. |
| `src/icon.rs` | Tray icon drawn in code (also the pixels of the Linux tray icon). |
| `src/update.rs` | Update check on GitHub (HTTPS via WinHTTP), download with SHA-256 check, silent installer run. |
| `installer/frigate-pip.iss` | Inno Setup script. |
| `build.rs`, `.cargo/config.toml` | MSVC builds: delay-loaded `mfplat.dll` and a statically linked C runtime, so the exe starts on any Windows 10/11 PC. |

Linux console version:

| Path | Contents |
|---|---|
| `src/console/` | `mod.rs` (main loop, windows, keys, mouse actions, viewer mode for tmux popups), `video.rs` (snapshot, RTSP → ffmpeg, MJPEG), `ffmpeg.rs`, `desktop.rs` (desktop windows), `x11.rs` (a minimal X11 client: windows, pictures, mouse events, monitors), `dbus.rs` (a minimal D-Bus client), `tray.rs` (tray icon with its menu, desktop notifications), `web.rs` (the settings page), `fb.rs` (framebuffer), `term.rs` (Kitty, Sixel, half blocks, status line), `tmux.rs` (tmux popups), `canvas.rs` (pictures, captions and buttons). |
| `packaging/` | Debian package: `frigate-pip@.service`, the menu entry and icon, and the maintainer scripts; the APT repository's public key and the GitHub Pages start page. |

Everything else:

| Path | Contents |
|---|---|
| `docs/<lang>/user-guide.md` | The user guide in every language: sections 1–11 for Windows, section 12 for the Linux console version. |
| `.github/workflows/build.yml` | CI for every push and pull request; releases. |
| `.github/workflows/apt.yml` | The APT repository on GitHub Pages. |

## Design principles

Please keep these in mind; pull requests are reviewed against them.

1. **Speed first.** Nothing may delay showing the window or the first picture. Do slow work (network, decoding, audio) on its own thread and never block the UI thread.
2. **Small and light.** No new dependencies without a very good reason. Waiting for detections must stay at practically 0% CPU and a few MB of memory.
3. **Every user-visible text is translated.** Text shown to users lives in `src/i18n/` only. Log messages and code comments are in English.
4. **Every option is documented.** A new option needs a tooltip (`tip_*` string) and a description in the user guide.
5. **Settings stay compatible.** Old `config.ini` files must keep working; new keys need sensible defaults.
6. **Both platforms build.** Shared modules (Frigate, RTSP, settings, translations) compile on Windows and Linux; platform code lives in the Windows-only modules (`#[cfg(windows)]`) or in `src/console/`. CI builds and tests both.

## Adding or improving a translation

User-interface texts are in `src/i18n/<code>.rs`, one `Strings` value per language. The compiler rejects a translation with a missing text, and the tests check that every language keeps the same `{0}`, `{1}`… placeholders.

To add a language:

1. Copy `src/i18n/en.rs` to `src/i18n/<code>.rs` and translate every value. Keep placeholders such as `{0}`, `{host}` and `{stream}` unchanged. Keep option names short enough for the settings window: check every tab of the settings window after the change.
2. In `src/i18n/mod.rs` add the module, a `Lang` variant, and the new language in `Lang::ALL`, `code`, `native_name`, `strings` and `from_primary_langid` (the Windows primary language ID).
3. Translate the user guide into `docs/<code>/user-guide.md`. Bold option names must match the texts in your `<code>.rs` exactly, and in-page links must point at your translated headings. Add the language to the *Languages* line of every guide and of `README.md`.
4. Add the language to `installer/frigate-pip.iss` (`[Languages]` and `[CustomMessages]`) if Inno Setup ships a translation for it.
5. Run `cargo test --release` and check the settings window in the new language: **Settings → Connection → Language**.

## Pull requests

1. Fork the repository and create a branch from `main`.
2. Keep each pull request focused on one change; describe what it changes and how you tested it (including on a real Frigate server, if relevant).
3. Make sure `cargo fmt`, `cargo clippy --release --all-targets` (no warnings) and `cargo test --release` pass.
4. Update the user guide (all languages if you can; otherwise English, and mention it in the pull request) and the translations for any new or changed text.
5. Do not include real IP addresses, camera names, credentials or recordings in code, tests, screenshots or logs.

## Releases

For every push and pull request, GitHub Actions runs `cargo fmt --check`, clippy and the tests on Windows and on Linux (x86_64 and ARM64). It builds the exe and the installer on Windows, and the static Linux binaries and the Debian packages on Linux; each package is installed, upgraded and removed once as a test.

Maintainers bump `version` in `Cargo.toml` and push a matching `vX.Y.Z` tag. The workflow then publishes the exe, the installer, the Linux archives and the Debian packages with their SHA-256 checksums on the Releases page (it fails if the tag and the version differ). After that it rebuilds the signed APT repository on GitHub Pages from the packages of all releases and installs FrigatePiP from it on x86_64 and ARM64; the signing key is the `APT_SIGNING_KEY` secret.
