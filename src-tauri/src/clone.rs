//! Setting up an existing project from a Git repository: clone, configure and
//! start DDEV, install Composer packages, then give the site a database.

use crate::compat::Compat;
use crate::config::{self, FieldError, DOCROOT};
use crate::steps::{Action, Check, Retry, Step};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DbSource {
    /// `drush site:install --existing-config` from the repo's config export.
    ExistingConfig,
    /// `ddev import-db` from a dump file on this machine.
    Dump { path: String },
    None,
}

/// What the Clone screen sends. Untrusted until `parse`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneInput {
    pub repo_url: String,
    pub parent_dir: String,
    pub name: String,
    /// Used only when the repo has no `.ddev/config.yaml` of its own.
    pub drupal: String,
    pub database: DbSource,
    pub admin_name: String,
    pub admin_email: String,
}

#[derive(Debug, Clone)]
pub struct CloneProject {
    pub name: String,
    pub dir: PathBuf,
    pub repo_url: String,
    pub ddev_type: String,
    pub php: String,
    pub database: DbSource,
    pub admin_name: String,
    pub admin_email: String,
}

impl CloneProject {
    pub fn needs_password(&self) -> bool {
        self.database == DbSource::ExistingConfig
    }
}

const DUMP_EXTENSIONS: &[&str] = &["sql", "gz", "bz2", "xz", "zip", "tgz", "tar"];

/// HTTPS, SSH (`ssh://` or `user@host:path`), `git://`, `file://` or an
/// absolute path to a local repository. Never starts with `-`, so it can't
/// be read as a Git option.
pub fn is_repo_url(url: &str) -> bool {
    if url.is_empty() || url.starts_with('-') || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let scheme = ["https://", "http://", "ssh://", "git://", "file://"].iter().any(|s| url.starts_with(s) && url.len() > s.len());
    let scp = url.split_once(':').is_some_and(|(host, path)| host.contains('@') && !host.contains('/') && !path.is_empty());
    scheme || scp || (Path::new(url).is_absolute() && Path::new(url).is_dir())
}

/// The last part of a repository URL, without `.git`, as a project name.
pub fn suggested_name(url: &str) -> String {
    let last = url.trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or_default();
    config::machine_name(last.trim_end_matches(".git"))
}

/// `git clone` needs a missing or empty folder. Checked before a fresh run,
/// not on resume, when the folder holds the clone.
pub fn check_folder(dir: &Path) -> Result<(), FieldError> {
    if std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some()) {
        Err(FieldError { field: "name", message: format!("{} already exists and isn't empty.", dir.display()) })
    } else {
        Ok(())
    }
}

pub fn parse(input: &CloneInput, compat: &Compat) -> Result<CloneProject, Vec<FieldError>> {
    let mut errors = Vec::new();
    let mut err = |field, message: &str| errors.push(FieldError { field, message: message.to_string() });

    let url = input.repo_url.trim();
    if !is_repo_url(url) {
        err("repoUrl", "Enter an HTTPS or SSH Git URL, such as git@github.com:me/site.git.");
    }
    if input.name.is_empty() || config::machine_name(&input.name) != input.name {
        err("name", "Use lowercase letters, numbers and hyphens.");
    }
    let parent = PathBuf::from(&input.parent_dir);
    if let Err(e) = config::check_parent(&parent) {
        err("parentDir", &e);
    }
    let dir = parent.join(&input.name);
    let release = compat.release(&input.drupal);
    if release.is_none() {
        err("drupal", "Unknown Drupal version.");
    }

    match &input.database {
        DbSource::ExistingConfig => {
            if !config::is_admin_name(&input.admin_name) {
                err("adminName", config::ADMIN_NAME_RULE);
            }
            if !config::is_email(&input.admin_email) {
                err("adminEmail", config::EMAIL_RULE);
            }
        }
        DbSource::Dump { path } => {
            let p = Path::new(path);
            let ext_ok = p.extension().and_then(|e| e.to_str()).is_some_and(|e| DUMP_EXTENSIONS.contains(&e));
            if !p.is_absolute() || !p.is_file() || !ext_ok {
                err("database", "Choose a database dump: .sql, .sql.gz, .sql.bz2, .sql.xz, .zip or .tar.gz.");
            }
        }
        DbSource::None => {}
    }

    match release {
        Some(r) if errors.is_empty() => Ok(CloneProject {
            name: input.name.clone(),
            dir,
            repo_url: url.to_string(),
            ddev_type: r.ddev_type.clone(),
            php: r.default_php.clone(),
            database: input.database.clone(),
            admin_name: input.admin_name.clone(),
            admin_email: input.admin_email.clone(),
        }),
        _ => Err(errors),
    }
}

