//! Frigate's HTTP API and WebSocket at the configured address. With a user name (port 8971,
//! or a reverse proxy in front of it) FrigatePiP logs in at `/api/login` and sends the token
//! it gets as a Bearer header and as the cookie; a refused token is replaced by a new login.
//! Clients made from the same address and login share one token.

use crate::config::Config;
use crate::net::{self, Response};
use crate::util::Cancel;
use crate::ws::WebSocket;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Frigate {
    base: String,
    login: Option<Arc<Login>>,
}

struct Login {
    base: String,
    user: String,
    password: String,
    /// The token, or why the last login failed and until when not to try again.
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    token: Option<String>,
    refused: Option<(Instant, String)>,
}

/// After a refused login, wait this long before trying again (Frigate may limit failures).
const RETRY_LOGIN: Duration = Duration::from_secs(30);

impl Frigate {
    pub fn new(cfg: &Config) -> Frigate {
        let base = cfg.url.trim_end_matches('/').to_string();
        let login = (!cfg.user.is_empty()).then(|| {
            static LOGINS: Mutex<Vec<Arc<Login>>> = Mutex::new(Vec::new());
            let mut all = LOGINS.lock().unwrap();
            if let Some(l) = all.iter().find(|l| l.base == base && l.user == cfg.user && l.password == cfg.password) {
                return l.clone();
            }
            if all.len() >= 8 {
                all.remove(0);
            }
            let l = Arc::new(Login { base: base.clone(), user: cfg.user.clone(), password: cfg.password.clone(), state: Mutex::default() });
            all.push(l.clone());
            l
        });
        Frigate { base, login }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// The address for `/ws` (ws:// or wss://).
    pub fn ws_url(&self) -> String {
        let b = self.base.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
        format!("{b}/ws")
    }

    /// Whether requests need FrigatePiP's own client (HTTPS or a login) rather than a plain
    /// HTTP GET by another program (ffmpeg on Linux).
    #[cfg(target_os = "linux")]
    pub fn needs_client(&self) -> bool {
        self.login.is_some() || !self.base.starts_with("http://")
    }

    pub fn user(&self) -> Option<&str> {
        self.login.as_ref().map(|l| l.user.as_str())
    }

    /// GET of an API path (e.g. `/api/config`).
    pub fn get(&self, path: &str, timeout: Duration, cancel: Option<&Cancel>) -> io::Result<Response> {
        let url = self.url(path);
        let Some(l) = &self.login else { return net::get(&url, timeout, cancel) };
        let mut renewed = false;
        loop {
            let (token, had) = l.token(timeout, cancel)?;
            let resp = net::request("GET", &url, &auth_headers(&token), &[], timeout, cancel)?;
            if resp.status == 401 && had && !renewed {
                // The token expired or was revoked (e.g. a changed password): log in again once.
                l.forget(&token);
                renewed = true;
                continue;
            }
            if let Some(t) = cookie_token(&resp.headers) {
                l.state.lock().unwrap().token = Some(t);
            }
            return Ok(resp);
        }
    }

    /// GET of the whole body; error if the status ≠ 200.
    pub fn get_bytes(&self, path: &str, timeout: Duration, cancel: Option<&Cancel>, limit: usize) -> io::Result<Vec<u8>> {
        let resp = self.get(path, timeout, cancel)?;
        if resp.status != 200 {
            return Err(io::Error::other(format!("HTTP {} for {}", resp.status, self.url(path))));
        }
        resp.read_all(limit)
    }

    /// Frigate's WebSocket (`/ws`).
    pub fn websocket(&self, timeout: Duration, read_timeout: Duration, cancel: &Cancel) -> io::Result<WebSocket> {
        let url = self.ws_url();
        let Some(l) = &self.login else { return WebSocket::connect(&url, &[], timeout, read_timeout, cancel) };
        let mut renewed = false;
        loop {
            let (token, had) = l.token(timeout, Some(cancel))?;
            match WebSocket::connect(&url, &auth_headers(&token), timeout, read_timeout, cancel) {
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied && had && !renewed => {
                    l.forget(&token);
                    renewed = true;
                }
                r => return r,
            }
        }
    }
}

impl Login {
    /// The current token (and whether it was already there), logging in when there is none.
    /// The lock is held during the login so that parallel requests log in only once.
    fn token(&self, timeout: Duration, cancel: Option<&Cancel>) -> io::Result<(String, bool)> {
        let mut st = self.state.lock().unwrap();
        if let Some(t) = &st.token {
            return Ok((t.clone(), true));
        }
        if let Some((at, why)) = &st.refused {
            if at.elapsed() < RETRY_LOGIN {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, why.clone()));
            }
        }
        match self.log_in(timeout, cancel) {
            Ok(t) => {
                *st = State { token: Some(t.clone()), refused: None };
                Ok((t, false))
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    st.refused = Some((Instant::now(), e.to_string()));
                }
                Err(e)
            }
        }
    }

    fn forget(&self, token: &str) {
        let mut st = self.state.lock().unwrap();
        if st.token.as_deref() == Some(token) {
            st.token = None;
        }
    }

    fn log_in(&self, timeout: Duration, cancel: Option<&Cancel>) -> io::Result<String> {
        let body = format!("{{\"user\": {}, \"password\": {}}}", crate::json::quote(&self.user), crate::json::quote(&self.password));
        let url = format!("{}/api/login", self.base);
        let resp = net::request("POST", &url, &[("Content-Type", "application/json".into())], body.as_bytes(), timeout, cancel)?;
        match (resp.status, cookie_token(&resp.headers)) {
            (200, Some(t)) => {
                crate::log!("Logged in to Frigate as {}", self.user);
                Ok(t)
            }
            (200, None) => Err(io::Error::other("Frigate accepted the login but sent no token")),
            (400 | 401 | 403, _) => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Frigate refused the login of {}: wrong user name or password", self.user),
            )),
            (429, _) => Err(io::Error::new(io::ErrorKind::PermissionDenied, "Frigate refuses logins for a while after too many failed ones")),
            (404, _) => Err(io::Error::other("this address has no login (HTTP 404 for /api/login); port 5000 needs no user name")),
            (s, _) => Err(io::Error::other(format!("login: HTTP {s}"))),
        }
    }
}

