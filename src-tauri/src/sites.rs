use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub name: String,
    pub status: String,
    pub kind: String,
    pub dir: PathBuf,
    pub url: Option<String>,
}

impl Site {
    pub fn running(&self) -> bool {
        self.status == "running"
    }
}

pub fn list(path: &str) -> Vec<Site> {
    std::process::Command::new("ddev")
        .args(["list", "-j"])
        .env("PATH", path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_list(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// Reads the `raw` array of `ddev list -j`.
pub fn parse_list(json: &str) -> Vec<Site> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let Some(raw) = v["raw"].as_array() else { return Vec::new() };
    raw.iter()
        .filter_map(|p| {
            Some(Site {
                name: p["name"].as_str()?.to_string(),
                status: p["status"].as_str().unwrap_or("unknown").to_string(),
                kind: p["type"].as_str().unwrap_or("").to_string(),
                dir: PathBuf::from(p["approot"].as_str()?),
                url: p["primary_url"].as_str().filter(|u| !u.is_empty()).map(str::to_string),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SiteAction {
    Start,
    Stop,
    Snapshot,
    ConfigImport,
    ConfigExport,
    Delete,
}

impl SiteAction {
    /// `ddev` arguments for this action. Project-level commands take the name;
    /// drush runs in the project folder and has no name argument.
    pub fn args(self, name: &str) -> Vec<&str> {
        match self {
            Self::Start => vec!["start", name],
            Self::Stop => vec!["stop", name],
            Self::Snapshot => vec!["snapshot", name],
            Self::ConfigImport => vec!["drush", "config:import", "-y"],
            Self::ConfigExport => vec!["drush", "config:export", "-y"],
            Self::Delete => vec!["delete", "-y", name],
        }
    }

    pub fn needs_running(self) -> bool {
        matches!(self, Self::Stop | Self::Snapshot | Self::ConfigImport | Self::ConfigExport)
    }
}

/// Runs a dashboard action on a site DDEV already knows. The name is looked up
/// in `ddev list`, so the UI can't point a command at an arbitrary folder.
pub async fn run(path: &str, name: &str, action: SiteAction) -> Result<(), String> {
    let owned = path.to_string();
    let all = tokio::task::spawn_blocking(move || list(&owned)).await.map_err(|e| e.to_string())?;
    let site = all.into_iter().find(|s| s.name == name).ok_or(format!("DDEV has no project named {name}."))?;
    if action.needs_running() && !site.running() {
        return Err(format!("Start {name} first."));
    }
    let out = tokio::process::Command::new("ddev")
        .args(action.args(&site.name))
        .current_dir(&site.dir)
        .env("PATH", path)
        .env("DDEV_NONINTERACTIVE", "true")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    Err(lines[lines.len().saturating_sub(6)..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"{"level":"info","msg":"table","raw":[
        {"approot":"/Users/me/Sites/blog","name":"blog","primary_url":"https://blog.ddev.site","status":"running","type":"drupal11"},
        {"approot":"/Users/me/Sites/old","name":"old","primary_url":"","status":"stopped","type":"drupal10"},
        {"name":"no-root","status":"stopped"}
    ]}"#;

    #[test]
    fn parses_ddev_list() {
        let sites = parse_list(LIST);
        assert_eq!(sites.len(), 2, "entries without an approot are dropped");
        assert_eq!(sites[0].name, "blog");
        assert!(sites[0].running());
        assert_eq!(sites[0].url.as_deref(), Some("https://blog.ddev.site"));
        assert_eq!(sites[1].url, None, "an empty URL means none");
        assert!(!sites[1].running());
        assert!(parse_list("not json").is_empty());
    }

    #[test]
    fn actions_map_to_ddev_args() {
        assert_eq!(SiteAction::Start.args("blog"), ["start", "blog"]);
        assert_eq!(SiteAction::Delete.args("blog"), ["delete", "-y", "blog"]);
        assert_eq!(SiteAction::ConfigExport.args("blog"), ["drush", "config:export", "-y"]);
        assert!(!SiteAction::Start.needs_running());
        assert!(SiteAction::ConfigImport.needs_running());
    }

    fn status(path: &str, name: &str) -> String {
        list(path).into_iter().find(|s| s.name == name).map(|s| s.status).unwrap_or_default()
    }

    /// Runs every action against a real, installed Drupal site named by
    /// DASH_SITE, ending with delete. The site's files are left in place.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn actions_on_a_real_site() {
        let name = std::env::var("DASH_SITE").expect("DASH_SITE");
        let path = crate::env::login_shell_path();
        let dir = list(&path).into_iter().find(|s| s.name == name).expect("DASH_SITE is a DDEV project").dir;

        assert!(run(&path, "no-such-site", SiteAction::Start).await.unwrap_err().contains("no project named"));
        run(&path, &name, SiteAction::Stop).await.unwrap();
        assert_eq!(status(&path, &name), "stopped");
        assert_eq!(run(&path, &name, SiteAction::ConfigExport).await.unwrap_err(), format!("Start {name} first."));
        run(&path, &name, SiteAction::Start).await.unwrap();
        assert_eq!(status(&path, &name), "running");

        run(&path, &name, SiteAction::Snapshot).await.unwrap();
        assert!(std::fs::read_dir(dir.join(".ddev/db_snapshots")).unwrap().next().is_some(), "snapshot written");
        run(&path, &name, SiteAction::ConfigExport).await.unwrap();
        assert!(dir.join("web/sites/default/files/sync/system.site.yml").is_file(), "config exported");
        run(&path, &name, SiteAction::ConfigImport).await.unwrap();

        run(&path, &name, SiteAction::Delete).await.unwrap();
        assert!(list(&path).iter().all(|s| s.name != name), "gone from ddev list");
        assert!(dir.join("web/index.php").is_file(), "files kept");
    }
}
