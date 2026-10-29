//! Remote updates. Both come from the base URL in Project defaults: `compat.json`
//! for Drupal, PHP and service data, and `latest.json` for the app itself.
//! Nothing is fetched until a URL is set. `compat.json` must come with a
//! `compat.json.sig` made with the app's updater key, the same key that
//! signs app updates.

use crate::compat::Compat;
use base64::Engine;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// HTTPS only, except plain HTTP to this machine for testing a local server.
pub fn endpoint(base: &str, file: &str) -> Result<String, String> {
    let base = base.trim().trim_end_matches('/');
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|l| base == *l || base.starts_with(&format!("{l}:")) || base.starts_with(&format!("{l}/")));
    if base.is_empty() {
        Err("Set an update URL in Project defaults first.".into())
    } else if !(base.starts_with("https://") || local) || base.contains(char::is_whitespace) {
        Err("The update URL must start with https://.".into())
    } else {
        Ok(format!("{base}/{file}"))
    }
}

fn decode_base64(s: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(s.trim()).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Checks a signature from `tauri signer sign` against the updater's public
/// key. Both are base64 minisign text, as in `tauri.conf.json`.
pub fn verify(data: &str, signature: &str, pubkey: &str) -> Result<(), String> {
    let key = decode_base64(pubkey)
        .and_then(|k| minisign_verify::PublicKey::decode(&k).map_err(|e| e.to_string()))
        .map_err(|e| format!("The app's update key is invalid: {e}"))?;
    let unsigned = || "compat.json isn't signed with this app's update key, so it was ignored.".to_string();
    let signature = decode_base64(signature)
        .and_then(|s| minisign_verify::Signature::decode(&s).map_err(|e| e.to_string()))
        .map_err(|_| unsigned())?;
    key.verify(data.as_bytes(), &signature, true).map_err(|_| unsigned())
}

/// Where the signature of a saved compat.json is kept: `compat.json.sig`.
fn signature_file(cache: &Path) -> PathBuf {
    let mut name = cache.as_os_str().to_owned();
    name.push(".sig");
    PathBuf::from(name)
}

/// The compat.json to use: a saved download when it's newer than the bundled
/// copy and still valid and signed, otherwise the bundled one.
pub fn load_compat(cache: &Path, pubkey: &str) -> Compat {
    let bundled = Compat::bundled();
    std::fs::read_to_string(cache)
        .ok()
        .zip(std::fs::read_to_string(signature_file(cache)).ok())
        .filter(|(text, signature)| verify(text, signature, pubkey).is_ok())
        .and_then(|(text, _)| Compat::from_json(&text).ok())
        .filter(|c| c.revision > bundled.revision)
        .unwrap_or(bundled)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatUpdate {
    pub revision: u32,
    pub updated: bool,
}

/// Keeps the download only if it is signed and newer than `current`, and
/// saves it with its signature.
pub fn accept_compat(
    current: &Compat,
    text: &str,
    signature: &str,
    cache: &Path,
    pubkey: &str,
) -> Result<(Compat, CompatUpdate), String> {
    verify(text, signature, pubkey)?;
    let remote = Compat::from_json(text)?;
    if remote.revision <= current.revision {
        return Ok((current.clone(), CompatUpdate { revision: current.revision, updated: false }));
    }
    if let Some(dir) = cache.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(cache, text).map_err(|e| e.to_string())?;
    std::fs::write(signature_file(cache), signature).map_err(|e| e.to_string())?;
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

    /// A throwaway updater key, in the base64 form `tauri signer` uses.
    struct Key(minisign::KeyPair);

    fn base64(s: String) -> String {
        base64::engine::general_purpose::STANDARD.encode(s)
    }

    impl Key {
        fn new() -> Key {
            Key(minisign::KeyPair::generate_unencrypted_keypair().unwrap())
        }

        fn public(&self) -> String {
            base64(self.0.pk.to_box().unwrap().to_string())
        }

        fn sign(&self, text: &str) -> String {
            base64(minisign::sign(Some(&self.0.pk), &self.0.sk, text.as_bytes(), None, None).unwrap().to_string())
        }
    }

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
        let key = Key::new();
        let accept = |text: &str| accept_compat(&bundled, text, &key.sign(text), &cache, &key.public());

        let (same, update) = accept(&with_revision(bundled.revision)).unwrap();
        assert!(!update.updated && !cache.exists());
        assert_eq!(same.revision, bundled.revision);

        let newer = bundled.revision + 1;
        let (c, update) = accept(&with_revision(newer)).unwrap();
        assert_eq!(update, CompatUpdate { revision: newer, updated: true });
        assert_eq!(c.revision, newer);
        assert_eq!(load_compat(&cache, &key.public()).revision, newer, "used on the next launch");

        assert!(accept("{\"revision\": 99}").is_err(), "invalid data is refused");
        assert_eq!(load_compat(&cache, &key.public()).revision, newer, "and doesn't replace the saved copy");
    }

    #[test]
    fn unsigned_or_tampered_downloads_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("compat.json");
        let bundled = Compat::bundled();
        let (key, other) = (Key::new(), Key::new());
        let text = with_revision(bundled.revision + 1);

        for (signature, why) in [
            (other.sign(&text), "signed with another key"),
            (key.sign(&with_revision(bundled.revision + 2)), "signature of different data"),
            ("not a signature".into(), "garbage"),
            (String::new(), "empty"),
        ] {
            let err = accept_compat(&bundled, &text, &signature, &cache, &key.public()).unwrap_err();
            assert!(err.contains("isn't signed"), "{why}: {err}");
            assert!(!cache.exists(), "{why}: nothing saved");
        }
    }

    #[test]
    fn saved_copy_is_checked_again_at_launch() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("compat.json");
        let bundled = Compat::bundled();
        let key = Key::new();
        let text = with_revision(bundled.revision + 1);
        accept_compat(&bundled, &text, &key.sign(&text), &cache, &key.public()).unwrap();
        assert_eq!(load_compat(&cache, &key.public()).revision, bundled.revision + 1);

        assert_eq!(load_compat(&cache, &Key::new().public()).revision, bundled.revision, "another app key");
        std::fs::write(&cache, with_revision(bundled.revision + 5)).unwrap();
        assert_eq!(load_compat(&cache, &key.public()).revision, bundled.revision, "edited on disk");
        std::fs::write(&cache, &text).unwrap();
        std::fs::remove_file(signature_file(&cache)).unwrap();
        assert_eq!(load_compat(&cache, &key.public()).revision, bundled.revision, "saved before signing existed");
    }

    #[test]
    fn the_apps_own_key_decodes() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let pubkey = conf["plugins"]["updater"]["pubkey"].as_str().unwrap();
        let err = verify("{}", &Key::new().sign("{}"), pubkey).unwrap_err();
        assert!(err.contains("isn't signed"), "the key is valid, only the signature is wrong: {err}");
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
        let key = Key::new();
        for text in ["not json".to_string(), with_revision(0)] {
            std::fs::write(&cache, &text).unwrap();
            std::fs::write(signature_file(&cache), key.sign(&text)).unwrap();
            assert_eq!(load_compat(&cache, &key.public()).revision, Compat::bundled().revision);
        }
    }
}
