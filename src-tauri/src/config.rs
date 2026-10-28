use crate::compat::{Compat, Database, DrupalRelease, Service};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const DOCROOT: &str = "web";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Webserver {
    #[serde(rename = "nginx-fpm")]
    NginxFpm,
    #[serde(rename = "apache-fpm")]
    ApacheFpm,
}

impl Webserver {
    pub fn as_str(self) -> &'static str {
        match self {
            Webserver::NginxFpm => "nginx-fpm",
            Webserver::ApacheFpm => "apache-fpm",
        }
    }
}

/// What the wizard sends. Untrusted until `parse` turns it into a `Project`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInput {
    pub name: String,
    pub parent_dir: String,
    pub drupal: String,
    pub php: String,
    pub webserver: Webserver,
    pub database: Database,
    pub services: Vec<String>,
    pub admin_name: String,
    pub admin_email: String,
    pub site_name: String,
    pub config_sync_dir: String,
    pub export_config_on_install: bool,
    pub init_git: bool,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub dir: PathBuf,
    pub release: DrupalRelease,
    pub php: String,
    pub webserver: Webserver,
    pub database: Database,
    pub services: Vec<Service>,
    pub admin_name: String,
    pub admin_email: String,
    pub site_name: String,
    pub config_sync_dir: String,
    pub export_config_on_install: bool,
    pub init_git: bool,
}

impl Project {
    /// The sync directory relative to the project root, for `mkdir`.
    pub fn config_sync_path(&self) -> PathBuf {
        normalize(&Path::new(DOCROOT).join(&self.config_sync_dir))
            .expect("validated in parse")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
}

/// Lowercase letters, digits and single hyphens, as DDEV hostnames require.
pub fn machine_name(display: &str) -> String {
    let mut out = String::new();
    for c in display.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.truncate(63);
    out.trim_end_matches('-').to_string()
}

fn is_machine_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 63 && machine_name(s) == s
}

/// Lexically resolves `..` without touching the disk. `None` if it climbs above the start.
fn normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

fn safe_path_chars(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
}

pub fn enclosing_ddev_project(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|d| d.join(".ddev/config.yaml").is_file())
        .map(Path::to_path_buf)
}

