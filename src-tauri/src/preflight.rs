use crate::compat::Compat;
use serde::Serialize;
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub id: &'static str,
    pub label: &'static str,
    pub level: Level,
    pub detail: String,
}

fn result(id: &'static str, label: &'static str, level: Level, detail: impl Into<String>) -> CheckResult {
    CheckResult { id, label, level, detail: detail.into() }
}

fn output(path: &str, program: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(program).args(args).env("PATH", path).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// `v1.25.5` or `1.25.5-rc1` → (1, 25, 5).
pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut n = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((n.next()??, n.next().flatten().unwrap_or(0), n.next().flatten().unwrap_or(0)))
}

struct DdevInfo {
    version: String,
    platform: String,
}

fn ddev_info(path: &str) -> Option<DdevInfo> {
    let json: serde_json::Value = serde_json::from_str(&output(path, "ddev", &["version", "-j"])?).ok()?;
    let raw = &json["raw"];
    Some(DdevInfo {
        version: raw["DDEV version"].as_str()?.to_string(),
        platform: raw["docker-platform"].as_str().unwrap_or("unknown").to_string(),
    })
}

/// DDEV moves to other ports when 80/443 are taken, so ask it for the real URL.
pub fn project_url(path: &str, name: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(&output(path, "ddev", &["describe", "-j", name])?).ok()?;
    json["raw"]["primary_url"].as_str().map(str::to_string)
}

fn port_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn free_gib(dir: &Path, path: &str) -> Option<u64> {
    let out = output(path, "df", &["-Pk", &dir.display().to_string()])?;
    let avail_kib: u64 = out.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()?;
    Some(avail_kib / 1024 / 1024)
}

pub fn run(path: &str, compat: &Compat, projects_dir: &Path) -> Vec<CheckResult> {
    let mut checks = Vec::new();
    let info = ddev_info(path);

    checks.push(match &info {
        None => result("ddev", "DDEV", Level::Fail, "DDEV is not installed or not on PATH. See ddev.readthedocs.io."),
        Some(i) => {
            let podman = i.platform.contains("podman");
            let min = if podman { &compat.min_ddev_version_podman } else { &compat.min_ddev_version };
            if parse_version(&i.version) >= parse_version(min) {
                result("ddev", "DDEV", Level::Ok, format!("{} on {}", i.version, i.platform))
            } else {
                result("ddev", "DDEV", Level::Fail, format!("{} is older than the required v{min}.", i.version))
            }
        }
    });

    let podman = info.as_ref().is_some_and(|i| i.platform.contains("podman"));
    let (program, args): (&str, &[&str]) = if podman {
        ("podman", &["info", "--format", "{{.Version.Version}}"])
    } else {
        ("docker", &["info", "--format", "{{.ServerVersion}}"])
    };
    checks.push(match output(path, program, args) {
        Some(v) => result("runtime", "Container runtime", Level::Ok, format!("{program} {v} is running")),
        None => result("runtime", "Container runtime", Level::Fail, format!("{program} is not running. Start Docker Desktop, OrbStack, Colima, Rancher Desktop or Podman.")),
    });

    let ca = output(path, "mkcert", &["-CAROOT"]).map(|d| Path::new(&d).join("rootCA.pem"));
    checks.push(match ca {
        Some(p) if p.is_file() => result("mkcert", "Trusted HTTPS", Level::Ok, "mkcert CA is installed"),
        _ => result("mkcert", "Trusted HTTPS", Level::Warn, "Run `mkcert -install` once for trusted HTTPS."),
    });

    let busy: Vec<String> = [80, 443].into_iter().filter(|p| !port_free(*p)).map(|p| p.to_string()).collect();
    checks.push(if busy.is_empty() {
        result("ports", "Ports 80 and 443", Level::Ok, "free")
    } else {
        result("ports", "Ports 80 and 443", Level::Warn, format!("Port {} in use. Fine if it's the DDEV router; otherwise DDEV serves sites on other ports, such as :30001.", busy.join(" and ")))
    });

    checks.push(match free_gib(projects_dir, path) {
        Some(gib) if gib >= 5 => result("disk", "Disk space", Level::Ok, format!("{gib} GiB free")),
        Some(gib) => result("disk", "Disk space", Level::Warn, format!("Only {gib} GiB free. A Drupal site needs about 1 GiB plus images.")),
        None => result("disk", "Disk space", Level::Warn, "Could not read free space."),
    });

    checks.push(match output(path, "git", &["--version"]) {
        Some(v) => result("git", "Git", Level::Ok, v),
        None => result("git", "Git", Level::Warn, "Git is not installed, so projects start without a repository."),
    });

    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ddev_versions() {
        assert_eq!(parse_version("v1.25.5"), Some((1, 25, 5)));
        assert_eq!(parse_version("1.24.0-rc1"), Some((1, 24, 0)));
        assert!(parse_version("v1.25.5") >= parse_version("1.24.0"));
        assert!(parse_version("v1.9.9") < parse_version("1.24.0"));
    }
}
