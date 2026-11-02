//! tmux output: FrigatePiP runs in the background and opens each preview window as a
//! tmux popup (`display-popup`) over whatever the user is doing in tmux. The popup runs
//! FrigatePiP itself in viewer mode (`--view`), which decodes and draws the camera.
//!
//! A lock file held by the viewer tells the background process when the user closed
//! the popup (q, x, Esc).

use super::canvas::Image;
use super::{Output, Rect, View};
use crate::config::Corner;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

/// Nominal terminal cell (pixels) used to turn the window width into columns.
const CELL: (u32, u32) = (9, 18);

struct Popup {
    client: String,
    lock: PathBuf,
    opened: Instant,
    /// The viewer has taken its lock at least once.
    seen: bool,
}

pub struct Tmux {
    exe: PathBuf,
    width: u32,
    corner: Corner,
    popups: HashMap<u64, Popup>,
    lock_dir: PathBuf,
    /// The hint about 24-bit colour was written to the log.
    rgb_hint: bool,
}

/// Single-quotes an argument for the shell that tmux runs the popup command with.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// `true` if another process (the viewer) holds the lock on `path`.
fn lock_held(path: &PathBuf) -> bool {
    let Ok(f) = File::open(path) else { return false };
    let fd = f.as_raw_fd();
    unsafe {
        if libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) == 0 {
            libc::flock(fd, libc::LOCK_UN);
            false
        } else {
            true
        }
    }
}

/// Viewer side: takes the lock for as long as the returned file lives.
pub fn hold_lock(path: &str) -> Option<File> {
    let f = OpenOptions::new().create(true).truncate(false).write(true).open(path).ok()?;
    (unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } == 0).then_some(f)
}

/// The most recently active tmux client: (name, columns, rows, 24-bit colour).
fn active_client() -> Option<(String, u32, u32, bool)> {
    let format = "#{client_activity} #{client_name} #{client_width} #{client_height} #{client_termfeatures}";
    let out = Command::new("tmux").args(["list-clients", "-F", format]).output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.split_whitespace().collect();
            let rgb = p.get(4).is_some_and(|f| f.split(',').any(|x| x == "RGB"));
            Some((p.first()?.parse::<u64>().ok()?, p.get(1)?.to_string(), p.get(2)?.parse().ok()?, p.get(3)?.parse().ok()?, rgb))
        })
        .max_by_key(|c| c.0)
        .map(|(_, name, w, h, rgb)| (name, w, h, rgb))
}

/// Popup size and position in cells for a window `width` pixels wide: (w, h, x, y)
/// for `display-popup` (border included; R = right edge, S = above the status line).
fn geometry(width: u32, aspect: f32, corner: Corner, client: (u32, u32)) -> (u32, u32, String, String) {
    let cols = (width / CELL.0).clamp(20, client.0.saturating_sub(4).max(20));
    // Half blocks: each cell shows two pixels one above the other, so a pixel is about square.
    let rows = ((cols as f32 / aspect / 2.0).round() as u32).max(4) + 1;
    let (w, h) = (cols + 2, (rows + 2).min(client.1.saturating_sub(2).max(6)));
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => "1".to_string(),
        Corner::TopRight | Corner::BottomRight => client.0.saturating_sub(w + 1).to_string(),
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => "0".to_string(),
        Corner::BottomLeft | Corner::BottomRight => "S".to_string(),
    };
    (w, h, x, y)
}

