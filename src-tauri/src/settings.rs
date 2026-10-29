use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub admin_name: String,
    pub admin_email: String,
    pub projects_dir: String,
    pub services: Vec<String>,
    pub config_sync_dir: String,
    pub ide: String,
    /// Base URL for compat.json and app updates. Empty means never check.
    pub update_url: String,
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
            update_url: String::new(),
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

pub fn set_admin_password(file: &Path, password: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(file)?.write_all(password.as_bytes())
}

pub fn admin_password(file: &Path) -> Option<String> {
    std::fs::read_to_string(file).ok().filter(|p| !p.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_password_round_trips_and_stays_private() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("nested/admin-password");
        assert_eq!(admin_password(&file), None);
        set_admin_password(&file, "first").unwrap();
        set_admin_password(&file, "second").unwrap();
        assert_eq!(admin_password(&file).as_deref(), Some("second"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
