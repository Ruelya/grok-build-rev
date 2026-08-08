//! Minimal WebDAV client for usage device snapshots (Basic auth).
//!
//! Inspired by cc-switch's transport (`ensure_remote_directories` segment
//! MKCOL + PROPFIND verify), but the **sync model is different**: each device
//! owns `…/devices/<id>/snapshot.json` and clients **add** day×model totals
//! across devices (never overwrite another machine's file).
//!
//! Config file: `~/.grok/usage/sync.toml`
//! ```toml
//! enabled = false
//! auto_sync = true
//! url = "https://dav.example.com"
//! username = "me"
//! password_env = "GROK_USAGE_WEBDAV_PASSWORD"
//! # password = "…"
//! remote_root = "grok-usage-sync"
//! profile = "default"
//! ```
//!
//! Effective path:
//!   `{url}/{remote_root}/{profile}/devices/<id>/snapshot.json`

use super::store::DeviceSnapshot;

/// Default remote folder when the user only sets a host URL.
pub const DEFAULT_REMOTE_ROOT: &str = "grok-usage-sync";
/// Default profile name under `remote_root`.
pub const DEFAULT_PROFILE: &str = "default";

#[derive(Debug, Clone)]
pub struct WebDavConfig {
    /// Full collection base: `url/remote_root/profile` (no trailing slash).
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub auto_sync: bool,
    /// Server origin only (for segment MKCOL from the root).
    pub url: String,
    pub remote_root: String,
    pub profile: String,
}

/// Snapshot of sync settings for the Usage UI (works even when disabled).
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub enabled: bool,
    pub auto_sync: bool,
    pub configured: bool,
    pub url: String,
    pub remote_root: String,
    pub profile: String,
    /// `{url}/{remote_root}/{profile}` for display.
    pub base_path: String,
    /// Absolute path to `sync.toml` for the UI.
    pub config_path: String,
    pub last_synced_at: Option<String>,
    pub last_result: Option<String>,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_sync: true,
            configured: false,
            url: String::new(),
            remote_root: DEFAULT_REMOTE_ROOT.to_string(),
            profile: DEFAULT_PROFILE.to_string(),
            base_path: String::new(),
            config_path: String::new(),
            last_synced_at: None,
            last_result: None,
        }
    }
}

/// Path to `~/.grok/usage/sync.toml`.
pub fn sync_toml_path() -> Option<std::path::PathBuf> {
    xai_grok_config::user_grok_home().map(|h| h.join("usage").join("sync.toml"))
}

fn sync_state_path() -> Option<std::path::PathBuf> {
    xai_grok_config::user_grok_home().map(|h| h.join("usage").join("sync_state.json"))
}

/// Ensure `~/.grok/usage/sync.toml` exists with safe defaults (does not overwrite).
pub fn ensure_default_config_file() {
    let Some(path) = sync_toml_path() else {
        return;
    };
    if path.is_file() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let body = format!(
        r#"# Grok usage WebDAV sync (device day×model snapshots)
# Each machine uploads its own devices/<id>/snapshot.json.
# Clients merge by adding day×model totals across devices.

enabled = false
auto_sync = true

# Server root only — remote_root/profile are appended automatically.
url = "https://dav.example.com"
username = ""
# Prefer env var so the password is not stored in the file:
password_env = "GROK_USAGE_WEBDAV_PASSWORD"
# password = ""

# Default remote folder + profile (like "远程根目录" / "同步配置名")
remote_root = "{DEFAULT_REMOTE_ROOT}"
profile = "{DEFAULT_PROFILE}"
"#
    );
    let _ = std::fs::write(path, body);
}

