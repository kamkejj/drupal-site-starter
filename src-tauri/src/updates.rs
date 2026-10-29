//! Remote updates. Both come from the base URL in Settings: `compat.json`
//! for Drupal, PHP and service data, and `latest.json` for the app itself.
//! Nothing is fetched until a URL is set.

use crate::compat::Compat;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

/// HTTPS only, except plain HTTP to this machine for testing a local server.
pub fn endpoint(base: &str, file: &str) -> Result<String, String> {
    let base = base.trim().trim_end_matches('/');
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|l| base == *l || base.starts_with(&format!("{l}:")) || base.starts_with(&format!("{l}/")));
    if base.is_empty() {
        Err("Set an update URL in Settings first.".into())
    } else if !(base.starts_with("https://") || local) || base.contains(char::is_whitespace) {
        Err("The update URL must start with https://.".into())
    } else {
        Ok(format!("{base}/{file}"))
    }
}

/// The compat.json to use: a saved download when it's newer than the bundled
/// copy and still valid, otherwise the bundled one.
pub fn load_compat(cache: &Path) -> Compat {
    let bundled = Compat::bundled();
    std::fs::read_to_string(cache)
        .ok()
        .and_then(|text| Compat::from_json(&text).ok())
        .filter(|c| c.revision > bundled.revision)
        .unwrap_or(bundled)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatUpdate {
    pub revision: u32,
    pub updated: bool,
}

/// Keeps `downloaded` only if it is newer than `current`, and saves it.
pub fn accept_compat(current: &Compat, text: &str, cache: &Path) -> Result<(Compat, CompatUpdate), String> {
    let remote = Compat::from_json(text)?;
    if remote.revision <= current.revision {
        return Ok((current.clone(), CompatUpdate { revision: current.revision, updated: false }));
    }
    if let Some(dir) = cache.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(cache, text).map_err(|e| e.to_string())?;
    let revision = remote.revision;
    Ok((remote, CompatUpdate { revision, updated: true }))
}

pub async fn download(url: &str) -> Result<String, String> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(url).send().await.map_err(|e| format!("Could not reach {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url} answered {}", response.status()));
    }
    response.text().await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_revision(revision: u32) -> String {
        let mut v: serde_json::Value = serde_json::from_str(include_str!("../resources/compat.json")).unwrap();
        v["revision"] = revision.into();
        v.to_string()
    }

    #[test]
    fn endpoint_needs_https_except_on_this_machine() {
        assert_eq!(endpoint("https://example.com/drupal/", "compat.json").unwrap(), "https://example.com/drupal/compat.json");
        assert_eq!(endpoint("http://127.0.0.1:8080", "latest.json").unwrap(), "http://127.0.0.1:8080/latest.json");
        assert!(endpoint("http://example.com", "compat.json").is_err());
        assert!(endpoint("http://localhost.evil.com", "compat.json").is_err());
        assert!(endpoint("", "compat.json").is_err());
    }

    #[test]
    fn newer_download_replaces_and_persists_older_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache/compat.json");
        let bundled = Compat::bundled();

        let (same, update) = accept_compat(&bundled, &with_revision(bundled.revision), &cache).unwrap();
        assert!(!update.updated && !cache.exists());
        assert_eq!(same.revision, bundled.revision);

        let newer = bundled.revision + 1;
        let (c, update) = accept_compat(&bundled, &with_revision(newer), &cache).unwrap();
        assert_eq!(update, CompatUpdate { revision: newer, updated: true });
        assert_eq!(c.revision, newer);
        assert_eq!(load_compat(&cache).revision, newer, "used on the next launch");

        assert!(accept_compat(&bundled, "{\"revision\": 99}", &cache).is_err(), "invalid data is refused");
        assert_eq!(load_compat(&cache).revision, newer, "and doesn't replace the saved copy");
    }

    /// Serves one HTTP response on a local port and returns its base URL.
    fn serve_once(status: &str, body: String) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let status = status.to_string();
        std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let _ = conn.read(&mut [0; 4096]);
            let _ = write!(conn, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        });
        base
    }

    #[tokio::test]
    async fn downloads_compat_over_http() {
        let base = serve_once("200 OK", with_revision(7));
        let text = download(&endpoint(&base, "compat.json").unwrap()).await.unwrap();
        assert_eq!(Compat::from_json(&text).unwrap().revision, 7);

        let base = serve_once("404 Not Found", String::new());
        assert!(download(&endpoint(&base, "compat.json").unwrap()).await.unwrap_err().contains("404"));
    }

    #[test]
    fn stale_or_broken_cache_falls_back_to_bundled() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("compat.json");
        std::fs::write(&cache, "not json").unwrap();
        assert_eq!(load_compat(&cache).revision, Compat::bundled().revision);
        std::fs::write(&cache, with_revision(0)).unwrap();
        assert_eq!(load_compat(&cache).revision, Compat::bundled().revision);
    }
}
