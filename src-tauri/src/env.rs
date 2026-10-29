use std::process::Command;

const MARKER: &str = "__SITE_STARTER_PATH__";

/// Apps launched from Finder or the Dock inherit a bare PATH, so read the
/// one the user's login shell builds. Falls back to common install dirs.
pub fn login_shell_path() -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let script = format!("printf '{MARKER}%s{MARKER}' \"$PATH\"");
    let from_shell = Command::new(&shell)
        .args(["-ilc", &script])
        .output()
        .ok()
        .and_then(|o| {
            let out = String::from_utf8_lossy(&o.stdout).into_owned();
            out.split(MARKER).nth(1).map(str::to_string)
        })
        .filter(|p| !p.is_empty());

    let current = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<String> = Vec::new();
    for dir in from_shell
        .iter()
        .flat_map(|p| p.split(':'))
        .chain(current.split(':'))
        .chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"])
    {
        if !dir.is_empty() && !parts.iter().any(|p| p == dir) {
            parts.push(dir.to_string());
        }
    }
    parts.join(":")
}
