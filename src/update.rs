//! Updates: the latest release on GitHub, download with a checksum check and a
//! silent run of the installer (which closes and restarts the application).
//!
//! HTTPS goes through WinHTTP (part of Windows), so the program needs no TLS library.

use crate::json::Json;
use std::path::PathBuf;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::Networking::WinHttp::*;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const LATEST: &str = "https://api.github.com/repos/maciejgornicz/frigate-pip/releases/latest";
/// Uninstall key written by the installer (`AppId` in installer/frigate-pip.iss).
const UNINSTALL_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{6B0F6C2E-5E7A-4E43-9C55-3A1F0B7D2C91}_is1");

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// Without the leading `v`, e.g. `1.2.0`.
    pub version: String,
    /// Release page on GitHub.
    pub page: String,
    /// Installer: (file name, download address).
    pub installer: Option<(String, String)>,
    /// Address of `SHA256SUMS.txt`.
    pub checksums: Option<String>,
}

/// What the settings window shows about updates.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    Latest,
    Available(String),
    Downloading,
    CheckFailed(String),
    InstallFailed(String),
}

/// Version of the running program (`FRIGATEPIP_FAKE_VERSION` pretends another one, for testing).
pub fn current_version() -> String {
    std::env::var("FRIGATEPIP_FAKE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let core = s.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u32>().ok());
    let major = parts.next()??;
    Some((major, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)))
}

/// `true` if `remote` is a newer version than `current`.
pub fn is_newer(remote: &str, current: &str) -> bool {
    matches!((parse_version(remote), parse_version(current)), (Some(r), Some(c)) if r > c)
}

/// Reads GitHub's "latest release" answer.
pub fn parse_release(j: &Json) -> Option<Release> {
    if j.get("draft").bool() == Some(true) || j.get("prerelease").bool() == Some(true) {
        return None;
    }
    let version = j.get("tag_name").str()?.trim_start_matches(['v', 'V']).to_string();
    parse_version(&version)?;
    let asset = |wanted: &dyn Fn(&str) -> bool| {
        j.get("assets").arr().iter().find_map(|a| {
            let name = a.get("name").str()?;
            let url = a.get("browser_download_url").str()?;
            wanted(name).then(|| (name.to_string(), url.to_string()))
        })
    };
    // The file name becomes a path in %TEMP%: plain characters only.
    let safe = |n: &str| n.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c));
    let installer = asset(&|n| n.starts_with("FrigatePiP-Setup-") && n.ends_with(".exe") && safe(n));
    let checksums = asset(&|n| n == "SHA256SUMS.txt").map(|(_, url)| url);
    Some(Release { version, page: j.get("html_url").str().unwrap_or_default().to_string(), installer, checksums })
}

/// The SHA-256 of `name` from a `SHA256SUMS.txt` file (`<hash>  <name>` lines).
fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let (hash, file) = (parts.next()?, parts.next()?.trim_start_matches('*'));
        (file == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

fn sha256_hex(data: &[u8]) -> String {
    use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
    let mut out = [0u8; 32];
    let _ = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, data, &mut out) };
    out.iter().map(|b| format!("{b:02x}")).collect()
}

/// Asks GitHub for the latest release.
pub fn check() -> Result<Release, String> {
    let body = https_get(LATEST, 1 << 20)?;
    let j = Json::parse(&String::from_utf8_lossy(&body)).ok_or("invalid answer from GitHub")?;
    parse_release(&j).ok_or_else(|| "no release found".into())
}