/// Toggle `enabled` in `sync.toml`, preserving the rest of the file when possible.
pub fn set_enabled(enabled: bool) -> Result<SyncStatus, String> {
    ensure_default_config_file();
    let path = sync_toml_path().ok_or_else(|| "no grok home".to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let new_line = if enabled {
        "enabled = true"
    } else {
        "enabled = false"
    };
    let mut replaced = false;
    let mut out = String::with_capacity(text.len() + 8);
    for line in text.lines() {
        let trimmed = line.trim();
        if !replaced && trimmed.starts_with("enabled") && trimmed.contains('=') {
            out.push_str(new_line);
            out.push('\n');
            replaced = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !replaced {
        // Prepend if the key was missing.
        out = format!("{new_line}\n{out}");
    }
    std::fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(load_sync_status())
}

/// Open `sync.toml` in the OS default editor/file handler.
pub fn open_sync_config() -> bool {
    ensure_default_config_file();
    let Some(path) = sync_toml_path() else {
        return false;
    };
    crate::app::link_opener::open_path(&path)
}

/// Read sync UI status (always; does not require enabled).
pub fn load_sync_status() -> SyncStatus {
    ensure_default_config_file();
    let mut st = SyncStatus::default();
    let Some(path) = sync_toml_path() else {
        return st;
    };
    st.config_path = path.display().to_string();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return st;
    };
    let Ok(table) = toml::from_str::<toml::Table>(&text) else {
        return st;
    };

    st.enabled = table
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    st.auto_sync = table
        .get("auto_sync")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    st.url = table
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .trim_end_matches('/')
        .to_string();
    st.remote_root = table
        .get("remote_root")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_REMOTE_ROOT)
        .trim_matches('/')
        .to_string();
    st.profile = table
        .get("profile")
        .or_else(|| table.get("sync_name"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_PROFILE)
        .trim_matches('/')
        .to_string();
    st.configured = !st.url.is_empty();
    if st.configured {
        st.base_path = join_base(&st.url, &st.remote_root, &st.profile);
    }

    if let Some(sp) = sync_state_path() {
        if let Ok(bytes) = std::fs::read(&sp) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                st.last_synced_at = v
                    .get("last_synced_at")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());
                st.last_result = v
                    .get("last_result")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());
            }
        }
    }
    st
}

/// Resolve password: non-empty `password_env` → env (fallback inline) → else `password`.
pub fn resolve_password(table: &toml::Table) -> String {
    let inline = table
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let env_key = table
        .get("password_env")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if !env_key.is_empty() {
        match std::env::var(env_key) {
            Ok(v) if !v.is_empty() => return v,
            _ => {
                // Env missing/empty → fall back to inline password field.
                return inline;
            }
        }
    }
    inline
}

/// Load live WebDAV client config when sync is enabled.
///
/// - `Ok(None)` — disabled (or not configured and disabled path).
/// - `Ok(Some)` — ready to use.
/// - `Err` — enabled but misconfigured (empty url / username / password).
pub fn load_config() -> Result<Option<WebDavConfig>, String> {
    let st = load_sync_status();
    if !st.enabled {
        return Ok(None);
    }
    if !st.configured {
        return Err("WebDAV enabled but url is empty — edit ~/.grok/usage/sync.toml".into());
    }
    let path = sync_toml_path().ok_or_else(|| "no grok home".to_string())?;
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let table: toml::Table = toml::from_str(&text).map_err(|e| e.to_string())?;

    let username = table
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let password = resolve_password(&table);
    if username.is_empty() {
        return Err("WebDAV enabled but username is empty".into());
    }
    if password.is_empty() {
        return Err(
            "WebDAV password empty — set password= or password_env= in sync.toml".into(),
        );
    }

    Ok(Some(WebDavConfig {
        base_url: st.base_path.clone(),
        username,
        password,
        auto_sync: st.auto_sync,
        url: st.url,
        remote_root: st.remote_root,
        profile: st.profile,
    }))
}

fn join_base(url: &str, remote_root: &str, profile: &str) -> String {
    let mut base = url.trim().trim_end_matches('/').to_string();
    let root = remote_root.trim().trim_matches('/');
    let prof = profile.trim().trim_matches('/');
    if !root.is_empty() {
        base.push('/');
        base.push_str(root);
    }
    if !prof.is_empty() {
        base.push('/');
        base.push_str(prof);
    }
    base
}

