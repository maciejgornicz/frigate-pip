# FrigatePiP — User guide

**Languages:** [English](../en/user-guide.md) · [Polski](../pl/user-guide.md) · [Deutsch](../de/user-guide.md) · [Español](../es/user-guide.md) · [Português](../pt/user-guide.md) · [Français](../fr/user-guide.md)

FrigatePiP is a small Windows tray application for [Frigate NVR](https://frigate.video). When Frigate detects a person, a car or any other object, FrigatePiP immediately shows a small picture-in-picture window with that camera's live video in a corner of your screen. When nothing moves any more, the window closes by itself.

Sections 1–11 describe the Windows application. FrigatePiP also runs on Linux — on the desktop and on a console without a desktop: that version is described separately in section 12, [Linux version](#12-linux-version).

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
12. [Linux version](#12-linux-version)

---

## 1. Requirements

- **Windows 10 (version 1809 or later) or Windows 11**, 64-bit.
- **Frigate** reachable from your PC over the network. Frigate 0.14 or later is recommended; the *Review* modes need 0.14 or later.
- Frigate's **API** must be reachable from the PC: the unauthenticated **port 5000** (`http://`, no login) or the authenticated **port 8971** (`https://`, with a Frigate user — the *viewer* role is enough; create one under *Settings → Users* in Frigate). With Docker, publish the port, e.g. `- "5000:5000"` or `- "8971:8971"`. A reverse proxy in front of Frigate works too.
- **Frigate add-on for Home Assistant:** port 5000 is disabled by default — enable it in the add-on's *Network* settings. Or use port 8971 with a Frigate user.
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
| **Ctrl + mouse wheel** | Zooms the picture in (up to 8×) or out around the mouse pointer; the caption shows the zoom, e.g. *2.0×*. How much detail there is depends on the stream: the main stream (`stream = main`) has the most. |
| **Ctrl + drag** | On a zoomed picture: moves the picture, so that you can see another part of it. Without zoom the window moves, as usual. |
| **×** (top-right corner) | Closes the window at once. The button appears when you point at the window. |
| **Speaker** (left of ×) | Turns the window's sound off; click again to turn it back on. The button appears when you point at a window that plays camera audio; while the sound is off, the crossed-out speaker stays visible. |
| **Volume slider** (under the speaker) | Sets the sound volume of this window: drag it or click on it. It appears together with the speaker; raising the volume also turns muted sound back on. On Linux you hear the new volume just after you let go. |
| **Replay** ⟲ (left of the other buttons) | Plays the start of the event again at normal speed, without sound; the caption shows *Replay −12 s*: how far behind live the picture is. With the [prebuffer](#prebuffer) the replay starts 5 s before the window opened, without it when the window opened. Click again to return to live video at once; after the replay it returns by itself. The button appears when you point at a window with live video; the window stays open during the replay. |
| **Right click** | Closes the window. |
| **Double click** | Opens this camera in the Frigate web interface. |
| **Hover** | While the mouse pointer is over the window, it does not close. After you move away it stays at least 3 more seconds. |

### When does a window close?

- **Time after last movement** seconds after the object (or any other matching object on that camera) last moved.
- At the latest after **at most** seconds, even if the object keeps moving.
- Never while it is pinned or while the mouse pointer is over it.
- Never during a replay; afterwards it stays at least 5 more seconds.

### More than one camera

Each camera has at most one window. New detections on a camera that already has a window only extend it and update the caption. Windows of different cameras are stacked from the chosen corner. **Max. number of windows** limits how many are open at once; **When all are in use** decides whether a new camera replaces the oldest window (in the same place on the screen; pinned windows are never replaced) or is skipped.

## 6. The settings window

Open it with a left click on the tray icon or with **Settings…** in the menu. **Every option has a description: hover over it with the mouse.** Press **F1** or click **Help** to open this guide.

Most changes take effect **immediately** on open preview windows, so you can see the result before saving. **OK** saves and closes, **Apply** saves and keeps the window open, **Cancel** (or closing the window) discards unsaved changes.

### 6.1 Connection

| Option | Description |
|---|---|
| **Frigate address** | Address of the Frigate server with the port: `http://192.168.1.10:5000` (port 5000, no login) or `https://192.168.1.10:8971` (port 8971, with the Frigate user below). The address of a reverse proxy works too. |
| **Frigate user / password** | Only for port 8971 or a reverse proxy that asks for a Frigate login: a Frigate user (*Settings → Users* in Frigate); the *viewer* role is enough. Leave empty for port 5000. FrigatePiP logs in, keeps the token Frigate gives it and logs in again when it expires. The password is stored in `config.ini`, like the MQTT password. |
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
| **Objects** | Object types (labels) that may open a window, e.g. person, car, dog. The list shows the objects tracked by your cameras. With all checked, every object type counts. The list above it chooses whose objects you see: **All cameras**, or one camera. Checks made for a camera give it its own list, used instead of the one for all cameras (in every mode, also for manual events); such a camera is marked with `*`. When a camera's checks are the same as for all cameras, it has no list of its own and follows later changes. **Same as all cameras** removes the camera's own list. |
| **Required zones** | If you check zones, a window opens only when the object enters one of them. With nothing checked, any place in the picture counts. |
| **Ignore objects that are not moving** | Objects that Frigate marks as stationary (for example a parked car) do not open windows and do not keep them open. Applies to *Every detected object*. |
| **Minimum confidence** | Detections with a lower confidence are ignored. 0% means Frigate's own thresholds decide. Applies to *Every detected object*. |

Frigate's false positives are always ignored. Each object opens a window only once; when it moves again, it only extends the open window.

**Manual events** open a window too: events created through Frigate's API (`POST /api/events/<camera>/<label>/create`, for example by a Home Assistant automation when the doorbell rings) and the items of a dedicated LPR camera. The caption shows the event's label. This works with **Every detected object (fastest)** and **Review alerts and detections**, and with **Review alerts only** when Frigate makes the event an alert (its label is one of the review alert labels, by default `person` and `car`). To try it, in PowerShell: `Invoke-RestMethod -Method Post -ContentType application/json -Body '{"duration": 10}' http://<frigate>:5000/api/events/<camera>/doorbell/create`; on Linux: `curl -X POST -H "Content-Type: application/json" -d '{"duration": 10}' http://<frigate>:5000/api/events/<camera>/doorbell/create`. The camera, object and zone filters apply as to other detections (an objects filter of `person, car` hides a `doorbell` event); the minimum confidence does not. Frigate creates the Review item that FrigatePiP reacts to only when the camera records (`record` enabled). A manual event made while the camera's Review item is still open (up to the alert cutoff, by default 40 s after the previous event) is added to that item, and Frigate announces it only when an object moves on the camera — so when testing, wait about a minute between two events.

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
| **MJPEG from Frigate** | About 5 frames per second at the detect resolution, through Frigate's API. Use it when RTSP is not available. |
| **Refreshed snapshots only** | Only Frigate's snapshots, refreshed regularly. The lightest option. |

#### Stream

| Choice | Description |
|---|---|
| **Automatic (by window size)** | The light **sub** stream for normal windows, the full-resolution **main** stream when the window is wider than **Main from window width** (in physical pixels). |
| **Sub (light)** | Always the sub stream. |
| **Main (full resolution)** | Always the main stream. |

FrigatePiP finds each camera's go2rtc streams in the Frigate configuration (`live.streams`, `live.stream_name`, or go2rtc streams named `<camera>` and `<camera>_sub`). You can set them yourself in the [settings file](#per-camera-settings).

#### Camera audio

**Camera audio** plays the sound of the camera (AAC or G.711 from the RTSP stream) in each open window. It runs separately, so it never delays the window or the video. The volume can be set in the Windows volume mixer (*FrigatePiP*). It requires **RTSP from go2rtc**. To mute a single window, click the speaker button in its top-right corner (see [Mouse actions](#mouse-actions)). **Volume** sets how loud a new window starts (0–100 %, `camera_volume`), for example to keep wind noise down; each window also has its own slider.

#### Prebuffer

**Prebuffer** keeps a connection to the sub stream of every camera, so live video starts at once (about 0.15 s) instead of waiting for the next key frame (up to about 1 s, depending on the camera's key frame interval). Cost: about 0.1–0.5 Mbit/s of local network traffic per camera, all the time. It also keeps the last 30 s of every camera in memory (as much as 30 s of its sub stream: about 0.2–2 MB per camera), so that the [replay](#mouse-actions) can start 5 s before the event.

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

All settings are stored in `%APPDATA%\FrigatePiP\config.ini` (a plain text file). The settings window writes all of them; you only need the file for a camera's name and streams. **Settings file (advanced)…** in the tray menu opens it in Notepad. Changes are applied automatically when the file is saved. Comments start with `;`. Yes/no values accept `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`.

| Section | Key | Default | Meaning |
|---|---|---|---|
| `[app]` | `language` | `auto` | `auto` (Windows language) or `en`, `pl`, `de`, `es`, `pt`, `fr`. |
| | `check_updates` | `true` | Look for a new version once a day. |
| `[frigate]` | `url` | `http://frigate.local:5000` | Frigate address. |
| | `user`, `password` | *(empty)* | Frigate login for port 8971 (empty for port 5000). |
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
| | `camera_volume` | `100` | Volume of a new window, 0–100 %. |
| `[camera.<name>]` | `labels` | *(empty = as in `[trigger]`)* | Objects for this camera, instead of `labels` in `[trigger]`, e.g. `person, car, dog`. |

### Per-camera settings

Add a section `[camera.<name>]`, where `<name>` is the camera name in Frigate:

```ini
[camera.front_door]
; Name shown in the caption, the menu and the settings
name = Front door
; go2rtc stream names
main = front_door
sub = front_door_sub
; Objects that open a window on this camera, instead of labels in [trigger]
labels = person, car, dog, cat
```

`labels` gives the camera its own object list: it replaces `labels` in `[trigger]` for this camera, in every mode (also for manual events). Empty or missing = the list for all cameras. The settings window and the Linux settings page set it too: pick the camera above the **Objects** list.

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
- Check the Frigate address and the port in a browser on the same PC: `http://<frigate>:5000/api/version` (or `https://<frigate>:8971/api/version` after logging in) should show the version.
- In Docker, the port (5000 or 8971) must be published.
- Home Assistant add-on: enable port 5000 in the add-on's *Network* settings, or use port 8971 with a Frigate user.
- Port 8971 or a reverse proxy: enter the Frigate user and password; **Test connection** shows whether the login works. A self-signed certificate (Frigate's own one on port 8971) is trusted the first time and remembered in `certificates.txt` (next to `config.ini`); if the server later shows a different one, FrigatePiP refuses to connect — when you replaced the certificate yourself, delete its line in that file.
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

**One camera's picture is behind another's**
- The cameras share no clock, so FrigatePiP cannot line up their pictures.
- Every window leaves a line in the log when it closes: *timing - 73 frames, picture in 3 ms (max 96), late up to 177 ms, longest gap 238 ms*. *picture in* is the time spent in FrigatePiP, *late up to* how far frames fell behind the timing of their stream, and *longest gap* the longest pause between two frames.
- Small values with a picture that is still behind mean the delay arises before the video reaches FrigatePiP: check the camera's stream settings. Large values point to the network or, for *picture in*, to the PC.

**No camera sound**
- **Camera audio** must be checked and **Live picture** set to **RTSP from go2rtc**.
- The go2rtc stream must contain audio — **Test connection** shows *audio … Hz* or *no audio*.
- Check the volume of *FrigatePiP* in the Windows volume mixer.

**FrigatePiP closes by itself**
- Every line of the log starts with the date and time. A crash of FrigatePiP leaves a last line starting with *Crash in thread*: attach it to an issue (see below).
- If the log simply stops, without *Shutting down* and without *Crash*, something else ended the program: Windows shutting down or updating, or the closing of the program that started it (for example a terminal window).

**Something else** — open an [issue on GitHub](https://github.com/maciejgornicz/frigate-pip/issues) and attach the relevant part of the log. Remove addresses and camera names you do not want to share.

---

## 12. Linux version

FrigatePiP also runs on Linux — on a **desktop** (GNOME, KDE, Xfce…) like on Windows, and **without a desktop**, for example on a Raspberry Pi with a monitor on the wall, or on a server you reach over SSH. It shows the same preview windows, in one of four modes:

| Mode | Where the windows appear | Runs in the background? | Typical use | How to start |
|---|---|---|---|---|
| **Desktop windows** | On the desktop (X11, or Wayland with XWayland): borderless windows above all others in the chosen corner, like on Windows | **Yes** — without a terminal; from the applications menu or with the desktop session | A Linux PC or laptop | Menu → **FrigatePiP**, or `frigate-pip` on the desktop — see [Desktop windows](#desktop-windows) |
| **Framebuffer** | On the screen of a Linux console without X or Wayland, in the chosen corner, over the console text | **Yes** — as a systemd service | A Raspberry Pi or another computer with a monitor and no desktop (a wall display) | `frigate-pip --output framebuffer`, as a service: see [Starting automatically](#starting-automatically-systemd) |
| **Terminal** | Inside the terminal window FrigatePiP runs in: a caption line, the picture and a status line | **No** — the terminal shows FrigatePiP while it runs (like `top`) | A terminal window or tmux pane kept open as a camera monitor, also over SSH | `frigate-pip` |
| **tmux popups** | As a tmux popup over whatever you are doing in tmux; it closes by itself | **Yes** — in a hidden tmux window | Working in tmux (locally or over SSH) and wanting cameras to pop up | `tmux new -d -s frigate-pip 'frigate-pip --output tmux'`, then work inside tmux — see [tmux popups](#tmux-popups) |

**On a Windows PC — also when you work in WSL — use the Windows application:** it runs in the notification area and its windows appear over everything, terminals included. The Linux version in WSL is useful only for trying it out.

When a window closes on the framebuffer, what was under it comes back. In a terminal and in tmux popups the picture is drawn with **Kitty graphics** (Kitty, WezTerm, Ghostty), **Sixel** (foot, Konsole, xterm, Windows Terminal 1.22+) or, in any terminal with 24-bit colour, with coloured half blocks (low resolution; tmux popups always use half blocks).

Video is decoded by the **ffmpeg** program; FrigatePiP's own RTSP client feeds it, so a picture appears a few milliseconds after the camera's next key frame.

### Requirements

- Linux on **x86_64** or **ARM64** (Raspberry Pi 3, 4 or 5 with a 64-bit system).
- For desktop windows: a desktop with X11, or Wayland with XWayland (GNOME, KDE Plasma, Xfce and most others).
- **ffmpeg**: installed together with the `.deb` package; otherwise `sudo apt install ffmpeg` (Debian, Ubuntu, Raspberry Pi OS) or your distribution's package.
- For captions on the framebuffer: the DejaVu font (`fonts-dejavu-core`, normally installed together with ffmpeg).
- For the framebuffer: membership of the `video` group (`sudo usermod -aG video $USER`, then log in again).
- Frigate reachable from the computer on its API port 5000 (`http://`) or 8971 (`https://`, with a Frigate user) and, for live video, go2rtc's RTSP port 8554 (details in [Requirements](#1-requirements)).

### Installation

1. Install the program:
   - **Debian, Ubuntu, Raspberry Pi OS (64-bit), from the FrigatePiP repository** (recommended — `sudo apt upgrade` then also updates FrigatePiP): add the repository once and install the program; apt also installs ffmpeg:

     ```
     sudo wget -qO /usr/share/keyrings/frigate-pip.gpg https://maciejgornicz.github.io/frigate-pip/apt/frigate-pip.gpg
     echo "deb [signed-by=/usr/share/keyrings/frigate-pip.gpg] https://maciejgornicz.github.io/frigate-pip/apt stable main" | sudo tee /etc/apt/sources.list.d/frigate-pip.list
     sudo apt update
     sudo apt install frigate-pip
     ```

   - **Debian, Ubuntu, Raspberry Pi OS (64-bit), a single package file** — download `frigate-pip_<version>_amd64.deb` (or `frigate-pip_<version>_arm64.deb` for a Raspberry Pi) from the [Releases](https://github.com/maciejgornicz/frigate-pip/releases) page and install it with apt, which also installs ffmpeg:

     ```
     sudo apt install ./frigate-pip_*.deb
     ```

   - **Other distributions** — download `FrigatePiP-<version>-linux-x86_64.tar.gz` or `FrigatePiP-<version>-linux-arm64.tar.gz`, unpack it and put the program on the path:

     ```
     tar xzf FrigatePiP-*-linux-*.tar.gz
     sudo install frigate-pip /usr/local/bin/
     ```

2. Start it once: it creates `~/.config/frigate-pip/config.ini`. Set `url` in the `[frigate]` section to your Frigate address.
3. Check the connection: `frigate-pip --test-connection`.
4. Start `frigate-pip`. With `--test <camera>` (or the keys **1–9**) you can see a camera right away.

The program is a single file without other libraries. To update it, run `sudo apt update && sudo apt upgrade` (repository), install the newer package file the same way or replace the file; to remove it, run `sudo apt remove frigate-pip` (or delete the file) and delete `~/.config/frigate-pip` — and, if you added the repository, `/etc/apt/sources.list.d/frigate-pip.list` and `/usr/share/keyrings/frigate-pip.gpg`.

### Where the windows appear

With `--output auto` (the default) FrigatePiP opens **desktop windows** when it runs on a desktop (`DISPLAY` is set), uses the **framebuffer** on a Linux console (`TERM=linux`) or without a terminal (as a service), and the **terminal** otherwise. The **tmux popups** mode is used only when you choose it (`--output tmux` or `output = tmux`). In a terminal, the picture mode is chosen automatically: Kitty graphics where available, otherwise Sixel when the terminal reports it, otherwise half blocks. Both can be set in the settings file or on the command line.

### Desktop windows

On a Linux desktop the preview windows work like on Windows: borderless windows above all others, in the chosen corner (`position`) of the chosen monitor (`monitor`: `primary` or a number counted from the left), as wide as `width` (multiplied by the desktop scaling). They need X11, or Wayland with XWayland — GNOME, KDE Plasma and most other desktops start XWayland for such programs. Pure Wayland is not enough: it does not let a program put a window in a corner or keep it on top.

- **Start:** applications menu → **FrigatePiP** (installed with apt), or `frigate-pip` in a terminal on the desktop. It runs in the background without a terminal; one copy runs per user. A second start (or `frigate-pip --settings`) opens the settings of the running copy.
- **With the desktop session:** `frigate-pip --autostart on` (off again: `frigate-pip --autostart off`). To stop it: `frigate-pip --quit`.
- **Tray icon:** with the same menu as on Windows (see [The tray icon](#4-the-tray-icon)); its colour shows the state: blue — connected, grey — not connected, amber — paused. It appears on KDE Plasma, Xfce, Cinnamon, MATE and Budgie, and on GNOME with the AppIndicator extension (included in Ubuntu).
- **Mouse:** like on Windows (see [Mouse actions](#mouse-actions)) — × or the right button closes, the speaker mutes, the slider under it sets the volume, ⟲ replays the start of the event, dragging moves the window, the wheel resizes it, Ctrl + wheel zooms the picture around the mouse, Ctrl + drag moves the zoomed picture, a click pins it, a double click opens the camera in Frigate, and a window under the mouse does not close.
- **Window options:** `topmost` (with `false` the windows are ordinary windows that others can cover), `sound` (the desktop’s notification sound) and `skip_fullscreen` (nothing while the active window is full-screen) work like on Windows.
- **Settings:** a click on the icon (or **Settings…** in its menu) opens them in the browser, with the same options, descriptions and connection test as on Windows; the page opens only on this computer. At the first start it opens by itself. They are saved in `~/.config/frigate-pip/config.ini` (described below).

### tmux popups

In this mode FrigatePiP has no window of its own: it waits for detections in the background and, for each one, opens a [tmux](https://github.com/tmux/tmux) popup with the camera over the panes of the tmux client you used last. It needs tmux 3.2 or later and FrigatePiP 1.3.0 or later (older versions do not know `--output tmux` and silently use the terminal mode).

**The popups appear only while you work inside tmux.** tmux can draw them only on its own screen: in a terminal window without tmux — a plain shell, an SSH session outside tmux — nothing is shown and detections are only written to the log. Starting the background process is therefore not enough; you also need a tmux session to work in:

1. Start FrigatePiP once, in a hidden tmux session: `tmux new -d -s frigate-pip 'frigate-pip --output tmux'`.
2. Open tmux for your own work — `tmux` starts a new session — and work there. The popups appear over this window.

- To stop FrigatePiP: `tmux kill-session -t frigate-pip`. To use its keys (pause, show a camera): `tmux attach -t frigate-pip`.
- The popup appears in the chosen **corner** (`position`) and its size follows the **window width** (`width`). It closes by itself after the usual time (`duration`, `max_duration`), or at once with **q**, **x** or **Esc**. **m** turns its sound off and on. **r** replays the start of the event (from the moment the popup opened).
- tmux shows one popup at a time: a detection on another camera replaces the open popup (or is skipped when `replace = false`).
- For the full colours of the picture, tmux must pass 24-bit colour on: add `set -as terminal-features ",xterm-256color:RGB"` to `~/.tmux.conf` (use your `$TERM` outside tmux) and restart tmux. FrigatePiP writes this hint into its log when the colours are missing.

### Keys

When FrigatePiP runs in a terminal or on a console:

| Key | Effect |
|---|---|
| **q** (or Ctrl+C) | Exit. |
| **x** (or Esc) | Close all preview windows. |
| **p** | Pause / resume notifications. |
| **m** | Sound off / on (with `camera_audio = true`) for all windows, also those opened later; the caption then shows *sound off*. |
| **r** | Replay the start of the event in every window (see [Mouse actions](#mouse-actions)); again: back to live video. |
| **1**–**9** | Show camera 1–9 (in the order of the Frigate configuration). |

### Command line

| Option | Effect |
|---|---|
| `--output auto\|window\|framebuffer\|terminal\|tmux` | Where to draw the windows (see the table of modes above). |
| `--graphics auto\|kitty\|sixel\|blocks` | How pictures are drawn in a terminal. |
| `--framebuffer /dev/fb0` | Framebuffer device. |
| `--hwaccel auto\|on\|off` | Video decoding on the graphics card (see `hwaccel` below). |
| `--test <camera>` | Show a camera right after the start. |
| `--test-connection` | Check the connection to Frigate (like **Test connection** on Windows) and exit. |
| `--list` | List the cameras with their objects and zones — the names for `cameras`, `labels` and `zones` in `[trigger]` — and exit. |
| `--autostart on\|off` | Start the desktop windows with the desktop session (an entry in `~/.config/autostart`), or stop doing so. |
| `--quit` | Stop the running desktop windows. |
| `--settings` | Open the settings of the running desktop windows (or start them with the settings open). |
| `--version`, `--help` | Version, short help. |

### Settings

The Linux version uses the same `config.ini` (in `~/.config/frigate-pip/`, or `$XDG_CONFIG_HOME/frigate-pip/`) as the Windows version — see [The settings file](#7-the-settings-file). Changes are applied within two seconds of saving the file. These settings are used: everything in `[app]` (language), `[frigate]`, `[mqtt]`, `[trigger]`, and in `[display]`: `monitor`, `position`, `margin`, `width`, `duration`, `max_duration`, `overlay`, `max_popups`, `replace`, `topmost`, `sound`, `skip_fullscreen`; in `[video]`: `live`, `stream`, `auto_main_width`, `prebuffer` (not for tmux popups), `camera_audio`; and the `[camera.<name>]` sections. The Windows-only settings (`gpu_warm`, `check_updates`) are ignored. `frigate-pip --list` shows the names of your cameras, objects and zones for the filters in `[trigger]`.

Additional settings in the `[console]` section:

| Key | Default | Meaning |
|---|---|---|
| `output` | `auto` | `auto`, `window`, `framebuffer`, `terminal` or `tmux`. |
| `graphics` | `auto` | `auto`, `kitty`, `sixel` or `blocks` (pictures in a terminal). |
| `framebuffer` | `/dev/fb0` | Framebuffer device. |
| `ffmpeg` | `ffmpeg` | ffmpeg program (name or full path). |
| `hwaccel` | `auto` | Video decoding on the graphics card (VAAPI, VDPAU, NVIDIA… whatever ffmpeg finds; without one, on the processor): `auto` = for the main stream, where it saves the most (the first picture of such a window may come about 0.2 s later; the prebuffer hides it), `on` = always, `off` = never. |
| `font` | *(DejaVu Sans Bold)* | TrueType font for captions on the framebuffer. |

The language follows the system locale (`LANG`), unless `language` is set in `[app]`. Camera audio is played through PulseAudio/PipeWire when available, otherwise through ALSA.

### Starting automatically (systemd)

For a monitor showing the console (e.g. a Raspberry Pi without a desktop), FrigatePiP can start as a service when the computer starts.

**Installed with apt (repository or `.deb` file):** the service is included — just enable it for your user (replace `pi` with your user name): `sudo systemctl enable --now frigate-pip@pi`. Its log: `journalctl -u frigate-pip@pi`.

**Installed from the archive:** create `/etc/systemd/system/frigate-pip.service` (replace `pi` with your user name):

```ini
[Unit]
Description=FrigatePiP
After=network-online.target
Wants=network-online.target

[Service]
User=pi
SupplementaryGroups=video audio
ExecStart=/usr/local/bin/frigate-pip --output framebuffer
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Then: `sudo systemctl enable --now frigate-pip`. The log goes to the journal (`journalctl -u frigate-pip`) and to `~/.config/frigate-pip/frigate-pip.log`. To keep the blinking console cursor away from the windows, add `vt.global_cursor_default=0` to the kernel command line (on a Raspberry Pi: `/boot/firmware/cmdline.txt`).

### Linux troubleshooting

- **"cannot start ffmpeg"** — install ffmpeg (`sudo apt install ffmpeg`) or set `ffmpeg` in `[console]`.
- **"/dev/fb0: Permission denied"** — add the user to the `video` group and log in again (for the service: `SupplementaryGroups=video`).
- **No caption on the framebuffer** — install `fonts-dejavu-core` or set `font` in `[console]`.
- **The picture in a terminal is blocky** — that is the half-block mode; use a terminal with Kitty graphics or Sixel (see above) or set `graphics`.
- **No tmux popup appears** — popups show only while you work inside tmux, not in a plain terminal window (see [tmux popups](#tmux-popups)). Check that a tmux client is attached (`tmux list-clients` lists it), that `frigate-pip --version` shows 1.3.0 or later, that tmux is 3.2 or later (`tmux -V`) and that the background process runs (`tmux ls` shows the `frigate-pip` session); the log says why a popup was not shown.
- **Pale colours in tmux popups** — enable 24-bit colour in tmux (see [tmux popups](#tmux-popups)).
- **No sound** — camera audio needs PulseAudio/PipeWire or ALSA; the user of the service must be in the `audio` group.
- **For a detailed report**, start FrigatePiP with `FRIGATEPIP_FFMPEG_LOG=/tmp/ffmpeg.log frigate-pip` and attach the end of that file and of the log to your issue.
- **No desktop windows** — they need X11 or XWayland: `echo $DISPLAY` must show something like `:0`. Without it FrigatePiP uses the console modes; the log says why (`No desktop windows`).
- **FrigatePiP says it is already running** — one copy runs per user, for example the one started with the desktop session; `frigate-pip --quit` stops it.
- **No tray icon on GNOME** — GNOME shows such icons only with the extension “AppIndicator and KStatusNotifierItem Support” (package `gnome-shell-extension-appindicator`; included in Ubuntu). Without it, start FrigatePiP again from the applications menu to open the settings.
