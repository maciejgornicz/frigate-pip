# FrigatePiP — User guide

**Languages:** [English](../en/user-guide.md) · [Polski](../pl/user-guide.md) · [Deutsch](../de/user-guide.md) · [Español](../es/user-guide.md) · [Português](../pt/user-guide.md) · [Français](../fr/user-guide.md)

FrigatePiP is a small Windows tray application for [Frigate NVR](https://frigate.video). When Frigate detects a person, a car or any other object, FrigatePiP immediately shows a small picture-in-picture window with that camera's live video in a corner of your screen. When nothing moves any more, the window closes by itself.

## Contents

1. [Requirements](#1-requirements)
2. [Installation](#2-installation)
3. [First start](#3-first-start)
4. [The tray icon](#4-the-tray-icon)
5. [Preview windows](#5-preview-windows)
6. [The settings window](#6-the-settings-window)
7. [The settings file](#7-the-settings-file)
8. [Command line](#8-command-line)
9. [Language](#9-language)
10. [Files, privacy and resources](#10-files-privacy-and-resources)
11. [Troubleshooting](#11-troubleshooting)

---

## 1. Requirements

- **Windows 10 (version 1809 or later) or Windows 11**, 64-bit.
- **Frigate** reachable from your PC over the network. Frigate 0.14 or later is recommended; the *Review* modes need 0.14 or later.
- Frigate's **API port 5000** must be reachable from the PC. FrigatePiP does not log in, so the authenticated port 8971 cannot be used. With Docker, publish the port, e.g. `- "5000:5000"`.
- **Frigate add-on for Home Assistant:** port 5000 is disabled by default — enable it in the add-on's *Network* settings. HTTPS addresses are not supported: use the plain `http://` address in your local network.
- For smooth live video: Frigate's built-in **go2rtc RTSP server, port 8554** (`- "8554:8554"`). Without it you can still use MJPEG or snapshots (see [Live picture](#live-picture)).
- Optional: an **MQTT broker** used by Frigate, if you prefer MQTT to the Frigate WebSocket.
- For H.265 (HEVC) cameras, Windows needs an HEVC decoder. Most PCs with a recent graphics driver have one; otherwise install *HEVC Video Extensions* from the Microsoft Store, or use an H.264 sub stream. If no decoder is available, FrigatePiP automatically shows MJPEG from Frigate instead.
- **Windows N editions** (for example Windows 11 Pro N) have no Media Foundation until you add the optional Windows feature *Media Feature Pack*. FrigatePiP still works without it, but live video comes as MJPEG from Frigate (about 5 frames per second) and AAC camera audio is not available.

## 2. Installation

### With the installer (recommended)

1. Download `FrigatePiP-<version>-installer.exe` from the [Releases](https://github.com/maciejgornicz/frigate-pip/releases) page.
2. Run it. The installer does not need administrator rights; it installs for the current user only, into `%LOCALAPPDATA%\Programs\FrigatePiP`.
3. Choose whether FrigatePiP should **start with Windows** (on by default) and whether to create a desktop shortcut.
4. At the end, leave **Launch FrigatePiP** checked. The tray icon appears and the settings window opens on the first start.

The installer and the application are not signed with a code-signing certificate. Windows SmartScreen may show *“Windows protected your PC”*; click **More info → Run anyway**. You can check the SHA-256 checksum published with each release.

### Portable (without installing)

Download `FrigatePiP-<version>-portable.exe` from the Releases page and run it from any folder. Settings are stored in the same place as with the installer (see [Files](#10-files-privacy-and-resources)). Use **Start FrigatePiP with Windows** in the settings to start it automatically.

### Updating

FrigatePiP checks for a new version once a day (**Check for updates** in the settings). When one is available, a notification appears and both the tray menu and the settings window offer **Update to …**: FrigatePiP downloads the installer from GitHub, verifies its SHA-256 checksum and runs it — the update takes a few seconds and keeps your settings. **Check now** in the settings looks for a new version at once. You can also run the installer of a new version yourself. The portable version only opens the release page: download the new exe and replace the old one.

### Uninstalling

**Settings → Apps → Installed apps → FrigatePiP → Uninstall** (or *Add or remove programs*). The uninstaller closes FrigatePiP and removes the program, the shortcuts and the autostart entry. Your settings and log in `%APPDATA%\FrigatePiP` are kept; delete that folder to remove them too.

## 3. First start

1. On the first start the settings window opens on the **Connection** tab.
2. Enter the **Frigate address**, e.g. `http://192.168.1.10:5000`.
3. Click **Test connection**. FrigatePiP checks, without saving anything:
   - the Frigate API and its version,
   - the list of cameras,
   - the event source (WebSocket or MQTT),
   - RTSP video from go2rtc, including the codec and whether the camera has audio.

   Every line starts with ✔ (works) or ✖ (problem, with a hint).
4. Click **OK**. From now on FrigatePiP listens for detections in the background.
5. To see how a window looks, right-click the tray icon and choose **Show camera → (a camera)**, or check **Show live** at the bottom of the settings window.

## 4. The tray icon

The icon in the notification area shows the state:

| Colour | Meaning |
|---|---|
| Blue | Connected to Frigate, waiting for detections |
| Grey | Not connected (FrigatePiP keeps retrying automatically) |
| Amber | Notifications are paused |

Hover over the icon to see the status (for example *Connected: http://192.168.1.10:5000*).

- **Left click** opens the settings window.
- **Right click** opens the menu:

| Menu item | What it does |
|---|---|
| *(status line)* | Connection state or remaining pause time (informational). |
| **Update to …** | Appears when a new version is available: downloads and installs it (see [Updating](#updating)). |
| **Settings…** | Opens the settings window. |
| **Show camera** | Opens a preview window of the chosen camera right away, even while notifications are paused. Useful to check the picture or simply to look. |
| **Pause notifications** | **For 15 minutes**, **For 1 hour**, or **Until resumed**. While paused, detections do not open windows. The item reads **Paused** while a pause is active, and **Resume now** ends it. |
| **Close preview windows** | Closes all open preview windows, including pinned ones. |
| **Open Frigate** | Opens the Frigate web interface in your browser. |
| **Help** | Opens this guide. |
| **Show log** | Opens the log file in Notepad (useful when something does not work). |
| **Settings file (advanced)…** | Opens `config.ini` in Notepad. Saved changes apply immediately. |
| **Exit** | Closes FrigatePiP. |

## 5. Preview windows

When a detection matches your settings, a preview window appears in the chosen screen corner:

1. The window appears at once, within a few milliseconds of Frigate's message.
2. It first shows Frigate's latest snapshot of the camera, then switches to live video as soon as the stream delivers a picture (usually within a second, almost instantly with the [prebuffer](#prebuffer)).
3. The caption (if enabled) shows the camera name, the object (for example *person*), the sub label (for example a recognised face or plate), the confidence and the time. A **red dot** means the picture is live.

Preview windows never take the keyboard focus, so they do not interrupt typing or games.

### Mouse actions

| Action | Effect |
|---|---|
| **Drag** | Moves the window. It stays where you put it until it closes. |
| **Click** | **Pins** the window: it stays open until you close it (the caption shows *pinned*). Click again to unpin. |
| **Mouse wheel** | Makes the window bigger or smaller. |
| **×** (top-right corner) | Closes the window at once. The button appears when you point at the window. |
| **Right click** | Closes the window. |
| **Double click** | Opens this camera in the Frigate web interface. |
| **Hover** | While the mouse pointer is over the window, it does not close. After you move away it stays at least 3 more seconds. |

### When does a window close?

- **Time after last movement** seconds after the object (or any other matching object on that camera) last moved.
- At the latest after **at most** seconds, even if the object keeps moving.
- Never while it is pinned or while the mouse pointer is over it.

### More than one camera

Each camera has at most one window. New detections on a camera that already has a window only extend it and update the caption. Windows of different cameras are stacked from the chosen corner. **Max. number of windows** limits how many are open at once; **When all are in use** decides whether a new camera replaces the oldest window (in the same place on the screen; pinned windows are never replaced) or is skipped.

## 6. The settings window

Open it with a left click on the tray icon or with **Settings…** in the menu. **Every option has a description: hover over it with the mouse.** Press **F1** or click **Help** to open this guide.

Most changes take effect **immediately** on open preview windows, so you can see the result before saving. **OK** saves and closes, **Apply** saves and keeps the window open, **Cancel** (or closing the window) discards unsaved changes.

### 6.1 Connection

| Option | Description |
|---|---|
| **Frigate address** | Address of the Frigate server with the port, e.g. `http://192.168.1.10:5000` or `http://frigate.local:5000`. Port 5000 does not require logging in. |
| **Event source** | **Frigate WebSocket (recommended)** — FrigatePiP connects directly to Frigate; nothing else is needed. **MQTT** — detections are read from your MQTT broker (the one configured in Frigate). Both give the same result; the WebSocket is usually a little faster. |
| **MQTT server**, **port** | Host name or IP address and port (usually 1883) of the MQTT broker. Used only with the MQTT source. |
| **User / password** | MQTT login. Leave empty if the broker allows anonymous access. |
| **MQTT topic prefix** | `mqtt.topic_prefix` from the Frigate configuration, usually `frigate`. |
| **RTSP address (go2rtc)** | Template of the address of live video. `{host}` is replaced with the host of the Frigate address and `{stream}` with the go2rtc stream name. The default `rtsp://{host}:8554/{stream}` fits a standard Frigate installation. If go2rtc requires a login, use `rtsp://user:password@{host}:8554/{stream}`. |
| **Test connection** | Checks the values entered above (Frigate, cameras, events, RTSP video and audio) without saving them. |
| **Language** | Language of FrigatePiP. **Automatic (Windows)** follows the Windows display language; English is used when that language is not available. See [Language](#9-language). |
| **Start FrigatePiP with Windows** | Starts FrigatePiP automatically when you sign in to Windows. |
| **Check for updates** | Once a day FrigatePiP asks GitHub whether a new version exists and, if so, shows a notification and offers the update. Nothing about you or your cameras is sent. |
| **Check now** / **Update to …** | Looks for a new version at once; when one is found, the button downloads and installs it (your settings are kept). The text next to it shows the result. |

### 6.2 Detection

| Option | Description |
|---|---|
| **Show a window for** | Which Frigate messages open a window: **Every detected object (fastest)** — every object tracked by Frigate; **Review alerts only** — only Review items with *alert* severity; **Review alerts and detections** — all Review items. Review items follow the alert and detection rules (labels and required zones) set in Frigate's `review` configuration and arrive slightly later than object events. |
| **Cameras** | Cameras that may open a window. Unchecked cameras are ignored. With all cameras checked, cameras added to Frigate later are included too. |
| **Objects** | Object types (labels) that may open a window, e.g. person, car, dog. The list shows the objects tracked by your cameras. With all checked, every object type counts. |
| **Required zones** | If you check zones, a window opens only when the object enters one of them. With nothing checked, any place in the picture counts. |
| **Ignore objects that are not moving** | Objects that Frigate marks as stationary (for example a parked car) do not open windows and do not keep them open. Applies to *Every detected object*. |
| **Minimum confidence** | Detections with a lower confidence are ignored. 0% means Frigate's own thresholds decide. Applies to *Every detected object*. |

Frigate's false positives are always ignored. Each object opens a window only once; when it moves again, it only extends the open window.

### 6.3 Preview window

| Option | Description |
|---|---|
| **Monitor** | Monitor on which windows appear: **Primary** or a specific monitor (numbered from the left, with its resolution). |
| **Screen corner** | Corner in which windows appear. More windows are stacked from this corner. |
| **Window width** | Width in pixels (at 100% scaling; FrigatePiP adjusts it to the monitor's scaling). The height follows the camera's aspect ratio. You can also change the size of a single window with the mouse wheel. |
| **Distance from edge** | Gap between the windows and the edges of the screen (and between windows). |
| **Time after last movement** | How many seconds a window stays open after the last movement. |
| **at most** | Maximum time in seconds a window is shown, even if the object keeps moving (unless it is pinned). |
| **Max. number of windows** | How many windows (cameras) can be open at the same time (1–8). |
| **When all are in use** | **Replace the oldest window (same place)** — the new camera takes the place of the oldest unpinned window; **Skip the new event** — the new detection is ignored. |
| **Always on top** | Keeps windows above all other windows. |
| **Caption on the image** | Shows the camera, object, confidence and time on the picture. |
| **Notification sound** | Plays the Windows *Asterisk* sound when a new window appears. This is not the camera audio (see [Camera audio](#camera-audio)). |
| **Do not show during full-screen games or presentations** | No windows appear while a full-screen game, video or presentation is running (detections are skipped, not delayed). |

### 6.4 Video & audio

#### Live picture

| Choice | Description |
|---|---|
| **RTSP from go2rtc** (default) | Smooth video at the camera's frame rate with hardware decoding (H.264 and H.265), low delay. Needs port 8554. |
| **MJPEG from Frigate** | About 5 frames per second at the detect resolution, through port 5000. Use it when RTSP is not available. |
| **Refreshed snapshots only** | Only Frigate's snapshots, refreshed regularly. The lightest option. |

#### Stream

| Choice | Description |
|---|---|
| **Automatic (by window size)** | The light **sub** stream for normal windows, the full-resolution **main** stream when the window is wider than **Main from window width** (in physical pixels). |
| **Sub (light)** | Always the sub stream. |
| **Main (full resolution)** | Always the main stream. |

FrigatePiP finds each camera's go2rtc streams in the Frigate configuration (`live.streams`, `live.stream_name`, or go2rtc streams named `<camera>` and `<camera>_sub`). You can set them yourself in the [settings file](#per-camera-settings).

#### Camera audio

**Camera audio** plays the sound of the camera (AAC or G.711 from the RTSP stream) in each open window. It runs separately, so it never delays the window or the video. The volume can be set in the Windows volume mixer (*FrigatePiP*). It requires **RTSP from go2rtc**.

#### Prebuffer

**Prebuffer** keeps a connection to the sub stream of every camera, so live video starts at once (about 0.15 s) instead of waiting for the next key frame (up to about 1 s, depending on the camera's key frame interval). Cost: about 0.1–0.5 Mbit/s of local network traffic per camera, all the time.

#### Ready GPU

**Ready GPU** keeps the graphics device initialised, so video starts about 130 ms sooner. Cost: about 1% of one CPU core while idle (graphics driver thread).

### 6.5 Bottom of the window

| Control | Description |
|---|---|
| **Preview:** camera | Camera used for the live preview of the settings. |
| **Show live** | Opens a real preview window of that camera. It follows your changes (corner, size, caption, stream, audio…) immediately, before you save them. It is pinned and does not count towards the window limit. |
| **OK** / **Cancel** / **Apply** | Save and close / discard / save and keep the window open. |
| **Help** | Opens this guide (also **F1**). |

## 7. The settings file

All settings are stored in `%APPDATA%\FrigatePiP\config.ini` (a plain text file). The settings window writes all of them; you only need the file for per-camera settings. **Settings file (advanced)…** in the tray menu opens it in Notepad. Changes are applied automatically when the file is saved. Comments start with `;`. Yes/no values accept `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`.

| Section | Key | Default | Meaning |
|---|---|---|---|
| `[app]` | `language` | `auto` | `auto` (Windows language) or `en`, `pl`, `de`, `es`, `pt`, `fr`. |
| | `check_updates` | `true` | Look for a new version once a day. |
| `[frigate]` | `url` | `http://frigate.local:5000` | Frigate address. |
| | `source` | `websocket` | `websocket` or `mqtt`. |
| | `rtsp` | `rtsp://{host}:8554/{stream}` | RTSP address template. |
| `[mqtt]` | `host` | *(empty)* | MQTT broker (only with `source = mqtt`). |
| | `port` | `1883` | Broker port. |
| | `user`, `password` | *(empty)* | Login. |
| | `prefix` | `frigate` | Topic prefix. |
| `[trigger]` | `mode` | `events` | `events` (every object), `alerts` (Review alerts) or `reviews` (alerts and detections). |
| | `cameras` | *(empty = all)* | Comma-separated camera names, e.g. `front_door, garden`. |
| | `labels` | *(empty = all)* | Comma-separated objects, e.g. `person, car`. |
| | `zones` | *(empty = anywhere)* | Required zones. |
| | `ignore_stationary` | `true` | Ignore stationary objects. |
| | `min_score` | `0` | Minimum confidence 0.0–1.0. |
| `[display]` | `monitor` | `primary` | `primary` or monitor number from the left (1, 2, 3…). |
| | `position` | `bottom-right` | `top-left`, `top-right`, `bottom-left`, `bottom-right`. |
| | `margin` | `16` | Distance from the screen edge in pixels. |
| | `width` | `480` | Window width in pixels. |
| | `duration` | `15` | Seconds after the last movement. |
| | `max_duration` | `120` | Maximum seconds a window is shown. |
| | `topmost` | `true` | Always on top. |
| | `sound` | `false` | Notification sound. |
| | `overlay` | `true` | Caption on the picture. |
| | `max_popups` | `4` | Max. number of windows. |
| | `replace` | `true` | `true` = replace the oldest window, `false` = skip the new event. |
| | `skip_fullscreen` | `false` | Do not show during full-screen applications. |
| `[video]` | `live` | `rtsp` | `rtsp`, `mjpeg` or `snapshot`. |
| | `stream` | `auto` | `auto`, `sub` or `main`. |
| | `auto_main_width` | `800` | Window width (px) from which `auto` uses the main stream. |
| | `prebuffer` | `false` | Prebuffer. |
| | `gpu_warm` | `false` | Ready GPU. |
| | `camera_audio` | `false` | Camera audio. |

### Per-camera settings

Add a section `[camera.<name>]`, where `<name>` is the camera name in Frigate:

```ini
[camera.front_door]
; Name shown in the caption, the menu and the settings
name = Front door
; go2rtc stream names
main = front_door
sub = front_door_sub
```

## 8. Command line

| Command | Effect |
|---|---|
| `frigate-pip.exe` | Starts FrigatePiP (only one copy runs at a time). |
| `frigate-pip.exe --settings` | Opens the settings window (starts FrigatePiP if it is not running). |
| `frigate-pip.exe --test <camera>` | Shows a preview window of the camera right away, e.g. `--test front_door`. You can create a Windows shortcut with a keyboard shortcut for your favourite camera. |
| `frigate-pip.exe --quit` | Closes the running FrigatePiP. |

If FrigatePiP is already running, the command is passed to it.

## 9. Language

FrigatePiP is available in English, Polish, German, Spanish, Portuguese and French. With **Automatic (Windows)** it uses the Windows display language (*Settings → Time & language → Language & region → Windows display language*), or English if that language is not available. You can choose a language yourself under **Connection → Language**; the settings window switches immediately.

## 10. Files, privacy and resources

| What | Where |
|---|---|
| Program | `%LOCALAPPDATA%\Programs\FrigatePiP\frigate-pip.exe` (installer) |
| Settings | `%APPDATA%\FrigatePiP\config.ini` |
| Log | `%APPDATA%\FrigatePiP\frigate-pip.log` (up to 512 KB; the previous one is kept as `frigate-pip.old.log`) |
| Autostart | Registry value `FrigatePiP` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |

**Privacy:** FrigatePiP connects only to the addresses you enter (Frigate, go2rtc and the MQTT broker) and, unless you switch off **Check for updates**, once a day to GitHub to look for a new version. It has no telemetry, no accounts and no cloud services. Pictures are only displayed and never saved. The **Help** links open this guide on GitHub in your browser.

**Resources:** while waiting for detections FrigatePiP uses practically no CPU and about 5–10 MB of memory. Video is decoded by the graphics card when possible. Without a usable graphics card (for example in some virtual machines) it draws with the WARP software renderer.

## 11. Troubleshooting

Start with **Test connection** in the settings and with **Show log** in the tray menu.

**The icon stays grey (not connected)**
- Check the Frigate address and the port (5000) in a browser on the same PC: `http://<frigate>:5000/api/version` should show the version.
- In Docker, port 5000 must be published. Port 8971 (with login) is not supported.
- Home Assistant add-on: enable port 5000 in the add-on's *Network* settings.
- The address must start with `http://`. HTTPS (for example through a reverse proxy) is not supported.
- With MQTT: check the broker address, login and topic prefix.

**No windows appear**
- Is the icon amber? Notifications are paused — choose **Resume now**.
- Check the **Detection** tab: cameras, objects and required zones. With required zones checked, objects must enter them.
- *Review* modes depend on Frigate's review configuration; try **Every detected object**.
- **Do not show during full-screen games or presentations** skips windows while a full-screen application is in front.
- **Max. number of windows** with **Skip the new event** skips detections when all windows are open.
- **Minimum confidence** may be too high.

**The window shows only a still picture**
- Live video needs go2rtc's RTSP port 8554. Check the **RTSP video** line of **Test connection**.
- If go2rtc requires a login, put it into the **RTSP address**.
- For H.265 cameras install *HEVC Video Extensions*, or use an H.264 sub stream.
- As a workaround select **MJPEG from Frigate**.
- On Windows N without the *Media Feature Pack* RTSP video is not available; FrigatePiP uses MJPEG automatically (the log says *MJPEG from Frigate instead of RTSP*).

**Video starts slowly**
- Enable **Prebuffer** and **Ready GPU**.
- Shorten the key frame interval (I-frame interval / GOP) of the camera's sub stream to 1–2 s.

**No camera sound**
- **Camera audio** must be checked and **Live picture** set to **RTSP from go2rtc**.
- The go2rtc stream must contain audio — **Test connection** shows *audio … Hz* or *no audio*.
- Check the volume of *FrigatePiP* in the Windows volume mixer.

**Something else** — open an [issue on GitHub](https://github.com/maciejgornicz/frigate-pip/issues) and attach the relevant part of the log. Remove addresses and camera names you do not want to share.