/// Split slash path into non-empty segments.
pub fn path_segments(raw: &str) -> Vec<String> {
    raw.trim()
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Build `origin/seg1/seg2/…` without double slashes. Trailing slash optional.
pub fn join_url_segments(origin: &str, segments: &[String], trailing_slash: bool) -> String {
    let mut base = origin.trim().trim_end_matches('/').to_string();
    for seg in segments {
        let s = seg.trim().trim_matches('/');
        if s.is_empty() {
            continue;
        }
        base.push('/');
        base.push_str(s);
    }
    if trailing_slash && !base.ends_with('/') {
        base.push('/');
    }
    base
}

/// Persist last sync timestamp + short result for the Usage modal.
pub fn record_sync_result(ok_note: &str) {
    let Some(path) = sync_state_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let v = serde_json::json!({
        "last_synced_at": now,
        "last_result": ok_note,
        "updated_at_rfc3339": chrono::Utc::now().to_rfc3339(),
    });
    if let Ok(bytes) = serde_json::to_vec_pretty(&v) {
        let _ = std::fs::write(path, bytes);
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

fn method_mkcol() -> reqwest::Method {
    reqwest::Method::from_bytes(b"MKCOL").unwrap_or(reqwest::Method::PUT)
}

fn method_propfind() -> reqwest::Method {
    reqwest::Method::from_bytes(b"PROPFIND").unwrap_or(reqwest::Method::GET)
}

/// Ensure remote directory chain exists (cc-switch style: segment MKCOL + verify).
///
/// Creates: `{url}/{remote_root}/{profile}/…extra` each level with trailing `/`.
pub fn ensure_remote_directories(cfg: &WebDavConfig, extra: &[&str]) -> Result<(), String> {
    let mut segments = path_segments(&cfg.remote_root);
    segments.extend(path_segments(&cfg.profile));
    for s in extra {
        segments.extend(path_segments(s));
    }
    if segments.is_empty() {
        return Ok(());
    }
    let origin = cfg.url.trim().trim_end_matches('/');
    for depth in 1..=segments.len() {
        let dir_url = join_url_segments(origin, &segments[..depth], true);
        mkcol_checked(cfg, &dir_url)?;
    }
    Ok(())
}

fn mkcol_checked(cfg: &WebDavConfig, dir_url: &str) -> Result<(), String> {
    let resp = client()
        .request(method_mkcol(), dir_url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .send()
        .map_err(|e| format!("webdav MKCOL {dir_url}: {e}"))?;
    let status = resp.status();
    let code = status.as_u16();
    if status.is_success() || code == 201 {
        return Ok(());
    }
    // Ambiguous — parent exists / already exists / method not allowed.
    if code == 405 || code == 409 || status.is_redirection() {
        if propfind_exists(cfg, dir_url)? {
            return Ok(());
        }
        return Err(format!(
            "webdav MKCOL {dir_url}: {status} (parent missing or no permission)"
        ));
    }
    if code == 401 || code == 403 {
        return Err(format!(
            "webdav MKCOL {dir_url}: {status} (check username/password)"
        ));
    }
    Err(format!("webdav MKCOL {dir_url}: {status}"))
}

fn propfind_exists(cfg: &WebDavConfig, url: &str) -> Result<bool, String> {
    let resp = client()
        .request(method_propfind(), url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .header("Depth", "0")
        .send()
        .map_err(|e| format!("webdav PROPFIND {url}: {e}"))?;
    let status = resp.status();
    Ok(status.is_success() || status.as_u16() == 207)
}

/// PUT this device's snapshot to `…/devices/<id>/snapshot.json`.
pub fn upload_snapshot(cfg: &WebDavConfig, snap: &DeviceSnapshot) -> Result<(), String> {
    ensure_remote_directories(cfg, &["devices", &snap.device_id])?;
    let url = format!(
        "{}/devices/{}/snapshot.json",
        cfg.base_url.trim_end_matches('/'),
        snap.device_id
    );
    put_bytes(cfg, &url, snap).or_else(|e| {
        // Retry once after re-creating the tree (pCloud 409 when parent raced).
        if e.contains("409") {
            ensure_remote_directories(cfg, &["devices", &snap.device_id])?;
            put_bytes(cfg, &url, snap)
        } else {
            Err(e)
        }
    })
}

fn put_bytes(cfg: &WebDavConfig, url: &str, snap: &DeviceSnapshot) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(snap).map_err(|e| e.to_string())?;
    let resp = client()
        .put(url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .map_err(|e| format!("webdav PUT: {e}"))?;
    let status = resp.status();
    let code = status.as_u16();
    if status.is_success() || code == 201 || code == 204 {
        return Ok(());
    }
    let body_hint = resp.text().unwrap_or_default();
    let hint = body_hint.chars().take(120).collect::<String>();
    if hint.trim().is_empty() {
        Err(format!("webdav PUT {url}: {status}"))
    } else {
        Err(format!("webdav PUT {url}: {status} ({hint})"))
    }
}

/// List `devices/*` via PROPFIND depth 1 and download each other device's snapshot.
pub fn download_all_devices(cfg: &WebDavConfig) -> Result<Vec<DeviceSnapshot>, String> {
    ensure_remote_directories(cfg, &["devices"])?;
    let base = format!("{}/devices", cfg.base_url.trim_end_matches('/'));
    // Prefer trailing slash for collection PROPFIND on picky servers.
    let list_url = if base.ends_with('/') {
        base.clone()
    } else {
        format!("{base}/")
    };

    let propfind = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/></d:prop></d:propfind>"#;
    let resp = client()
        .request(method_propfind(), &list_url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .header("Depth", "1")
        .header(reqwest::header::CONTENT_TYPE, "application/xml")
        .body(propfind)
        .send()
        .map_err(|e| format!("webdav PROPFIND: {e}"))?;

    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() && status.as_u16() != 207 {
        return Err(format!(
            "webdav PROPFIND {list_url}: {status} — cannot list remote devices"
        ));
    }

    let hrefs = extract_hrefs(&text);
    let mut snaps = Vec::new();
    let me = super::store::device_id();
    let mut seen = std::collections::BTreeSet::new();
    for href in hrefs {
        let id = device_id_from_href(&href);
        if id.is_empty() || id == "devices" || id == "snapshot.json" {
            continue;
        }
        if id == me {
            continue; // never pull our own remote file over local scan
        }
        if !seen.insert(id.clone()) {
            continue;
        }
        let url = if href.contains("snapshot.json") {
            absolutize(&cfg.base_url, &href)
        } else {
            format!(
                "{}/devices/{}/snapshot.json",
                cfg.base_url.trim_end_matches('/'),
                id
            )
        };
        match download_one(cfg, &url) {
            Ok(s) => snaps.push(s),
            Err(e) => tracing::debug!(%url, %e, "skip remote snapshot"),
        }
    }
    Ok(snaps)
}

fn download_one(cfg: &WebDavConfig, url: &str) -> Result<DeviceSnapshot, String> {
    let resp = client()
        .get(url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("GET {}: {}", url, resp.status()));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

/// Persist remote snapshots under local `usage/devices/<id>/` for merge.
pub fn cache_remote_snapshots(snaps: &[DeviceSnapshot]) {
    for s in snaps {
        let Some(dir) = super::store::usage_dir() else {
            continue;
        };
        // Never overwrite our own local snapshot from remote.
        if s.device_id == super::store::device_id() {
            continue;
        }
        let path = dir.join("devices").join(&s.device_id).join("snapshot.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(s) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

fn extract_hrefs(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (open, close) in [
        ("<d:href>", "</d:href>"),
        ("<href>", "</href>"),
        ("<D:href>", "</D:href>"),
    ] {
        let mut rest = xml;
        while let Some(i) = rest.find(open) {
            rest = &rest[i + open.len()..];
            if let Some(j) = rest.find(close) {
                let h = rest[..j].trim();
                if !h.is_empty() {
                    out.push(h.to_string());
                }
                rest = &rest[j + close.len()..];
            } else {
                break;
            }
        }
    }
    out
}

fn device_id_from_href(href: &str) -> String {
    let href = href.trim_end_matches('/');
    let parts: Vec<_> = href.split('/').filter(|p| !p.is_empty()).collect();
    if let Some(i) = parts.iter().position(|p| *p == "devices") {
        if let Some(id) = parts.get(i + 1) {
            if *id != "snapshot.json" {
                return (*id).to_string();
            }
        }
    }
    parts.last().copied().unwrap_or("").to_string()
}

fn absolutize(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if let Ok(u) = url::Url::parse(base) {
        if let Ok(joined) = u.join(href) {
            return joined.to_string();
        }
    }
    format!("{}{}", base.trim_end_matches('/'), href)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_segments_splits() {
        assert_eq!(
            path_segments("/a/b/c/"),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert!(path_segments("").is_empty());
        assert_eq!(path_segments("single"), vec!["single".to_string()]);
    }

    #[test]
    fn join_url_segments_no_double_slash() {
        let segs = path_segments("grok-usage-sync/default");
        let u = join_url_segments("https://dav.example.com/", &segs, true);
        assert_eq!(u, "https://dav.example.com/grok-usage-sync/default/");
        assert!(!u.contains("//g"));
    }

    #[test]
    fn resolve_password_prefers_env() {
        let mut table = toml::Table::new();
        table.insert(
            "password".into(),
            toml::Value::String("inline".into()),
        );
        table.insert(
            "password_env".into(),
            toml::Value::String("GROK_TEST_WEBDAV_PW_UNIQUE".into()),
        );
        // Ensure env unset → fallback inline
        // SAFETY: test-only env mutation; unique key avoids colliding with process config.
        unsafe {
            std::env::remove_var("GROK_TEST_WEBDAV_PW_UNIQUE");
        }
        assert_eq!(resolve_password(&table), "inline");
        unsafe {
            std::env::set_var("GROK_TEST_WEBDAV_PW_UNIQUE", "from-env");
        }
        assert_eq!(resolve_password(&table), "from-env");
        unsafe {
            std::env::remove_var("GROK_TEST_WEBDAV_PW_UNIQUE");
        }
    }

    #[test]
    fn resolve_password_inline_only() {
        let mut table = toml::Table::new();
        table.insert(
            "password".into(),
            toml::Value::String("only".into()),
        );
        assert_eq!(resolve_password(&table), "only");
    }

    #[test]
    fn device_id_from_href_parses() {
        assert_eq!(
            device_id_from_href("/grok-usage-sync/default/devices/pc-abc/snapshot.json"),
            "pc-abc"
        );
        assert_eq!(
            device_id_from_href("https://x/devices/pc-abc/"),
            "pc-abc"
        );
    }
}
