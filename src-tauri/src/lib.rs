mod compat;
mod config;
mod env;
mod preflight;
mod presets;
mod runner;
mod script;
mod settings;
mod sites;
mod steps;

use compat::Compat;
use config::{FieldError, Project, ProjectInput};
use runner::{NativeExecutor, Run, RunEvent, Secret};
use serde::Serialize;
use settings::AppSettings;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::sync::watch;

struct AppState {
    compat: Compat,
    exec: NativeExecutor,
    cancel: Mutex<Option<watch::Sender<bool>>>,
}

fn data_file(app: &AppHandle, parts: &[&str]) -> Result<PathBuf, String> {
    let mut p = app.path().app_data_dir().map_err(|e| e.to_string())?;
    p.extend(parts);
    Ok(p)
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    data_file(app, &["settings.json"])
}

fn presets_file(app: &AppHandle) -> Result<PathBuf, String> {
    data_file(app, &["presets.json"])
}

fn run_state_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    data_file(app, &["runs", &format!("{name}.json")])
}

fn parse(state: &AppState, input: &ProjectInput) -> Result<Project, Vec<FieldError>> {
    config::parse(input, &state.compat)
}

#[tauri::command]
fn get_compat(state: State<AppState>) -> Compat {
    state.compat.clone()
}

