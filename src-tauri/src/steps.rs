//! The ordered step list. The live runner and the exported script both render
//! from `build`, so the two cannot drift apart.

use crate::compat::{Compat, DbType, Wiring};
use crate::config::{Project, DOCROOT};
use serde::Serialize;

pub const SETTINGS_PHP: &str = "web/sites/default/settings.php";
pub const DDEV_INCLUDE_MARKER: &str = "// Automatically generated include for settings managed by ddev.";
pub const PASSWORD_ENV: &str = "DRUPAL_ADMIN_PASS";

pub fn config_sync_line(dir: &str) -> String {
    format!("$settings['config_sync_directory'] = '{dir}';")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    /// `ddev <args>` in the project folder. With `stdin_secret`, the admin
    /// password is written to stdin so it never appears in argv. Only plain
    /// `ddev exec` forwards stdin; `ddev exec --raw` does not.
    Ddev { args: Vec<String>, stdin_secret: bool },
    Git { args: Vec<String> },
    CreateProjectDir,
    Mkdir { path: String },
    WriteFile { path: String, contents: String },
    AppendFile { path: String, text: String },
    /// Inserts `config_sync_directory` above the DDEV include in settings.php.
    SetConfigSyncDir { dir: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Check {
    None,
    Exists { path: String },
    Contains { path: String, needle: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Retry {
    Rerun,
    CleanupFirst { cleanup: Action },
    StartOver { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub title: String,
    pub action: Action,
    pub timeout_secs: u64,
    pub check: Check,
    pub retry: Retry,
}

fn ddev(args: &[&str]) -> Action {
    Action::Ddev { args: args.iter().map(|s| s.to_string()).collect(), stdin_secret: false }
}

fn git(args: &[&str]) -> Action {
    Action::Git { args: args.iter().map(|s| s.to_string()).collect() }
}

fn step(id: impl Into<String>, title: impl Into<String>, action: Action, timeout_secs: u64) -> Step {
    Step { id: id.into(), title: title.into(), action, timeout_secs, check: Check::None, retry: Retry::Rerun }
}

impl Step {
    fn check(mut self, check: Check) -> Self {
        self.check = check;
        self
    }

    fn retry(mut self, retry: Retry) -> Self {
        self.retry = retry;
        self
    }
}

fn contains(path: &str, needle: &str) -> Check {
    Check::Contains { path: path.into(), needle: needle.into() }
}

fn simpletest_db(kind: DbType) -> &'static str {
    match kind {
        DbType::Postgres => "pgsql://db:db@db/db",
        DbType::Mariadb | DbType::Mysql => "mysql://db:db@db/db",
    }
}

const MIN: u64 = 60;

/// File types each tool checks. PHPCS's list also fills phpcs.xml.dist;
/// PHPStan's matches phpstan-drupal's fileExtensions.
const LINT_EXTENSIONS: &[&str] = &["php", "module", "inc", "install", "test", "profile", "theme", "info", "yml"];
const ANALYSE_EXTENSIONS: &[&str] = &["php", "module", "inc", "install", "theme", "profile"];

/// PHPStan exits 1 and PHPCS 4 exits 16 when they find no files, which every
/// fresh site hits, so each tool runs only once matching custom code exists.
fn when_custom_code(extensions: &[&str], command: &str) -> String {
    let names = extensions
        .iter()
        .map(|e| format!("-name '*.{e}'"))
        .collect::<Vec<_>>()
        .join(" -o ");
    format!(
        "if find web/modules/custom web/themes/custom -type f \\( {names} \\) | grep -q .; \
         then {command}; else echo 'No custom code to check yet.'; fi"
    )
}

pub fn build(p: &Project, compat: &Compat) -> Vec<Step> {
    let r = &p.release;
    let mut steps = vec![
        step("mkdir", "Create the project folder", Action::CreateProjectDir, MIN),
        step(
            "ddev-config",
            "Configure DDEV",
            ddev(&[
                "config",
                &format!("--project-name={}", p.name),
                &format!("--project-type={}", r.ddev_type),
                &format!("--docroot={DOCROOT}"),
                &format!("--php-version={}", p.php),
                &format!("--webserver-type={}", p.webserver.as_str()),
                &format!("--database={}:{}", p.database.kind.as_str(), p.database.version),
            ]),
            MIN,
        ),
        step("ddev-start", "Start DDEV", ddev(&["start"]), 10 * MIN),
        step(
            "create-project",
            format!("Download {}", r.label),
            ddev(&["composer", "create-project", &r.package, "-n"]),
            30 * MIN,
        )
        .check(Check::Exists { path: "composer.json".into() })
        .retry(Retry::StartOver { reason: "create-project needs an empty folder.".into() }),
    ];

    for (key, value) in &r.composer_config {
        steps.push(step(
            format!("composer-config:{key}"),
            format!("Set Composer {key} to {value}"),
            ddev(&["composer", "config", key, value]),
            MIN,
        ));
    }
    if !r.reinstall.is_empty() {
        let mut args = vec!["composer", "reinstall"];
        args.extend(r.reinstall.iter().map(String::as_str));
        args.push("-n");
        steps.push(step("reinstall", format!("Reinstall {}", r.reinstall.join(", ")), ddev(&args), 10 * MIN));
    }

    if let Some(drush) = &r.drush_package {
        steps.push(
            step("drush", "Add Drush", ddev(&["composer", "require", drush, "-n"]), 10 * MIN)
                .check(contains("composer.json", "\"drush/drush\"")),
        );
    }

    for plugin in &compat.composer_plugins {
        steps.push(step(
            format!("allow-plugin:{plugin}"),
            format!("Allow Composer plugin {plugin}"),
            ddev(&["composer", "config", "--no-plugins", &format!("allow-plugins.{plugin}"), "true"]),
            MIN,
        ));
    }

    steps.push(
        step(
            "core-dev",
            "Add code quality and testing tools (drupal/core-dev)",
            ddev(&[
                "composer",
                "require",
                "--dev",
                &r.dev_package,
                "--with-all-dependencies",
                "-n",
            ]),
            20 * MIN,
        )
        .check(contains("composer.json", "\"drupal/core-dev\"")),
    );

    for s in &p.services {
        if let Some(addon) = &s.addon {
            steps.push(step(
                format!("addon:{}", s.id),
                format!("Add the {} add-on", s.label),
                ddev(&["add-on", "get", addon]),
                5 * MIN,
            ));
        }
    }
    steps.push(step("ddev-restart", "Restart DDEV", ddev(&["restart"]), 10 * MIN));

    let files = [
        (
            "phpcs.xml.dist",
            include_str!("../resources/templates/phpcs.xml.dist")
                .replace("{{EXTENSIONS}}", &LINT_EXTENSIONS.join(",")),
        ),
        ("phpstan.neon", include_str!("../resources/templates/phpstan.neon").to_string()),
        (
            "phpunit.xml",
            include_str!("../resources/templates/phpunit.xml")
                .replace("{{SIMPLETEST_DB}}", simpletest_db(p.database.kind)),
        ),
        ("web/modules/custom/.gitkeep", String::new()),
        ("web/themes/custom/.gitkeep", String::new()),
    ];
    for (path, contents) in files {
        steps.push(step(
            format!("write:{path}"),
            format!("Write {path}"),
            Action::WriteFile { path: path.into(), contents },
            MIN,
        ));
    }

    for (name, command) in [
        ("lint", when_custom_code(LINT_EXTENSIONS, "phpcs")),
        ("analyse", when_custom_code(ANALYSE_EXTENSIONS, "phpstan analyse --memory-limit=-1")),
        ("test", "phpunit".to_string()),
    ] {
        steps.push(step(
            format!("script:{name}"),
            format!("Add the composer {name} script"),
            ddev(&["composer", "config", &format!("scripts.{name}"), &command]),
            MIN,
        ));
    }

    let sync_path = p.config_sync_path().display().to_string();
    steps.push(step(
        "mkdir-config-sync",
        format!("Create {sync_path}"),
        Action::Mkdir { path: sync_path },
        MIN,
    ));
    steps.push(
        step(
            "settings-config-sync",
            "Set config_sync_directory in settings.php",
            Action::SetConfigSyncDir { dir: p.config_sync_dir.clone() },
            MIN,
        )
        .check(contains(SETTINGS_PHP, &config_sync_line(&p.config_sync_dir))),
    );

    steps.push(
        step(
            "site-install",
            "Install Drupal",
            ddev(&[
                "drush",
                "site:install",
                &r.install_profile,
                "-y",
                &format!("--account-name={}", p.admin_name),
                &format!("--account-mail={}", p.admin_email),
                &format!("--site-name={}", p.site_name),
            ]),
            20 * MIN,
        )
        .retry(Retry::CleanupFirst { cleanup: ddev(&["drush", "sql:drop", "-y"]) }),
    );
    steps.push(step(
        "admin-password",
        "Set the admin password",
        Action::Ddev {
            args: vec![
                "exec".into(),
                format!("read -r p && drush user:password {} \"$p\"", crate::script::quote(&p.admin_name)),
            ],
            stdin_secret: true,
        },
        5 * MIN,
    ));

    for s in &p.services {
        if let Wiring::Module { packages, modules, setup, settings } = &s.wiring {
            let mut require = vec!["composer", "require"];
            require.extend(packages.iter().map(String::as_str));
            require.push("-n");
            steps.push(step(format!("wire:{}:require", s.id), format!("Add the {} module", s.label), ddev(&require), 10 * MIN));

            let mut enable = vec!["drush", "pm:enable", "-y"];
            enable.extend(modules.iter().map(String::as_str));
            steps.push(step(format!("wire:{}:enable", s.id), format!("Enable the {} module", s.label), ddev(&enable), 5 * MIN));

            for (n, cmd) in setup.iter().enumerate() {
                let mut args = vec!["drush"];
                args.extend(cmd.drush.iter().map(String::as_str));
                steps.push(step(format!("wire:{}:setup:{n}", s.id), cmd.title.clone(), ddev(&args), 5 * MIN));
            }

            if let Some(php) = settings {
                let file = format!("settings.{}.php", s.id);
                steps.push(step(
                    format!("wire:{}:settings", s.id),
                    format!("Write {file}"),
                    Action::WriteFile { path: format!("{DOCROOT}/sites/default/{file}"), contents: format!("<?php\n\n{php}\n") },
                    MIN,
                ));
                steps.push(
                    step(
                        format!("wire:{}:include", s.id),
                        format!("Include {file} from settings.php"),
                        Action::AppendFile {
                            path: SETTINGS_PHP.into(),
                            text: format!(
                                "\n// {label} settings, written by Drupal DDEV Site Creator.\n\
                                 if (getenv('IS_DDEV_PROJECT') == 'true' && file_exists(__DIR__ . '/{file}')) {{\n  \
                                 include __DIR__ . '/{file}';\n}}\n",
                                label = s.label,
                            ),
                        },
                        MIN,
                    )
                    .check(contains(SETTINGS_PHP, &file)),
                );
            }
        }
    }

    if p.export_config_on_install {
        steps.push(step("config-export", "Export configuration", ddev(&["drush", "config:export", "-y"]), 5 * MIN));
    }

    // Cron writes these on the first page view; writing them now keeps the
    // first commit from going dirty.
    steps.push(step(
        "htaccess",
        "Write protective .htaccess files",
        ddev(&["drush", "php:eval", "\\Drupal::service('file.htaccess_writer')->ensure();"]),
        MIN,
    ));

    if p.init_git {
        steps.push(
            step(
                "write:.gitignore",
                "Write .gitignore",
                Action::WriteFile {
                    path: ".gitignore".into(),
                    contents: include_str!("../resources/templates/gitignore").into(),
                },
                MIN,
            )
            .check(Check::Exists { path: ".gitignore".into() }),
        );
        steps.push(step("git-init", "Initialize Git", git(&["init", "-q"]), MIN).check(Check::Exists { path: ".git".into() }));
        steps.push(step("git-add", "Stage files", git(&["add", "-A"]), 5 * MIN));
        steps.push(step("git-commit", "Create the first commit", git(&["commit", "-q", "-m", "Initial Drupal site"]), 5 * MIN));
    }

    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::Database;
    use crate::config::{parse, tests::input};
    use crate::script::render;

    fn summary(steps: &[Step]) -> String {
        steps
            .iter()
            .map(|s| match &s.action {
                Action::WriteFile { path, .. } => format!("{}: write {path}", s.id),
                a => format!("{}: {}", s.id, serde_json::to_string(a).unwrap()),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn snapshot(name: &str, tweak: impl FnOnce(&mut crate::config::ProjectInput)) {
        let parent = std::path::Path::new("/tmp");
        let mut i = input(parent);
        tweak(&mut i);
        let compat = Compat::bundled();
        let p = parse(&i, &compat).unwrap();
        let steps = build(&p, &compat);
        let script = render(&p, &steps);
        let bash = std::process::Command::new("bash")
            .args(["-n", "-c", &script])
            .output()
            .unwrap();
        assert!(bash.status.success(), "script parses: {}", String::from_utf8_lossy(&bash.stderr));
        insta::assert_snapshot!(format!("{name}-steps"), summary(&steps));
        insta::assert_snapshot!(format!("{name}-script"), script);
    }

    #[test]
    fn drupal11_defaults() {
        snapshot("drupal11", |_| {});
    }

    #[test]
    fn drupal12_preview() {
        snapshot("drupal12", |i| {
            i.drupal = "12".into();
            i.php = "8.5".into();
            i.database = Database { kind: DbType::Postgres, version: "18".into() };
        });
    }

    #[test]
    fn drupal11_memcached_elasticsearch() {
        snapshot("drupal11-memcached-elasticsearch", |i| {
            i.services = vec!["memcached".into(), "elasticsearch".into()];
        });
    }

    #[test]
    fn cms_postgres_wired_services_no_git() {
        snapshot("cms-postgres", |i| {
            i.drupal = "cms".into();
            i.database = Database { kind: DbType::Postgres, version: "17".into() };
            i.services = vec!["redis".into(), "solr".into(), "selenium".into()];
            i.site_name = "Bob's \"Site\"".into();
            i.export_config_on_install = false;
            i.init_git = false;
        });
    }
}
