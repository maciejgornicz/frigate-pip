//! TLS for `https://` and `wss://`: the system's SChannel on Windows, rustls with the
//! Mozilla root certificates on Linux. A certificate that is not publicly valid (e.g. the
//! self-signed one Frigate makes for port 8971) is trusted on first use: its SHA-256
//! fingerprint is kept in `certificates.txt` and any other certificate for that address
//! is refused later.

use std::collections::HashMap;
use std::io;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Mutex;

pub use imp::TlsStream;

/// How the server's certificate was accepted (for the connection test).
#[derive(Clone, Debug, PartialEq)]
pub enum Trust {
    /// Valid for this name under a trusted root certificate.
    Public,
    /// Not publicly valid; trusted now for the first time and remembered.
    FirstUse(String),
    /// Not publicly valid; the same certificate as remembered earlier.
    Remembered(String),
}

/// `host:port` → fingerprint, as in `certificates.txt` (read on first use).
static PINS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
/// The last decision per `host:port`.
static SEEN: Mutex<Option<HashMap<String, Trust>>> = Mutex::new(None);

fn pins_path() -> PathBuf {
    crate::util::app_dir().join("certificates.txt")
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

/// `host:port fingerprint` lines; `#` starts a comment.
fn parse_pins(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once(char::is_whitespace))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_ascii_uppercase()))
        .collect()
}

/// The remembered fingerprint for `key`, or `fp` remembered now (trust on first use).
fn pin_for(key: &str, fp: &str) -> (String, bool) {
    let mut pins = PINS.lock().unwrap();
    let map = pins.get_or_insert_with(|| parse_pins(&std::fs::read_to_string(pins_path()).unwrap_or_default()));
    if let Some(p) = map.get(key) {
        return (p.clone(), false);
    }
    map.insert(key.to_string(), fp.to_string());
    let path = pins_path();
    let mut text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| "# Certificates trusted on first use. Delete a line to trust a new certificate of that address.\n".into());
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("{key} {fp}\n"));
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, text.replace("\r\n", "\n").replace('\n', crate::util::NEWLINE)) {
        crate::log!("Cannot save {}: {e}", path.display());
    }
    crate::log!("TLS {key}: certificate {fp} trusted on first use");
    (fp.to_string(), true)
}

/// Decides about a certificate that did not pass the public check (`fp` = its fingerprint).
fn first_use_or_pinned(key: &str, fp: String) -> Result<(), String> {
    let (pinned, new) = pin_for(key, &fp);
    if pinned == fp {
        seen(key, if new { Trust::FirstUse(fp) } else { Trust::Remembered(fp) });
        Ok(())
    } else {
        Err(format!("the certificate of {key} has changed (now {fp}); if you replaced it, delete its line in {}", pins_path().display()))
    }
}

/// Notes the decision. A certificate first trusted by this run stays "first use" for the
/// connection test, even though the next connection already finds it remembered.
fn seen(key: &str, t: Trust) {
    let mut all = SEEN.lock().unwrap();
    let all = all.get_or_insert_with(HashMap::new);
    if let (Some(Trust::FirstUse(a)), Trust::Remembered(b)) = (all.get(key), &t) {
        if a == b {
            return;
        }
    }
    all.insert(key.to_string(), t);
}

fn key(host: &str, port: u16) -> String {
    format!("{}:{port}", host.to_ascii_lowercase())
}

/// The last decision about the certificate of `host:port` (after a connection).
pub fn trust(host: &str, port: u16) -> Option<Trust> {
    SEEN.lock().unwrap().as_ref()?.get(&key(host, port)).cloned()
}

/// TLS over a connected socket; the handshake is done before returning.
pub fn connect(tcp: TcpStream, host: &str, port: u16) -> io::Result<TlsStream> {
    imp::connect(tcp, host, key(host, port))
}

/// The socket under the TLS stream (timeouts).
pub fn socket(s: &TlsStream) -> &TcpStream {
    imp::socket(s)
}

#[cfg(windows)]
mod imp {
    use schannel::cert_context::HashAlgorithm;
    use schannel::schannel_cred::{Direction, SchannelCred};
    use schannel::tls_stream::{Builder, HandshakeError};
    use std::io;
    use std::net::TcpStream;

    pub type TlsStream = schannel::tls_stream::TlsStream<TcpStream>;