impl Tmux {
    pub fn new(width: u32, corner: Corner) -> Result<Tmux, String> {
        let ok = Command::new("tmux").arg("-V").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        if !ok {
            return Err("tmux is not installed (e.g. sudo apt install tmux)".into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let lock_dir = base.join(format!("frigate-pip-{}", std::process::id()));
        std::fs::create_dir_all(&lock_dir).map_err(|e| format!("{}: {e}", lock_dir.display()))?;
        crate::log!("tmux output: popups from {}", exe.display());
        Ok(Tmux { exe, width, corner, popups: HashMap::new(), lock_dir, rgb_hint: false })
    }

    fn close(&mut self, id: u64, popup: Popup) {
        if lock_held(&popup.lock) {
            let _ = Command::new("tmux").args(["display-popup", "-C", "-c", &popup.client]).stderr(Stdio::null()).status();
        }
        let _ = std::fs::remove_file(&popup.lock);
        crate::log!("tmux popup {id} closed");
    }
}

impl Output for Tmux {
    fn size(&self) -> (u32, u32) {
        (u32::MAX / 4, u32::MAX / 4)
    }

    fn draw(&mut self, _id: u64, _rect: Rect, _img: Image, _caption: &str, _live: bool) {}

    fn clear(&mut self, id: u64) {
        if let Some(p) = self.popups.remove(&id) {
            self.close(id, p);
        }
    }

    fn configure(&mut self, cfg: &crate::config::Config) {
        self.width = cfg.width.max(64) as u32;
        self.corner = cfg.position;
    }

    fn max_windows(&self) -> Option<usize> {
        // tmux shows one popup per client.
        Some(1)
    }

    fn open_external(&mut self, id: u64, view: &View) -> Option<bool> {
        let Some((client, cw, ch, rgb)) = active_client() else {
            crate::log!("{}: no tmux client attached - nothing shown", view.camera);
            return Some(false);
        };
        if !rgb && !self.rgb_hint {
            self.rgb_hint = true;
            crate::log!("tmux passes no 24-bit colour to {client}: for a better picture add to ~/.tmux.conf: set -as terminal-features \",xterm-256color:RGB\" (your $TERM outside tmux) and restart tmux");
        }
        let (w, h, x, y) = geometry(self.width, view.aspect, self.corner, (cw, ch));
        let lock = self.lock_dir.join(format!("view-{id}.lock"));
        let mut cmd = vec![quote(&self.exe.to_string_lossy()), "--view".into(), quote(&view.camera)];
        if let Some(s) = &view.stream {
            cmd.extend(["--view-stream".into(), quote(s)]);
        }
        cmd.extend(["--view-aspect".into(), format!("{:.4}", view.aspect)]);
        cmd.extend(["--view-caption".into(), quote(&view.caption)]);
        cmd.extend(["--view-lock".into(), quote(&lock.to_string_lossy())]);
        if view.muted {
            cmd.push("--view-muted".into());
        }
        if view.main {
            cmd.push("--view-main".into());
        }
        let spawned = Command::new("tmux")
            .args(["display-popup", "-c", &client, "-w", &w.to_string(), "-h", &h.to_string(), "-x", &x, "-y", &y, "-E", &cmd.join(" ")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Ok(mut child) => {
                // Reap the tmux command when it finishes.
                let _ = std::thread::Builder::new().name("tmux-popup".into()).spawn(move || {
                    let _ = child.wait();
                });
                crate::log!("{}: tmux popup {w}x{h} at {x},{y} on {client}", view.camera);
                self.popups.insert(id, Popup { client, lock, opened: Instant::now(), seen: false });
                Some(true)
            }
            Err(e) => {
                crate::log!("{}: cannot run tmux: {e}", view.camera);
                Some(false)
            }
        }
    }

    fn alive(&mut self, id: u64) -> bool {
        let Some(p) = self.popups.get_mut(&id) else { return false };
        if lock_held(&p.lock) {
            p.seen = true;
            return true;
        }
        // Not started yet (a few seconds of grace) or closed by the user.
        let alive = !p.seen && p.opened.elapsed().as_secs() < 5;
        if !alive {
            if let Some(p) = self.popups.remove(&id) {
                let _ = std::fs::remove_file(&p.lock);
                crate::log!("tmux popup {id} closed by the user");
            }
        }
        alive
    }
}

impl Drop for Tmux {
    fn drop(&mut self) {
        let ids: Vec<u64> = self.popups.keys().copied().collect();
        for id in ids {
            self.clear(id);
        }
        let _ = std::fs::remove_dir(&self.lock_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quoting() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn popup_geometry() {
        // 480 px → 53 columns; 16:9 → 15 picture rows + caption, plus the border.
        assert_eq!(geometry(480, 16.0 / 9.0, Corner::BottomRight, (200, 50)), (55, 18, "144".into(), "S".into()));
        assert_eq!(geometry(480, 16.0 / 9.0, Corner::TopLeft, (200, 50)), (55, 18, "1".into(), "0".into()));
        // Never larger than the client.
        let (w, h, _, _) = geometry(4000, 1.0, Corner::TopLeft, (80, 24));
        assert!(w <= 80 && h <= 22, "{w}x{h}");
    }
}