#[tauri::command]
fn machine_name(display: String) -> String {
    config::machine_name(&display)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LoadedSettings {
    settings: AppSettings,
    has_password: bool,
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<LoadedSettings, String> {
    Ok(LoadedSettings {
        settings: AppSettings::load(&settings_file(&app)?),
        has_password: settings::admin_password().is_some(),
    })
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings, password: Option<String>) -> Result<(), String> {
    settings.save(&settings_file(&app)?).map_err(|e| e.to_string())?;
    match password.filter(|p| !p.is_empty()) {
        Some(p) => settings::set_admin_password(&p),
        None => Ok(()),
    }
}

#[tauri::command]
fn list_presets(app: AppHandle) -> Result<Vec<presets::Preset>, String> {
    Ok(presets::load(&presets_file(&app)?))
}

fn update_presets(app: &AppHandle, change: impl FnOnce(&mut Vec<presets::Preset>)) -> Result<Vec<presets::Preset>, String> {
    let file = presets_file(app)?;
    let mut all = presets::load(&file);
    change(&mut all);
    presets::save(&file, &all)?;
    Ok(all)
}

#[tauri::command]
fn save_preset(app: AppHandle, preset: presets::Preset) -> Result<Vec<presets::Preset>, String> {
    let preset = presets::Preset { name: preset.name.trim().to_string(), ..preset };
    if preset.name.is_empty() {
        return Err("Name the preset.".into());
    }
    update_presets(&app, |all| presets::merge(all, vec![preset]))
}

#[tauri::command]
fn delete_preset(app: AppHandle, name: String) -> Result<Vec<presets::Preset>, String> {
    update_presets(&app, |all| all.retain(|p| p.name != name))
}

#[tauri::command]
fn import_presets(app: AppHandle, path: String) -> Result<Vec<presets::Preset>, String> {
    let incoming = presets::read(Path::new(&path))?;
    update_presets(&app, |all| presets::merge(all, incoming))
}

#[tauri::command]
fn export_presets(app: AppHandle, path: String) -> Result<(), String> {
    presets::write(Path::new(&path), &presets::load(&presets_file(&app)?))
}

#[tauri::command]
async fn preflight(state: State<'_, AppState>, projects_dir: String) -> Result<Vec<preflight::CheckResult>, String> {
    let (path, compat) = (state.exec.path.clone(), state.compat.clone());
    tauri::async_runtime::spawn_blocking(move || preflight::run(&path, &compat, Path::new(&projects_dir)))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn existing_projects(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let path = state.exec.path.clone();
    tauri::async_runtime::spawn_blocking(move || sites::list(&path).into_iter().map(|s| s.name).collect())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn project_url(state: State<'_, AppState>, name: String) -> Result<String, String> {
    let path = state.exec.path.clone();
    let fallback = format!("https://{name}.ddev.site");
    tauri::async_runtime::spawn_blocking(move || preflight::project_url(&path, &name).unwrap_or(fallback))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_sites(state: State<'_, AppState>) -> Result<Vec<sites::Site>, String> {
    let path = state.exec.path.clone();
    tauri::async_runtime::spawn_blocking(move || sites::list(&path))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn site_action(state: State<'_, AppState>, name: String, action: sites::SiteAction) -> Result<(), String> {
    sites::run(&state.exec.path, &name, action).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Plan {
    steps: Vec<steps::Step>,
    script: String,
    dir: String,
    folder_warning: Option<String>,
}

#[tauri::command]
fn plan_project(state: State<AppState>, input: ProjectInput) -> Result<Plan, Vec<FieldError>> {
    let p = parse(&state, &input)?;
    let steps = steps::build(&p, &state.compat);
    let folder_warning = std::fs::read_dir(&p.dir).ok().map(|mut entries| {
        if entries.next().is_some() {
            format!("{} already exists and isn't empty.", p.dir.display())
        } else {
            format!("{} already exists.", p.dir.display())
        }
    });
    Ok(Plan { script: script::render(&p, &steps), dir: p.dir.display().to_string(), steps, folder_warning })
}

#[tauri::command]
fn export_script(state: State<AppState>, input: ProjectInput, path: String) -> Result<(), String> {
    let p = parse(&state, &input).map_err(|e| format!("{e:?}"))?;
    let script = script::render(&p, &steps::build(&p, &state.compat));
    std::fs::write(&path, script).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

#[tauri::command]
async fn start_run(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ProjectInput,
    password: Option<String>,
    resume: bool,
    on_event: Channel<RunEvent>,
) -> Result<String, String> {
    let p = parse(&state, &input).map_err(|e| format!("{e:?}"))?;
    let password = password
        .filter(|p| !p.is_empty())
        .or_else(settings::admin_password)
        .ok_or("Set an admin password in Settings or for this project.")?;
    let steps = steps::build(&p, &state.compat);
    let state_path = run_state_file(&app, &p.name)?;
    if !resume {
        let _ = std::fs::remove_file(&state_path);
    }

    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let log_path = data_file(&app, &["logs", &format!("{}-{stamp}.log", p.name)])?;
    std::fs::create_dir_all(log_path.parent().expect("has parent")).map_err(|e| e.to_string())?;
    let log = Mutex::new(std::fs::File::create(&log_path).map_err(|e| e.to_string())?);

    let (tx, cancel) = watch::channel(false);
    *state.cancel.lock().expect("lock") = Some(tx);
    let secret = Secret(password);
    let run = Run { exec: &state.exec, dir: &p.dir, steps: &steps, secret: &secret, state_path: &state_path, cancel };
    let emit = |event: RunEvent| {
        let line = match &event {
            RunEvent::Log { id, line } => format!("[{id}] {line}"),
            other => serde_json::to_string(other).unwrap_or_default(),
        };
        let _ = writeln!(log.lock().expect("lock"), "{line}");
        let _ = on_event.send(event);
    };
    run.execute(&emit).await;
    *state.cancel.lock().expect("lock") = None;
    Ok(log_path.display().to_string())
}

#[tauri::command]
fn cancel_run(state: State<AppState>) {
    if let Some(tx) = state.cancel.lock().expect("lock").as_ref() {
        let _ = tx.send(true);
    }
}

/// The IDE setting is a command name or an absolute path, spawned with the
/// project folder as its only argument, never run through a shell.
fn ide_program(ide: &str) -> Result<&str, String> {
    let ide = ide.trim();
    let valid = !ide.is_empty() && ide.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
    if valid { Ok(ide) } else { Err(format!("Set an IDE command in Settings, such as code or phpstorm (got {ide:?}).")) }
}

#[tauri::command]
async fn open_in_ide(app: AppHandle, state: State<'_, AppState>, input: ProjectInput) -> Result<(), String> {
    let p = parse(&state, &input).map_err(|e| format!("{e:?}"))?;
    let settings = AppSettings::load(&settings_file(&app)?);
    let ide = ide_program(&settings.ide)?;
    tokio::process::Command::new(ide)
        .arg(&p.dir)
        .env("PATH", &state.exec.path)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not start {ide}: {e}. Is its command-line launcher installed?"))
}

#[tauri::command]
async fn login_link(state: State<'_, AppState>, input: ProjectInput) -> Result<String, String> {
    let p = parse(&state, &input).map_err(|e| format!("{e:?}"))?;
    let out = tokio::process::Command::new("ddev")
        .args(["drush", "user:login", "--no-browser"])
        .current_dir(&p.dir)
        .env("PATH", &state.exec.path)
        .env("DDEV_NONINTERACTIVE", "true")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    match stdout.lines().map(str::trim).rfind(|l| l.starts_with("http")) {
        Some(url) if out.status.success() => Ok(url.to_string()),
        _ => Err(format!("drush user:login failed: {}", String::from_utf8_lossy(&out.stderr).trim())),
    }
}

/// `ddev delete -Oy` plus removing the folder. Refuses to remove a folder that
/// isn't this DDEV project, so a wrong path can't wipe unrelated files.
#[tauri::command]
async fn delete_project(app: AppHandle, state: State<'_, AppState>, input: ProjectInput) -> Result<(), String> {
    let p = parse(&state, &input).map_err(|e| format!("{e:?}"))?;
    let ddev_config = std::fs::read_to_string(p.dir.join(".ddev/config.yaml")).unwrap_or_default();
    let ours = ddev_config.lines().any(|l| l.trim() == format!("name: {}", p.name));
    let empty = std::fs::read_dir(&p.dir).map(|mut d| d.next().is_none()).unwrap_or(true);
    if ours {
        let status = tokio::process::Command::new("ddev")
            .args(["delete", "-Oy", &p.name])
            .env("PATH", &state.exec.path)
            .env("DDEV_NONINTERACTIVE", "true")
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("ddev delete exited with {status}"));
        }
    }
    if ours || empty {
        if p.dir.exists() {
            std::fs::remove_dir_all(&p.dir).map_err(|e| e.to_string())?;
        }
    } else {
        return Err(format!("{} is not the DDEV project {}, so it was left alone.", p.dir.display(), p.name));
    }
    let _ = std::fs::remove_file(run_state_file(&app, &p.name)?);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState {
        compat: Compat::bundled(),
        exec: NativeExecutor { path: env::login_shell_path() },
        cancel: Mutex::new(None),
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_compat,
            machine_name,
            load_settings,
            save_settings,
            preflight,
            existing_projects,
            list_sites,
            site_action,
            project_url,
            plan_project,
            export_script,
            start_run,
            cancel_run,
            delete_project,
            open_in_ide,
            list_presets,
            save_preset,
            delete_preset,
            import_presets,
            export_presets,
            login_link,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