const MIN: u64 = 60;

fn ddev(args: &[&str]) -> Action {
    Action::Ddev { args: args.iter().map(|s| s.to_string()).collect(), stdin_secret: false }
}

fn step(id: &str, title: &str, action: Action, timeout_secs: u64) -> Step {
    Step { id: id.into(), title: title.into(), action, timeout_secs, check: Check::None, retry: Retry::Rerun }
}

pub fn build(p: &CloneProject) -> Vec<Step> {
    let config = ".ddev/config.yaml";
    let mut steps = vec![
        Step {
            check: Check::Exists { path: ".git".into() },
            retry: Retry::StartOver { reason: "git clone needs an empty folder.".into() },
            ..step("git-clone", "Clone the repository", Action::GitClone { url: p.repo_url.clone() }, 30 * MIN)
        },
        // Skipped when the repo brings its own DDEV config.
        Step {
            check: Check::Exists { path: config.into() },
            ..step(
                "ddev-config",
                "Configure DDEV",
                ddev(&[
                    "config",
                    &format!("--project-name={}", p.name),
                    &format!("--project-type={}", p.ddev_type),
                    &format!("--docroot={DOCROOT}"),
                    &format!("--php-version={}", p.php),
                ]),
                MIN,
            )
        },
        // A repo's own config names the project after its author's copy.
        Step {
            check: Check::Contains { path: config.into(), needle: format!("name: {}\n", p.name) },
            ..step("ddev-name", &format!("Name the DDEV project {}", p.name), ddev(&["config", &format!("--project-name={}", p.name)]), MIN)
        },
        step("ddev-start", "Start DDEV", ddev(&["start"]), 10 * MIN),
        step("composer-install", "Install Composer packages", ddev(&["composer", "install", "-n"]), 30 * MIN),
    ];

    match &p.database {
        DbSource::ExistingConfig => {
            steps.push(Step {
                retry: Retry::CleanupFirst { cleanup: ddev(&["drush", "sql:drop", "-y"]) },
                ..step(
                    "site-install",
                    "Install Drupal from the exported config",
                    ddev(&[
                        "drush",
                        "site:install",
                        "--existing-config",
                        "-y",
                        &format!("--account-name={}", p.admin_name),
                        &format!("--account-mail={}", p.admin_email),
                    ]),
                    20 * MIN,
                )
            });
            steps.push(step(
                "admin-password",
                "Set the admin password",
                Action::Ddev {
                    args: vec!["exec".into(), format!("read -r p && drush user:password {} \"$p\"", crate::script::quote(&p.admin_name))],
                    stdin_secret: true,
                },
                5 * MIN,
            ));
        }
        DbSource::Dump { path } => {
            steps.push(step("import-db", "Import the database dump", ddev(&["import-db", &format!("--file={path}")]), 30 * MIN));
            steps.push(step("cache-rebuild", "Rebuild caches", ddev(&["drush", "cache:rebuild"]), 5 * MIN));
        }
        DbSource::None => {}
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(parent: &Path, database: DbSource) -> CloneInput {
        CloneInput {
            repo_url: "git@github.com:acme/intranet.git".into(),
            parent_dir: parent.display().to_string(),
            name: "intranet".into(),
            drupal: "11".into(),
            database,
            admin_name: "admin".into(),
            admin_email: "admin@example.com".into(),
        }
    }

    fn fields(r: Result<CloneProject, Vec<FieldError>>) -> Vec<&'static str> {
        r.err().unwrap_or_default().into_iter().map(|e| e.field).collect()
    }

    #[test]
    fn accepts_common_git_urls_and_refuses_options() {
        for ok in ["https://github.com/acme/site.git", "git@github.com:acme/site.git", "ssh://git@gitlab.com/acme/site", "file:///tmp/repo"] {
            assert!(is_repo_url(ok), "{ok}");
        }
        for bad in ["--upload-pack=touch /tmp/x", "https://", "acme/site", "git@host:", "https://x y", ""] {
            assert!(!is_repo_url(bad), "{bad}");
        }
    }

    #[test]
    fn suggests_a_name_from_the_url() {
        assert_eq!(suggested_name("git@github.com:acme/My_Intranet.git"), "my-intranet");
        assert_eq!(suggested_name("https://gitlab.com/acme/site/"), "site");
    }

    #[test]
    fn dump_must_be_an_existing_dump_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dump = tmp.path().join("db.sql.gz");
        std::fs::write(&dump, "").unwrap();
        let ok = parse(&input(tmp.path(), DbSource::Dump { path: dump.display().to_string() }), &Compat::bundled());
        assert!(!ok.unwrap().needs_password());
        let bad = DbSource::Dump { path: tmp.path().join("notes.txt").display().to_string() };
        assert_eq!(fields(parse(&input(tmp.path(), bad), &Compat::bundled())), vec!["database"]);
    }

    #[test]
    fn refuses_a_folder_that_is_not_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("intranet");
        assert!(check_folder(&dir).is_ok(), "missing folder is fine");
        std::fs::create_dir(&dir).unwrap();
        assert!(check_folder(&dir).is_ok(), "empty folder is fine");
        std::fs::write(dir.join("x"), "").unwrap();
        assert_eq!(check_folder(&dir).unwrap_err().field, "name");
    }

    #[test]
    fn steps_for_each_database_source() {
        let tmp = tempfile::tempdir().unwrap();
        let ids = |db| {
            let p = parse(&input(tmp.path(), db), &Compat::bundled()).unwrap();
            build(&p).into_iter().map(|s| s.id).collect::<Vec<_>>().join(" ")
        };
        let base = "git-clone ddev-config ddev-name ddev-start composer-install";
        assert_eq!(ids(DbSource::None), base);
        assert_eq!(ids(DbSource::ExistingConfig), format!("{base} site-install admin-password"));
        let dump = tmp.path().join("db.sql");
        std::fs::write(&dump, "").unwrap();
        assert_eq!(ids(DbSource::Dump { path: dump.display().to_string() }), format!("{base} import-db cache-rebuild"));
    }
}

