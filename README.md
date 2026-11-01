# FrigatePiP

**Instant picture-in-picture camera windows for [Frigate NVR](https://frigate.video) on Windows.**

When Frigate detects a person, a car or any other object, FrigatePiP pops up a small always-on-top window with that camera's live video in a corner of your screen — within milliseconds — and closes it again when nothing moves any more. It lives in the notification area, takes about 5–10 MB of memory and practically no CPU while waiting.

**User guide:** [English](docs/en/user-guide.md) · [Polski](docs/pl/user-guide.md) · [Deutsch](docs/de/user-guide.md) · [Español](docs/es/user-guide.md) · [Português](docs/pt/user-guide.md) · [Français](docs/fr/user-guide.md)

## Features

- **Fast:** the window appears ~5 ms after Frigate's message, Frigate's snapshot is shown ~35 ms later, and smooth live video follows (well under a second; almost instantly with the optional prebuffer).
- **Live video from go2rtc (RTSP)** with hardware decoding of H.264 and H.265 (Media Foundation + Direct3D 11), up to 4K, or MJPEG / snapshots when RTSP is not available.
- **Camera audio** (AAC, G.711) on a separate thread, so it never delays the picture.
- **Filters:** cameras, objects, required zones, minimum confidence, stationary objects; every object, or Frigate *Review* alerts / detections only.
- **Windows the way you like them:** monitor, corner, size, timing, how many at once, replace the oldest or skip, always on top, caption, notification sound, quiet during full-screen games and presentations.
- **Mouse control:** drag to move, click to pin, wheel to resize, right-click to close, double-click to open the camera in Frigate.
- **Settings window** with a live preview of your changes, a connection test and a description of every option.
- **Events via the Frigate WebSocket** (nothing else needed) or via **MQTT**.
- **Six languages** — English, Polski, Deutsch, Español, Português, Français — chosen automatically from Windows.
- A single small executable (~0.7 MB) with no runtime dependencies; per-user installer, no administrator rights.

## Installation

1. Download **`FrigatePiP-Setup-<version>.exe`** from [Releases](https://github.com/maciejgornicz/frigate-pip/releases/latest).
2. Run it (no administrator rights needed). If Windows SmartScreen says *“Windows protected your PC”*, click **More info → Run anyway** — the program is not code-signed.
3. Keep **Start FrigatePiP with Windows** checked if you want it to start automatically, and finish with **Launch FrigatePiP**.
4. The settings window opens. Enter your **Frigate address** (for example `http://192.168.1.10:5000`), click **Test connection**, then **OK**.

Prefer no installer? Download `frigate-pip.exe` from the same release and run it from any folder.

**Requirements:** Windows 10 (1809+) or 11, 64-bit; Frigate reachable on its API port **5000** (the unauthenticated port; 8971 is not supported) and, for live video, go2rtc's RTSP port **8554**. Details, updating and uninstalling: [user guide → Installation](docs/en/user-guide.md#2-installation).

## Usage in short

- **Left-click** the tray icon to open the settings; **right-click** for the menu (show a camera, pause notifications, open Frigate, help, log, exit).
- Hover over any option in the settings window to see what it does; **F1** opens the guide.
- Settings are stored in `%APPDATA%\FrigatePiP\config.ini`; the log in `%APPDATA%\FrigatePiP\frigate-pip.log`.
- Command line: `frigate-pip.exe --settings`, `--test <camera>`, `--quit`.

Everything is described in the [user guide](docs/en/user-guide.md).

## How it is so fast

1. Detections arrive over Frigate's WebSocket (`/ws`); other messages are discarded after reading only their topic.
2. The window is created at once (~5 ms). In parallel:
   - Frigate's `latest.jpg` is decoded with WIC and painted with GDI (~35 ms),
   - Direct3D 11 / Direct2D start up and take over drawing,
   - the RTSP client connects to go2rtc (~20 ms) and waits for a key frame.
3. The H.264 SPS is rewritten (`max_num_reorder_frames = 0`); without it the Windows decoder holds frames until the next IDR (up to ~1 s of delay).
4. Only the newest decoded frame is drawn; late frames are dropped.
5. Camera audio uses the same RTSP connection; AAC decoding and WASAPI playback run on their own thread with a ~100–250 ms buffer.
6. Optional: the *prebuffer* keeps the sub streams connected so video starts without waiting for a key frame, and *Ready GPU* keeps the graphics device initialised.

## Building from source

```
rustup default stable            # MSVC or GNU toolchain, x86_64
cargo build --release            # target\release\frigate-pip.exe
cargo test --release
```

The installer is built with [Inno Setup 6](https://jrsoftware.org/isinfo.php): `ISCC.exe /DAppVersion=1.0.0 installer\frigate-pip.iss`. See [CONTRIBUTING.md](CONTRIBUTING.md) for the project layout, live tests and how to add a language.

## Contributing

Bug reports, translations and pull requests are welcome — please read [CONTRIBUTING.md](CONTRIBUTING.md).

## License

FrigatePiP is free software: you can redistribute it and/or modify it under the terms of the **GNU General Public License v3.0**. See [LICENSE](LICENSE).

FrigatePiP is an independent project and is not affiliated with Frigate or its authors.
