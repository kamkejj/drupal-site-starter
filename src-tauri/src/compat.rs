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
    Preview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrupalRelease {
    pub id: String,
    pub label: String,
    pub package: String,
    pub ddev_type: String,
    pub install_profile: String,
    /// `composer config` pairs applied right after create-project, in order.
    #[serde(default)]
    pub composer_config: Vec<(String, String)>,
    /// Packages to reinstall after `composer_config`, such as core from source.
    #[serde(default)]
    pub reinstall: Vec<String>,
    /// `None` when the project template already requires Drush.
    pub drush_package: Option<String>,
    pub dev_package: String,
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
    /// `setup` runs after the modules are enabled, for wiring a module alone
    /// can't do, such as creating a Search API server. `settings` is PHP that
    /// goes in its own settings file, included from settings.php after the
    /// modules are enabled, because it names services they define.
    Module {
        packages: Vec<String>,
        modules: Vec<String>,
        #[serde(default)]
        setup: Vec<SetupCommand>,
        #[serde(default)]
        settings: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupCommand {
    pub title: String,
    pub drush: Vec<String>,
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

/// A set of contributed modules added after install, such as SEO basics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub id: String,
    pub label: String,
    pub description: String,
    pub packages: Vec<String>,
    /// Required with `composer require --dev`, for tools like Devel.
    #[serde(default)]
    pub dev: bool,
    pub modules: Vec<String>,
    #[serde(default)]
    pub setup: Vec<SetupCommand>,
    /// Release ids it works on. Empty means every release.
    #[serde(default)]
    pub releases: Vec<String>,
}

/// A Drupal recipe from Packagist, applied with `drush recipe` after install.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    pub id: String,
    pub label: String,
    pub description: String,
    pub package: String,
    #[serde(default)]
    pub releases: Vec<String>,
}

impl Recipe {
    /// Composer installs `drupal-recipe` packages to `recipes/<name>`.
    pub fn dir_name(&self) -> &str {
        let name = self.package.split(':').next().unwrap_or_default();
        name.rsplit('/').next().unwrap_or(name)
    }
}

fn available(releases: &[String], release: &str) -> bool {
    releases.is_empty() || releases.iter().any(|r| r == release)
}

impl Bundle {
    pub fn available_for(&self, release: &str) -> bool {
        available(&self.releases, release)
    }
}

impl Recipe {
    pub fn available_for(&self, release: &str) -> bool {
        available(&self.releases, release)
    }
}

/// `vendor/name`, optionally with a version constraint. Never starts with `-`,
/// so a package can't be read as a Composer option.
fn is_package(s: &str) -> bool {
    let (name, constraint) = s.split_once(':').unwrap_or((s, ""));
    let part = |p: &str| !p.is_empty() && !p.starts_with(['-', '.']) && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.-".contains(c));
    let constraint_ok = constraint.chars().all(|c| c.is_ascii_alphanumeric() || "^~*.@|<>=, -".contains(c));
    matches!(name.split_once('/'), Some((v, n)) if part(v) && part(n)) && constraint_ok
}

fn is_machine_word(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-".contains(c))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Compat {
    /// Raised with every published change. A downloaded copy replaces the
    /// bundled one only when its revision is higher.
    #[serde(default)]
    pub revision: u32,
    pub min_ddev_version: String,
    pub min_ddev_version_podman: String,
    pub composer_plugins: Vec<String>,
    pub drupal: Vec<DrupalRelease>,
    pub services: Vec<Service>,
    #[serde(default)]
    pub bundles: Vec<Bundle>,
    #[serde(default)]
    pub recipes: Vec<Recipe>,
}

impl Compat {
    pub fn bundled() -> Compat {
        serde_json::from_str(include_str!("../resources/compat.json"))
            .expect("bundled compat.json is valid")
    }

    /// Parses and checks a compat.json from outside the app. Its values end up
    /// in `ddev` and `composer` arguments, so names are held to strict rules.
    pub fn from_json(text: &str) -> Result<Compat, String> {
        let c: Compat = serde_json::from_str(text).map_err(|e| format!("compat.json: {e}"))?;
        c.validate()?;
        Ok(c)
    }

    pub fn validate(&self) -> Result<(), String> {
        let fail = |what: String| Err(format!("compat.json: {what}"));
        if self.drupal.is_empty() {
            return fail("no Drupal releases".into());
        }
        for r in &self.drupal {
            if !is_machine_word(&r.id) || !is_machine_word(&r.ddev_type) || !is_machine_word(&r.install_profile) {
                return fail(format!("release {:?} has an invalid id, DDEV type or profile", r.id));
            }
            let packages = std::iter::once(&r.package).chain(&r.drush_package).chain([&r.dev_package]).chain(&r.reinstall);
            if let Some(bad) = packages.into_iter().find(|p| !is_package(p)) {
                return fail(format!("release {} has an invalid package {bad:?}", r.id));
            }
            if !r.php.contains(&r.default_php) {
                return fail(format!("release {} default PHP isn't in its list", r.id));
            }
            if !r.databases.get(&r.default_database.kind).is_some_and(|v| v.contains(&r.default_database.version)) {
                return fail(format!("release {} default database isn't in its list", r.id));
            }
        }
        let releases: Vec<&str> = self.drupal.iter().map(|r| r.id.as_str()).collect();
        for s in &self.services {
            if !is_machine_word(&s.id) || s.addon.as_ref().is_some_and(|a| !is_package(a)) {
                return fail(format!("service {:?} has an invalid id or add-on", s.id));
            }
            if let Wiring::Module { packages, modules, .. } = &s.wiring
                && (packages.iter().any(|p| !is_package(p)) || modules.iter().any(|m| !is_machine_word(m)))
            {
                return fail(format!("service {} has an invalid package or module", s.id));
            }
        }
        for b in &self.bundles {
            if !is_machine_word(&b.id) || b.packages.iter().any(|p| !is_package(p)) || b.modules.iter().any(|m| !is_machine_word(m)) {
                return fail(format!("bundle {:?} has an invalid id, package or module", b.id));
            }
            if let Some(r) = b.releases.iter().find(|r| !releases.contains(&r.as_str())) {
                return fail(format!("bundle {} names unknown release {r}", b.id));
            }
        }
        for r in &self.recipes {
            if !is_machine_word(&r.id) || !is_package(&r.package) || !is_machine_word(r.dir_name()) {
                return fail(format!("recipe {:?} has an invalid id or package", r.id));
            }
            if let Some(rel) = r.releases.iter().find(|x| !releases.contains(&x.as_str())) {
                return fail(format!("recipe {} names unknown release {rel}", r.id));
            }
        }
        Ok(())
    }

    pub fn release(&self, id: &str) -> Option<&DrupalRelease> {
        self.drupal.iter().find(|r| r.id == id)
    }

    pub fn service(&self, id: &str) -> Option<&Service> {
        self.services.iter().find(|s| s.id == id)
    }

    pub fn bundle(&self, id: &str) -> Option<&Bundle> {
        self.bundles.iter().find(|b| b.id == id)
    }

    pub fn recipe(&self, id: &str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_compat_is_internally_consistent() {
        Compat::bundled().validate().unwrap();
    }

    #[test]
    fn packages_cannot_pose_as_options() {
        for ok in ["drupal/core-dev:^11", "drupal/elasticsearch_connector:^9.0@alpha", "ddev/ddev-redis", "drupal/recommended-project:^12@beta"] {
            assert!(is_package(ok), "{ok}");
        }
        for bad in ["--dev", "drupal", "-x/y", "drupal/../x", "drupal/x:^1;rm", "Drupal/X"] {
            assert!(!is_package(bad), "{bad}");
        }
    }

    #[test]
    fn rejects_remote_compat_with_unsafe_values() {
        let mut v: serde_json::Value = serde_json::from_str(include_str!("../resources/compat.json")).unwrap();
        v["recipes"][0]["package"] = "drupal/../../etc".into();
        assert!(Compat::from_json(&v.to_string()).unwrap_err().contains("recipe"));
        assert!(Compat::from_json("{}").is_err());
    }

    #[test]
    fn recipe_dir_drops_vendor_and_constraint() {
        let r = Recipe { id: "seo".into(), label: "".into(), description: "".into(), package: "drupal/drupal_cms_seo_tools:^2".into(), releases: vec![] };
        assert_eq!(r.dir_name(), "drupal_cms_seo_tools");
    }
}
