mod compat;
mod config;
mod env;
mod preflight;
mod runner;
mod script;
mod settings;
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
async fn preflight(state: State<'_, AppState>, projects_dir: String) -> Result<Vec<preflight::CheckResult>, String> {
    let (path, compat) = (state.exec.path.clone(), state.compat.clone());
    tauri::async_runtime::spawn_blocking(move || preflight::run(&path, &compat, Path::new(&projects_dir)))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn existing_projects(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let path = state.exec.path.clone();
    tauri::async_runtime::spawn_blocking(move || preflight::existing_projects(&path))
        .await
        .map_err(|e| e.to_string())
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
            plan_project,
            export_script,
            start_run,
            cancel_run,
            delete_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