fn auth_headers(token: &str) -> [(&'static str, String); 2] {
    [("Authorization", format!("Bearer {token}")), ("Cookie", format!("frigate_token={token}"))]
}

/// The JWT from a `Set-Cookie` header (Frigate's cookie is `frigate_token` by default; its
/// name can be changed, so any cookie holding a JWT counts).
fn cookie_token(headers: &[(String, String)]) -> Option<String> {
    let jwt = |v: &str| v.starts_with("eyJ") && v.matches('.').count() == 2;
    headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("Set-Cookie"))
        .filter_map(|(_, v)| v.split(';').next()?.split_once('='))
        .map(|(name, value)| (name.trim(), value.trim()))
        .find(|(name, value)| !value.is_empty() && (*name == "frigate_token" || jwt(value)))
        .map(|(_, value)| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IniDoc;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    fn h(k: &str, v: &str) -> (String, String) {
        (k.into(), v.into())
    }

    #[test]
    fn finds_the_token_cookie() {
        let hs = [h("Set-Cookie", "other=1; Path=/"), h("set-cookie", "frigate_token=eyJa.b.c; Path=/; HttpOnly")];
        assert_eq!(cookie_token(&hs).as_deref(), Some("eyJa.b.c"));
        assert_eq!(cookie_token(&[h("Set-Cookie", "my_jwt=eyJx.y.z")]).as_deref(), Some("eyJx.y.z"));
        assert_eq!(cookie_token(&[h("Set-Cookie", "frigate_token=; Max-Age=0")]), None);
    }

    #[test]
    fn ws_address_follows_the_scheme() {
        let c = |url: &str| {
            let mut cfg = Config::from_ini(&IniDoc::parse(""));
            cfg.url = url.into();
            Frigate::new(&cfg).ws_url()
        };
        assert_eq!(c("http://f:5000/"), "ws://f:5000/ws");
        assert_eq!(c("https://f:8971"), "wss://f:8971/ws");
    }

    /// A Frigate that wants a login: `/api/login` gives token `eyJt.1.x` (then `.2.`…), and an
    /// API request with an older token is refused once.
    fn fake_frigate() -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", l.local_addr().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        std::thread::spawn(move || {
            let mut issued = 0;
            for s in l.incoming() {
                let mut s = s.unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                r.read_line(&mut first).unwrap();
                let (mut auth, mut len) = (String::new(), 0usize);
                loop {
                    let mut line = String::new();
                    r.read_line(&mut line).unwrap();
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    let (k, v) = line.split_once(": ").unwrap();
                    match k.to_ascii_lowercase().as_str() {
                        "authorization" => auth = v.to_string(),
                        "content-length" => len = v.parse().unwrap(),
                        _ => {}
                    }
                }
                let mut body = vec![0u8; len];
                r.read_exact(&mut body).unwrap();
                let path = first.split_whitespace().nth(1).unwrap().to_string();
                seen.lock().unwrap().push(format!("{path} {auth}"));
                let reply = if path == "/api/login" {
                    if String::from_utf8_lossy(&body).contains("\"secret\"") {
                        issued += 1;
                        format!("HTTP/1.1 200 OK\r\nSet-Cookie: frigate_token=eyJt.{issued}.x; Path=/\r\nContent-Length: 2\r\n\r\n{{}}")
                    } else {
                        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_string()
                    }
                } else if auth == format!("Bearer eyJt.{issued}.x") {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_string()
                } else {
                    "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_string()
                };
                s.write_all(reply.as_bytes()).unwrap();
            }
        });
        (addr, log)
    }

    #[test]
    fn logs_in_shares_the_token_and_renews_it() {
        let (addr, log) = fake_frigate();
        let mut cfg = Config::from_ini(&IniDoc::parse(""));
        cfg.url = addr.clone();
        cfg.user = "viewer".into();
        cfg.password = "secret".into();
        let t = Duration::from_secs(2);
        let a = Frigate::new(&cfg);
        assert_eq!(a.get_bytes("/api/version", t, None, 100).unwrap(), b"ok");
        // A second client with the same settings uses the same token: no new login.
        assert_eq!(Frigate::new(&cfg).get_bytes("/api/config", t, None, 100).unwrap(), b"ok");
        // The server forgets the token (as after a restart): one new login, then success.
        *a.login.as_ref().unwrap().state.lock().unwrap() = State { token: Some("eyJold.0.x".into()), refused: None };
        assert_eq!(a.get_bytes("/api/stats", t, None, 100).unwrap(), b"ok");
        let paths: Vec<String> = log.lock().unwrap().iter().map(|l| l.split(' ').next().unwrap().to_string()).collect();
        assert_eq!(paths, ["/api/login", "/api/version", "/api/config", "/api/stats", "/api/login", "/api/stats"]);
        assert!(log.lock().unwrap()[1].ends_with("Bearer eyJt.1.x"));

        // A wrong password is reported, and not retried at once.
        cfg.password = "wrong".into();
        let b = Frigate::new(&cfg);
        let e = b.get("/api/version", t, None).err().unwrap();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        let logins = log.lock().unwrap().len();
        assert!(b.get("/api/version", t, None).is_err());
        assert_eq!(log.lock().unwrap().len(), logins);
    }
}