pub fn parse(input: &ProjectInput, compat: &Compat) -> Result<Project, Vec<FieldError>> {
    let mut errors = Vec::new();
    let mut err = |field, message: &str| {
        errors.push(FieldError { field, message: message.to_string() })
    };

    if !is_machine_name(&input.name) {
        err("name", "Use lowercase letters, numbers and hyphens.");
    }

    let parent = PathBuf::from(&input.parent_dir);
    if !parent.is_absolute() || !parent.is_dir() {
        err("parentDir", "Choose an existing folder.");
    } else if let Some(outer) = enclosing_ddev_project(&parent) {
        err("parentDir", &format!("This folder is inside the DDEV project at {}.", outer.display()));
    }

    let release = compat.release(&input.drupal);
    match release {
        None => err("drupal", "Unknown Drupal version."),
        Some(r) => {
            if !r.php.contains(&input.php) {
                err("php", &format!("{} supports PHP {}.", r.label, r.php.join(", ")));
            }
            let ok_db = r
                .databases
                .get(&input.database.kind)
                .is_some_and(|v| v.contains(&input.database.version));
            if !ok_db {
                err("database", &format!("{} does not support this database version.", r.label));
            }
        }
    }

    let mut services = Vec::new();
    for id in &input.services {
        match compat.service(id) {
            Some(s) => services.push(s.clone()),
            None => err("services", &format!("Unknown service \"{id}\".")),
        }
    }

    let admin_ok = !input.admin_name.is_empty()
        && input.admin_name.len() <= 60
        && input.admin_name.chars().all(|c| c.is_ascii_alphanumeric() || "._@-".contains(c));
    if !admin_ok {
        err("adminName", "Use letters, numbers, dots, hyphens, underscores or @.");
    }

    let email = input.admin_email.as_str();
    let email_ok = email.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
    }) && !email.chars().any(|c| c.is_whitespace() || c.is_control() || "\"'\\".contains(c));
    if !email_ok {
        err("adminEmail", "Enter a valid email address.");
    }

    if input.site_name.trim().is_empty() || input.site_name.chars().any(char::is_control) {
        err("siteName", "Enter a site name.");
    }

    let sync = input.config_sync_dir.as_str();
    let sync_ok = !sync.is_empty()
        && safe_path_chars(sync)
        && Path::new(sync).is_relative()
        && normalize(&Path::new(DOCROOT).join(sync)).is_some_and(|p| p != Path::new(""));
    if !sync_ok {
        err("configSyncDir", "Use a relative path that stays inside the project, such as ../config/sync.");
    }

    match release {
        Some(r) if errors.is_empty() => Ok(Project {
            name: input.name.clone(),
            dir: parent.join(&input.name),
            release: r.clone(),
            php: input.php.clone(),
            webserver: input.webserver,
            database: input.database.clone(),
            services,
            admin_name: input.admin_name.clone(),
            admin_email: input.admin_email.clone(),
            site_name: input.site_name.trim().to_string(),
            config_sync_dir: sync.to_string(),
            export_config_on_install: input.export_config_on_install,
            init_git: input.init_git,
        }),
        _ => Err(errors),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::compat::DbType;

    pub fn input(parent: &Path) -> ProjectInput {
        ProjectInput {
            name: "mysite".into(),
            parent_dir: parent.display().to_string(),
            drupal: "11".into(),
            php: "8.3".into(),
            webserver: Webserver::NginxFpm,
            database: Database { kind: DbType::Mariadb, version: "10.11".into() },
            services: vec!["mailpit".into(), "adminer".into()],
            admin_name: "admin".into(),
            admin_email: "me@example.com".into(),
            site_name: "My Site".into(),
            config_sync_dir: "../config/sync".into(),
            export_config_on_install: true,
            init_git: true,
        }
    }

    fn fields(r: Result<Project, Vec<FieldError>>) -> Vec<&'static str> {
        r.err().unwrap_or_default().into_iter().map(|e| e.field).collect()
    }

    #[test]
    fn deserializes_the_payload_the_wizard_sends() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = r#"{"name":"acme-intranet","parentDir":"PARENT","drupal":"11","php":"8.3","webserver":"nginx-fpm","database":{"type":"postgres","version":"17"},"services":["mailpit","adminer","redis"],"adminName":"admin","adminEmail":"admin@example.com","siteName":"Acme Intranet","configSyncDir":"../config/sync","exportConfigOnInstall":true,"initGit":true}"#
            .replace("PARENT", &tmp.path().display().to_string());
        let input: ProjectInput = serde_json::from_str(&payload).unwrap();
        assert!(parse(&input, &Compat::bundled()).is_ok());
    }

    #[test]
    fn machine_name_follows_hostname_rules() {
        assert_eq!(machine_name("  My Cool Site! "), "my-cool-site");
        assert_eq!(machine_name("Ünïcode__Site--2"), "n-code-site-2");
        assert_eq!(machine_name("---"), "");
    }

    #[test]
    fn valid_input_parses_and_resolves_sync_dir_outside_webroot() {
        let tmp = tempfile::tempdir().unwrap();
        let p = parse(&input(tmp.path()), &Compat::bundled()).unwrap();
        assert_eq!(p.dir, tmp.path().join("mysite"));
        assert_eq!(p.config_sync_path(), PathBuf::from("config/sync"));
    }

    #[test]
    fn rejects_php_the_release_does_not_support() {
        let tmp = tempfile::tempdir().unwrap();
        let mut i = input(tmp.path());
        i.php = "8.1".into();
        assert_eq!(fields(parse(&i, &Compat::bundled())), vec!["php"]);
    }

    #[test]
    fn rejects_sync_dir_escaping_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        let mut i = input(tmp.path());
        for bad in ["../../outside", "/etc", "..", "../config/$(rm)"] {
            i.config_sync_dir = bad.into();
            assert_eq!(fields(parse(&i, &Compat::bundled())), vec!["configSyncDir"], "{bad}");
        }
    }

    #[test]
    fn blocks_parent_inside_another_ddev_project() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".ddev")).unwrap();
        std::fs::write(tmp.path().join(".ddev/config.yaml"), "name: x").unwrap();
        let nested = tmp.path().join("sub");
        std::fs::create_dir(&nested).unwrap();
        assert_eq!(fields(parse(&input(&nested), &Compat::bundled())), vec!["parentDir"]);
    }

    #[test]
    fn rejects_shell_metacharacters_in_admin_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let mut i = input(tmp.path());
        i.admin_name = "admin; rm -rf /".into();
        i.admin_email = "a\"@b.com".into();
        assert_eq!(fields(parse(&i, &Compat::bundled())), vec!["adminName", "adminEmail"]);
    }
}
