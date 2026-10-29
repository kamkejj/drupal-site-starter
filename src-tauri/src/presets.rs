//! Saved presets: the reusable parts of a project, without its name, folder
//! or admin account. Stored as JSON; imported and exported as JSON or YAML.

use crate::compat::Database;
use crate::config::{Ci, Webserver};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub drupal: String,
    pub php: String,
    pub webserver: Webserver,
    pub database: Database,
    pub services: Vec<String>,
    #[serde(default)]
    pub bundles: Vec<String>,
    #[serde(default)]
    pub recipes: Vec<String>,
    #[serde(default)]
    pub extra_modules: Vec<String>,
    #[serde(default)]
    pub ci: Ci,
    pub config_sync_dir: String,
    pub export_config_on_install: bool,
    pub init_git: bool,
}

fn is_yaml(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()), Some("yaml" | "yml"))
}

pub fn load(file: &Path) -> Vec<Preset> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(file: &Path, presets: &[Preset]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, serde_json::to_string_pretty(presets).expect("serializable")).map_err(|e| e.to_string())
}

/// Adds each preset, replacing one with the same name, and keeps them sorted.
pub fn merge(into: &mut Vec<Preset>, incoming: Vec<Preset>) {
    for p in incoming {
        into.retain(|q| q.name != p.name);
        into.push(p);
    }
    into.sort_by_key(|p| p.name.to_lowercase());
}

/// Parses an import file. Release, PHP and database values are checked
/// against compat.json later, when the wizard plans a project from them.
pub fn read(path: &Path) -> Result<Vec<Preset>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (presets, raw): (Vec<Preset>, Vec<serde_json::Value>) = if is_yaml(path) {
        (serde_saphyr::from_str(&text).map_err(|e| e.to_string())?, serde_saphyr::from_str(&text).map_err(|e| e.to_string())?)
    } else {
        (serde_json::from_str(&text).map_err(|e| e.to_string())?, serde_json::from_str(&text).map_err(|e| e.to_string())?)
    };
    let presets: Vec<Preset> = presets.into_iter().map(|p| Preset { name: p.name.trim().to_string(), ..p }).collect();
    if presets.iter().any(|p| p.name.is_empty()) {
        return Err("Every preset needs a name.".into());
    }
    for (p, raw) in presets.iter().zip(&raw) {
        let known = serde_json::to_value(p).expect("serializable");
        if let Some(field) = unknown_field(raw, &known, "") {
            return Err(format!("Preset \"{}\" has an unknown field \"{field}\".", p.name));
        }
    }
    Ok(presets)
}

/// The first key in `raw` that a saved preset doesn't have, such as a
/// misspelling that serde would otherwise skip. Nested keys come back as
/// `database.typ`. Unknown keys aren't rejected when loading presets.json,
/// so a file written by a newer version still loads.
fn unknown_field(raw: &serde_json::Value, known: &serde_json::Value, prefix: &str) -> Option<String> {
    let (raw, known) = (raw.as_object()?, known.as_object()?);
    raw.iter().find_map(|(key, value)| match known.get(key) {
        None => Some(format!("{prefix}{key}")),
        Some(k) => unknown_field(value, k, &format!("{prefix}{key}.")),
    })
}

pub fn write(path: &Path, presets: &[Preset]) -> Result<(), String> {
    let text = if is_yaml(path) {
        serde_saphyr::to_string(&presets).map_err(|e| e.to_string())?
    } else {
        serde_json::to_string_pretty(presets).expect("serializable")
    };
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::DbType;

    fn preset(name: &str, php: &str) -> Preset {
        Preset {
            name: name.into(),
            drupal: "11".into(),
            php: php.into(),
            webserver: Webserver::NginxFpm,
            database: Database { kind: DbType::Postgres, version: "17".into() },
            services: vec!["redis".into(), "solr".into()],
            bundles: vec!["seo".into()],
            recipes: vec![],
            extra_modules: vec!["media".into()],
            ci: Ci::Gitlab,
            config_sync_dir: "../config/sync".into(),
            export_config_on_install: true,
            init_git: false,
        }
    }

    #[test]
    fn exports_and_imports_in_both_formats() {
        let tmp = tempfile::tempdir().unwrap();
        let presets = vec![preset("Agency", "8.3"), preset("Search", "8.4")];
        for file in ["presets.json", "presets.yaml", "presets.yml"] {
            let path = tmp.path().join(file);
            write(&path, &presets).unwrap();
            assert_eq!(read(&path).unwrap(), presets, "{file} round-trips");
        }
        let yaml = std::fs::read_to_string(tmp.path().join("presets.yaml")).unwrap();
        assert!(yaml.contains("webserver: nginx-fpm") && yaml.contains("type: postgres"), "YAML is readable:\n{yaml}");
    }

    #[test]
    fn merge_replaces_same_name_and_sorts() {
        let mut saved = vec![preset("search", "8.3"), preset("Agency", "8.3")];
        merge(&mut saved, vec![preset("search", "8.4")]);
        let summary: Vec<(&str, &str)> = saved.iter().map(|p| (p.name.as_str(), p.php.as_str())).collect();
        assert_eq!(summary, [("Agency", "8.3"), ("search", "8.4")]);
    }

    #[test]
    fn import_rejects_unnamed_and_malformed_presets() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        write(&path, &[preset("  ", "8.3")]).unwrap();
        assert!(read(&path).is_err(), "blank name");
        std::fs::write(&path, r#"[{"name": "x"}]"#).unwrap();
        assert!(read(&path).is_err(), "missing fields");
    }

    #[test]
    fn import_rejects_misspelled_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.yaml");
        write(&path, &[preset("Agency", "8.3")]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace("bundles:", "bundle:")).unwrap();
        assert_eq!(read(&path).unwrap_err(), r#"Preset "Agency" has an unknown field "bundle"."#);
        std::fs::write(&path, text.replace("version:", "versoin:").replace("type: postgres", "type: postgres\n    version: '17'")).unwrap();
        assert_eq!(read(&path).unwrap_err(), r#"Preset "Agency" has an unknown field "database.versoin"."#);
    }

    #[test]
    fn presets_saved_before_bundles_still_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("old.json");
        std::fs::write(&path, r#"[{"name":"Old","drupal":"11","php":"8.3","webserver":"nginx-fpm","database":{"type":"mariadb","version":"10.11"},"services":[],"configSyncDir":"../config/sync","exportConfigOnInstall":true,"initGit":true}]"#).unwrap();
        let p = &read(&path).unwrap()[0];
        assert!(p.bundles.is_empty() && p.recipes.is_empty() && p.extra_modules.is_empty() && p.ci == Ci::None);
    }
}
