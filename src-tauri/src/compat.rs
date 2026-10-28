use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DbType {
    Mariadb,
    Mysql,
    Postgres,
}

impl DbType {
    pub fn as_str(self) -> &'static str {
        match self {
            DbType::Mariadb => "mariadb",
            DbType::Mysql => "mysql",
            DbType::Postgres => "postgres",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Database {
    #[serde(rename = "type")]
    pub kind: DbType,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseStatus {
    Supported,
    EndingSoon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrupalRelease {
    pub id: String,
    pub label: String,
    pub package: String,
    pub ddev_type: String,
    pub core_major: u32,
    pub install_profile: String,
    pub includes_drush: bool,
    pub php: Vec<String>,
    pub default_php: String,
    pub databases: BTreeMap<DbType, Vec<String>>,
    pub default_database: Database,
    pub status: ReleaseStatus,
    #[serde(default)]
    pub status_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Wiring {
    None,
    ContainerOnly,
    Module { packages: Vec<String>, modules: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub label: String,
    pub addon: Option<String>,
    pub wiring: Wiring,
    pub default_on: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Compat {
    pub min_ddev_version: String,
    pub min_ddev_version_podman: String,
    pub composer_plugins: Vec<String>,
    pub drupal: Vec<DrupalRelease>,
    pub services: Vec<Service>,
}

impl Compat {
    pub fn bundled() -> Compat {
        serde_json::from_str(include_str!("../resources/compat.json"))
            .expect("bundled compat.json is valid")
    }

    pub fn release(&self, id: &str) -> Option<&DrupalRelease> {
        self.drupal.iter().find(|r| r.id == id)
    }

    pub fn service(&self, id: &str) -> Option<&Service> {
        self.services.iter().find(|s| s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_compat_is_internally_consistent() {
        let compat = Compat::bundled();
        for r in &compat.drupal {
            assert!(r.php.contains(&r.default_php), "{} default php", r.id);
            let versions = &r.databases[&r.default_database.kind];
            assert!(versions.contains(&r.default_database.version), "{} default db", r.id);
        }
    }
}