/// Downloads the installer, checks its SHA-256 and starts it silently. The installer
/// closes FrigatePiP, replaces it and starts the new version (settings are kept).
pub fn install(r: &Release) -> Result<(), String> {
    let (name, url) = r.installer.as_ref().ok_or("the release has no installer")?;
    let sums_url = r.checksums.as_ref().ok_or("the release has no checksums")?;
    let sums = String::from_utf8_lossy(&https_get(sums_url, 64 << 10)?).into_owned();
    let expected = expected_hash(&sums, name).ok_or("the installer is not listed in SHA256SUMS.txt")?;
    let data = https_get(url, 64 << 20)?;
    if sha256_hex(&data) != expected {
        return Err("the downloaded installer does not match its checksum".into());
    }
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, &data).map_err(|e| e.to_string())?;
    crate::log!("Update {}: starting {}", r.version, path.display());
    let started = unsafe {
        ShellExecuteW(
            HWND::default(),
            w!("open"),
            &HSTRING::from(path.as_os_str()),
            w!("/SILENT /SUPPRESSMSGBOXES /NORESTART"),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if started.0 as usize <= 32 {
        return Err(format!("cannot start the installer (error {})", started.0 as usize));
    }
    Ok(())
}

/// `true` if this exe is the copy installed by the installer (then an update runs the
/// installer; a portable copy only opens the release page).
pub fn installed() -> bool {
    let (Ok(exe), Some(dir)) = (std::env::current_exe(), install_location()) else { return false };
    let dir = dir.to_string_lossy().trim_end_matches('\\').to_string();
    exe.parent().is_some_and(|p| p.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(&dir))
}

fn install_location() -> Option<PathBuf> {
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, UNINSTALL_KEY, w!("InstallLocation"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as _), Some(&mut len))
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let n = (len as usize / 2).saturating_sub(1).min(buf.len());
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

/// WinHTTP handle closed on drop.
struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let _ = unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

fn open(h: *mut core::ffi::c_void, what: &str) -> Result<Handle, String> {
    if h.is_null() {
        Err(format!("{what}: {}", windows::core::Error::from_win32()))
    } else {
        Ok(Handle(h))
    }
}

/// HTTPS GET (redirects are followed, the system proxy is used); at most `max` bytes.
fn https_get(url: &str, max: usize) -> Result<Vec<u8>, String> {
    let u = crate::net::Url::parse(url).filter(|u| u.scheme == "https").ok_or("not an https address")?;
    let err = |e: windows::core::Error| e.to_string();
    unsafe {
        let agent = HSTRING::from(format!("FrigatePiP/{}", env!("CARGO_PKG_VERSION")));
        let session = open(WinHttpOpen(&agent, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0), "WinHttpOpen")?;
        let _ = WinHttpSetTimeouts(session.0, 10_000, 10_000, 10_000, 30_000);
        let connect = open(WinHttpConnect(session.0, &HSTRING::from(u.host.as_str()), u.port, 0), "WinHttpConnect")?;
        let path = HSTRING::from(u.path.as_str());
        let request = open(
            WinHttpOpenRequest(connect.0, w!("GET"), &path, PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE),
            "WinHttpOpenRequest",
        )?;
        let headers: Vec<u16> = "Accept: application/vnd.github+json, */*\r\n".encode_utf16().collect();
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).map_err(err)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(err)?;
        let (mut status, mut len) = (0u32, 4u32);
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut _),
            &mut len,
            std::ptr::null_mut(),
        )
        .map_err(err)?;
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        let mut out = Vec::new();
        let mut buf = vec![0u8; 64 << 10];
        loop {
            let mut n = 0u32;
            WinHttpReadData(request.0, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut n).map_err(err)?;
            if n == 0 {
                return Ok(out);
            }
            out.extend_from_slice(&buf[..n as usize]);
            if out.len() > max {
                return Err("the answer is too large".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("1.1.0", "1.0.2"));
        assert!(is_newer("v2.0", "1.9.9"));
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(!is_newer("1.0.2", "1.0.2"));
        assert!(!is_newer("1.0.1", "1.0.2"));
        assert!(!is_newer("garbage", "1.0.0"));
        assert_eq!(parse_version("v1.2.3-beta"), Some((1, 2, 3)));
    }

    #[test]
    fn reads_github_release() {
        let j = Json::parse(
            r#"{"tag_name": "v1.2.0", "html_url": "https://github.com/x/y/releases/tag/v1.2.0", "draft": false, "prerelease": false,
               "assets": [{"name": "frigate-pip.exe", "browser_download_url": "https://e/frigate-pip.exe"},
                          {"name": "FrigatePiP-Setup-1.2.0.exe", "browser_download_url": "https://e/setup.exe"},
                          {"name": "SHA256SUMS.txt", "browser_download_url": "https://e/sums"}]}"#,
        )
        .unwrap();
        let r = parse_release(&j).unwrap();
        assert_eq!(r.version, "1.2.0");
        assert_eq!(r.installer, Some(("FrigatePiP-Setup-1.2.0.exe".into(), "https://e/setup.exe".into())));
        assert_eq!(r.checksums.as_deref(), Some("https://e/sums"));
        let pre = Json::parse(r#"{"tag_name": "v9.0.0", "prerelease": true, "assets": []}"#).unwrap();
        assert_eq!(parse_release(&pre), None);
        let odd = Json::parse(r#"{"tag_name": "v9.0.0", "assets": [{"name": "FrigatePiP-Setup-..\\x.exe", "browser_download_url": "https://e/x"}]}"#)
            .unwrap();
        assert_eq!(parse_release(&odd).unwrap().installer, None);
    }

    #[test]
    fn checksums() {
        let sums = "11f2cec719534fe9ea07ad400115381f8d8e35622a75fb110f95cca8c88aeb66  FrigatePiP-Setup-1.0.1.exe\n\
                    87c321c6fbbffb4b170915b1f5bddd5bf23d4dfbcdd591013e19bfc451921360 *frigate-pip.exe\n";
        assert_eq!(expected_hash(sums, "frigate-pip.exe").as_deref(), Some("87c321c6fbbffb4b170915b1f5bddd5bf23d4dfbcdd591013e19bfc451921360"));
        assert_eq!(expected_hash(sums, "other.exe"), None);
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// Asks the real GitHub API (network needed).
    #[test]
    #[ignore = "requires internet access"]
    fn live_check() {
        let r = check().unwrap();
        println!("{r:?}");
        assert!(r.installer.is_some() && r.checksums.is_some());
    }
}
