use serde::{Deserialize, Serialize};
use std::path::Path;

const KEYCHAIN_SERVICE: &str = "dev.spinup.drupal";
const KEYCHAIN_USER: &str = "default-admin";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub admin_name: String,
    pub admin_email: String,
    pub projects_dir: String,
    pub services: Vec<String>,
    pub config_sync_dir: String,
    pub ide: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        AppSettings {
            admin_name: "admin".into(),
            admin_email: "admin@example.com".into(),
            projects_dir: format!("{home}/Projects"),
            services: vec!["mailpit".into(), "adminer".into()],
            config_sync_dir: "../config/sync".into(),
            ide: "code".into(),
        }
    }
}

impl AppSettings {
    pub fn load(file: &Path) -> AppSettings {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(file, serde_json::to_string_pretty(self).expect("serializable"))
    }
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER).map_err(|e| e.to_string())
}

pub fn set_admin_password(password: &str) -> Result<(), String> {
    entry()?.set_password(password).map_err(|e| e.to_string())
}

pub fn admin_password() -> Option<String> {
    entry().ok()?.get_password().ok()
}
