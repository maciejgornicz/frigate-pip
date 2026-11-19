//! The action buttons of the Linux windows: what the settings give a window, and sending a
//! request.

use super::Ev;
use crate::action::{self, Buttons, Look};
use crate::config::{Config, ACTIONS};
use std::sync::mpsc::Sender;
use std::time::Instant;

/// The camera's actions with a URL as a window shows them: (number, label), in number order.
pub fn labels(cfg: &Config, camera: &str) -> Vec<(usize, String)> {
    cfg.actions_for(camera).into_iter().map(|a| (a.n, action::label_or_default(a.label))).collect()
}

/// The window's buttons follow the camera's settings, number by number (see `action::refresh`).
pub fn refresh(buttons: &mut Buttons, cfg: &Config, camera: &str) {
    let mut confirm = [None; ACTIONS];
    for a in cfg.actions_for(camera) {
        if let Some(c) = confirm.get_mut(a.n.wrapping_sub(1)) {
            *c = Some(a.confirm);
        }
    }
    action::refresh(buttons, &confirm);
}

/// Presses action `n` of a window showing `camera`: arms it or sends its request, whose result
/// comes back as `Ev::ActionDone`. Returns the action's look afterwards (`None` = no such
/// action). A request that cannot start counts as failed at once.
pub fn press(buttons: &mut Buttons, n: usize, cfg: &Config, camera: &str, tx: &Sender<Ev>, now: Instant) -> Option<Look> {
    if let Some(ticket) = action::press(buttons, n, now) {
        // Only here does the URL leave the settings, straight to the request's thread.
        let url = cfg.action_url(camera, n).map(str::to_string);
        let tx = tx.clone();
        let done = move |ok: bool| {
            let _ = tx.send(Ev::ActionDone(ticket, ok));
        };
        if !url.is_some_and(|url| action::spawn(url, camera.to_string(), n, done)) {
            if let Some(b) = buttons[n - 1].as_mut() {
                b.finished(ticket, false, now);
            }
        }
    }
    Some(buttons.get(n.checked_sub(1)?)?.as_ref()?.look(now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n;
    use std::time::Duration;

    #[test]
    fn buttons_follow_the_settings() {
        let mut cfg = Config::from_ini(&crate::config::IniDoc::parse(""));
        cfg.set_action("gate", 1, Some("http://h/a".into()), Some("Gate".into()), true);
        cfg.set_action("gate", 3, Some("http://h/b".into()), None, false);
        let mut b: Buttons = Default::default();
        refresh(&mut b, &cfg, "gate");
        assert_eq!(b.iter().map(Option::is_some).collect::<Vec<_>>(), [true, false, true, false]);
        assert_eq!(labels(&cfg, "gate"), [(1, "Gate".to_string()), (3, i18n::t().action_default.to_string())]);
        assert!(b[0].as_ref().unwrap().confirm() && !b[2].as_ref().unwrap().confirm());
        refresh(&mut b, &cfg, "garden");
        assert!(b.iter().all(Option::is_none));
    }

    #[test]
    fn a_press_sends_and_the_result_comes_back_as_an_event() {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/api/webhook/x", l.local_addr().unwrap().port());
        let server = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let (mut got, mut buf) = (Vec::new(), [0u8; 4096]);
            // The head, then the JSON body, which ends the request.
            while !got.ends_with(b"}") {
                match s.read(&mut buf).unwrap() {
                    0 => break,
                    n => got.extend_from_slice(&buf[..n]),
                }
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            String::from_utf8_lossy(&got).to_string()
        });
        let mut cfg = Config::from_ini(&crate::config::IniDoc::parse(""));
        cfg.set_action("gate", 2, Some(url), None, false);
        let mut b: Buttons = Default::default();
        refresh(&mut b, &cfg, "gate");
        let (tx, rx) = std::sync::mpsc::channel();
        assert_eq!(press(&mut b, 2, &cfg, "gate", &tx, Instant::now()), Some(Look::Sending));
        let Ok(Ev::ActionDone(ticket, ok)) = rx.recv_timeout(Duration::from_secs(15)) else { panic!("no result") };
        assert!(ok, "HTTP 200");
        let now = Instant::now();
        assert!(b[1].as_mut().unwrap().finished(ticket, ok, now), "the button that sent it takes the result");
        assert_eq!(b[1].as_ref().unwrap().look(now), Look::Done);
        let req = server.join().unwrap();
        assert!(req.starts_with("POST /api/webhook/x ") && req.ends_with("{\"camera\": \"gate\", \"action\": 2}"), "{req}");
    }

    #[test]
    fn a_press_without_a_url_fails_at_once() {
        let cfg = Config::from_ini(&crate::config::IniDoc::parse(""));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut b: Buttons = Default::default();
        b[1] = Some(action::Button::new(false));
        let t = Instant::now();
        assert_eq!(press(&mut b, 2, &cfg, "gate", &tx, t), Some(Look::Failed), "the settings lost the URL meanwhile");
        assert!(rx.try_recv().is_err(), "nothing was sent");
        assert_eq!(press(&mut b, 1, &cfg, "gate", &tx, t), None, "no such action");
        assert_eq!(press(&mut b, 9, &cfg, "gate", &tx, t), None);
    }
}