/// Real DDEV run that clones SMOKE_REPO (a Drupal repo with a config export,
/// such as one `smoke_creates_a_real_site` made) into SMOKE_PARENT and
/// installs it from that config.
/// `SMOKE_REPO=/path/to/site SMOKE_PARENT=/some/dir cargo test smoke_clones -- --ignored --nocapture`
#[cfg(test)]
mod smoke {
    use super::*;
    use crate::runner::{NativeExecutor, Run, RunEvent, Secret};
    use tokio::sync::watch;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn smoke_clones_a_real_site() {
        let parent = PathBuf::from(std::env::var("SMOKE_PARENT").expect("SMOKE_PARENT"));
        let input = CloneInput {
            repo_url: std::env::var("SMOKE_REPO").expect("SMOKE_REPO"),
            parent_dir: parent.display().to_string(),
            name: std::env::var("SMOKE_NAME").unwrap_or_else(|_| "ddevcreator-smoke-clone".into()),
            drupal: "11".into(),
            database: DbSource::ExistingConfig,
            admin_name: "admin".into(),
            admin_email: "admin@example.com".into(),
        };
        let p = parse(&input, &Compat::bundled()).unwrap();
        check_folder(&p.dir).unwrap();
        let steps = build(&p);
        let exec = NativeExecutor { path: crate::env::login_shell_path() };
        let (_tx, cancel) = watch::channel(false);
        let secret = Secret("smoke-pass-123".into());
        let state_path = parent.join(format!("{}.state.json", p.name));
        let run = Run { exec: &exec, dir: &p.dir, steps: &steps, secret: &secret, state_path: &state_path, cancel };
        let emit = |e: RunEvent| match e {
            RunEvent::Log { id, line } => println!("[{id}] {line}"),
            other => println!("== {}", serde_json::to_string(&other).unwrap()),
        };
        assert!(run.execute(&emit).await, "run succeeded");
        let config = std::fs::read_to_string(p.dir.join(".ddev/config.yaml")).unwrap();
        assert!(config.contains(&format!("name: {}\n", p.name)), "renamed to the clone's own project name");
    }
}