    pub fn connect(tcp: TcpStream, host: &str, key: String) -> io::Result<TlsStream> {
        let cred = SchannelCred::builder().acquire(Direction::Outbound)?;
        let mut b = Builder::new();
        b.domain(host).verify_callback(move |r| {
            if r.result().is_ok() {
                super::seen(&key, super::Trust::Public);
                return Ok(());
            }
            let leaf = r.chain().and_then(|c| c.get(0)).ok_or_else(|| io::Error::other("no server certificate"))?;
            let fp = super::hex(&leaf.fingerprint(HashAlgorithm::sha256())?);
            super::first_use_or_pinned(&key, fp).map_err(|m| io::Error::new(io::ErrorKind::InvalidData, m))
        });
        b.connect(cred, tcp).map_err(|e| match e {
            HandshakeError::Failure(e) if e.kind() == io::ErrorKind::InvalidData => e,
            HandshakeError::Failure(e) => io::Error::new(e.kind(), format!("TLS: {e}")),
            HandshakeError::Interrupted(_) => io::Error::new(io::ErrorKind::TimedOut, "TLS: no answer during the handshake"),
        })
    }

    pub fn socket(s: &TlsStream) -> &TcpStream {
        s.get_ref()
    }
}

#[cfg(not(windows))]
mod imp {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::client::WebPkiServerVerifier;
    use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, RootCertStore, SignatureScheme, StreamOwned};
    use std::io;
    use std::net::TcpStream;
    use std::sync::{Arc, OnceLock};

    pub type TlsStream = StreamOwned<ClientConnection, TcpStream>;

    struct Shared {
        provider: Arc<CryptoProvider>,
        public: Arc<WebPkiServerVerifier>,
    }

    fn shared() -> &'static Shared {
        static S: OnceLock<Shared> = OnceLock::new();
        S.get_or_init(|| {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
            let public = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone()).build().expect("root certificates");
            Shared { provider, public }
        })
    }

    #[derive(Debug)]
    struct Verifier {
        key: String,
        public: Arc<WebPkiServerVerifier>,
        algs: WebPkiSupportedAlgorithms,
    }

    impl ServerCertVerifier for Verifier {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            intermediates: &[CertificateDer<'_>],
            name: &ServerName<'_>,
            ocsp: &[u8],
            now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            if self.public.verify_server_cert(end_entity, intermediates, name, ocsp, now).is_ok() {
                super::seen(&self.key, super::Trust::Public);
                return Ok(ServerCertVerified::assertion());
            }
            let fp = super::hex(ring::digest::digest(&ring::digest::SHA256, end_entity.as_ref()).as_ref());
            super::first_use_or_pinned(&self.key, fp).map(|()| ServerCertVerified::assertion()).map_err(rustls::Error::General)
        }

        fn verify_tls12_signature(
            &self,
            m: &[u8],
            c: &CertificateDer<'_>,
            d: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls12_signature(m, c, d, &self.algs)
        }

        fn verify_tls13_signature(
            &self,
            m: &[u8],
            c: &CertificateDer<'_>,
            d: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls13_signature(m, c, d, &self.algs)
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.algs.supported_schemes()
        }
    }

    pub fn connect(mut tcp: TcpStream, host: &str, key: String) -> io::Result<TlsStream> {
        let s = shared();
        let verifier = Verifier { key, public: s.public.clone(), algs: s.provider.signature_verification_algorithms };
        let config = ClientConfig::builder_with_provider(s.provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(io::Error::other)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        let name =
            ServerName::try_from(host.to_string()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid host name {host}")))?;
        let mut conn = ClientConnection::new(Arc::new(config), name).map_err(io::Error::other)?;
        while conn.is_handshaking() {
            conn.complete_io(&mut tcp).map_err(|e| match e.get_ref().and_then(|i| i.downcast_ref::<rustls::Error>()) {
                Some(rustls::Error::General(m)) => io::Error::new(io::ErrorKind::InvalidData, m.clone()),
                _ => io::Error::new(e.kind(), format!("TLS: {e}")),
            })?;
        }
        Ok(StreamOwned::new(conn, tcp))
    }

    pub fn socket(s: &TlsStream) -> &TcpStream {
        &s.sock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pins() {
        let p = parse_pins("# comment\nFrigate.local:8971 ab:CD\n\n10.0.0.5:443\t01:02 \n");
        assert_eq!(p.get("frigate.local:8971").map(String::as_str), Some("AB:CD"));
        assert_eq!(p.get("10.0.0.5:443").map(String::as_str), Some("01:02"));
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn fingerprints_are_hex_pairs() {
        assert_eq!(hex(&[0xE3, 0x0B, 0x01]), "E3:0B:01");
    }
}
