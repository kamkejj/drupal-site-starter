mod clone;
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
mod updates;

use compat::Compat;
use config::{FieldError, Project, ProjectInput};
use runner::{NativeExecutor, Run, RunEvent, RunState, Secret};
use serde::{Deserialize, Serialize};
use settings::AppSettings;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::watch;

struct AppState {
    /// Bundled, or a newer downloaded copy; replaced when an update arrives.
    compat: RwLock<Compat>,
    exec: NativeExecutor,
    cancel: Mutex<Option<watch::Sender<bool>>>,
    /// The app update found by the last check, kept for install.
    update: Mutex<Option<tauri_plugin_updater::Update>>,
}

impl AppState {
    fn compat(&self) -> Compat {
        self.compat.read().expect("lock").clone()
    }
}

fn data_file(app: &AppHandle, parts: &[&str]) -> Result<PathBuf, String> {
    let mut p = app.path().app_data_dir().map_err(|e| e.to_string())?;
    p.extend(parts);
    Ok(p)
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    data_file(app, &["settings.json"])
}

fn password_file(app: &AppHandle) -> Result<PathBuf, String> {
    data_file(app, &["admin-password"])
}

fn presets_file(app: &AppHandle) -> Result<PathBuf, String> {
    data_file(app, &["presets.json"])
}

fn compat_cache_file(app: &AppHandle) -> Result<PathBuf, String> {
    data_file(app, &["compat.json"])
}

/// The updater's public key in `tauri.conf.json`, which also signs compat.json.
fn update_pubkey(app: &AppHandle) -> Result<String, String> {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u["pubkey"].as_str())
        .map(str::to_string)
        .ok_or_else(|| "tauri.conf.json has no updater pubkey.".to_string())
}

fn run_state_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    data_file(app, &["runs", &format!("{name}.json")])
}

fn parse(state: &AppState, input: &ProjectInput) -> Result<Project, Vec<FieldError>> {
    config::parse(input, &state.compat())
}

/// A run the Run screen drives: creating a new site or cloning one.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Job {
    Create { input: ProjectInput },
    Clone { input: clone::CloneInput },
}

struct Resolved {
    name: String,
    dir: PathBuf,
    steps: Vec<steps::Step>,
    script: Option<String>,
    needs_password: bool,
}

fn resolve(state: &AppState, job: &Job, resume: bool) -> Result<Resolved, Vec<FieldError>> {
    let compat = state.compat();
    Ok(match job {
        Job::Create { input } => {
            let p = config::parse(input, &compat)?;
            let steps = steps::build(&p, &compat);
            Resolved { script: Some(script::render(&p, &steps)), name: p.name, dir: p.dir, steps, needs_password: true }
        }
        Job::Clone { input } => {
            let p = clone::parse(input, &compat)?;
            if !resume {
                clone::check_folder(&p.dir).map_err(|e| vec![e])?;
            }
            Resolved { steps: clone::build(&p), needs_password: p.needs_password(), name: p.name, dir: p.dir, script: None }
        }
    })
}

fn resolve_err(state: &AppState, job: &Job, resume: bool) -> Result<Resolved, String> {
    resolve(state, job, resume).map_err(|e| e.into_iter().map(|f| f.message).collect::<Vec<_>>().join(" "))
}

#[tauri::command]
fn get_compat(state: State<AppState>) -> Compat {
    state.compat()
}

#[tauri::command]
fn suggested_clone_name(url: String) -> String {
    clone::suggested_name(&url)
}

#[tauri::command]
async fn check_compat_update(app: AppHandle, state: State<'_, AppState>) -> Result<updates::CompatUpdate, String> {
    let url = updates::endpoint(&AppSettings::load(&settings_file(&app)?).update_url, "compat.json")?;
    let text = updates::download(&url).await?;
    let signature = updates::download(&format!("{url}.sig")).await?;
    let (next, update) = updates::accept_compat(&state.compat(), &text, &signature, &compat_cache_file(&app)?, &update_pubkey(&app)?)?;
    *state.compat.write().expect("lock") = next;
    Ok(update)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppUpdate {
    version: String,
    current_version: String,
    notes: Option<String>,
}

#[tauri::command]
async fn check_app_update(app: AppHandle, state: State<'_, AppState>) -> Result<Option<AppUpdate>, String> {
    let url = updates::endpoint(&AppSettings::load(&settings_file(&app)?).update_url, "latest.json")?;
    let url = tauri::Url::parse(&url).map_err(|e| e.to_string())?;
    let updater = app.updater_builder().endpoints(vec![url]).map_err(|e| e.to_string())?.build().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| format!("Could not check for app updates: {e}"))?;
    let info = update.as_ref().map(|u| AppUpdate { version: u.version.clone(), current_version: u.current_version.clone(), notes: u.body.clone() });
    *state.update.lock().expect("lock") = update;
    Ok(info)
}

/// Downloads and installs the update the last check found, then restarts.
#[tauri::command]
async fn install_app_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let update = state.update.lock().expect("lock").take().ok_or("Check for updates first.")?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| format!("Update failed: {e}"))?;
    app.restart()
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
        has_password: settings::admin_password(&password_file(&app)?).is_some(),
    })
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings, password: Option<String>) -> Result<(), String> {
    settings.save(&settings_file(&app)?).map_err(|e| e.to_string())?;
    match password.filter(|p| !p.is_empty()) {
        Some(p) => settings::set_admin_password(&password_file(&app)?, &p).map_err(|e| e.to_string()),
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
    let (path, compat) = (state.exec.path.clone(), state.compat());
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
async fn port_user(state: State<'_, AppState>, port: u16) -> Result<Option<String>, String> {
    let path = state.exec.path.clone();
    tauri::async_runtime::spawn_blocking(move || sites::port_user(&sites::list(&path), port))
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
    name: String,
    steps: Vec<steps::Step>,
    script: Option<String>,
    dir: String,
    folder_warning: Option<String>,
    needs_password: bool,
}

#[tauri::command]
fn plan_project(state: State<AppState>, job: Job) -> Result<Plan, Vec<FieldError>> {
    let r = resolve(&state, &job, false)?;
    let folder_warning = std::fs::read_dir(&r.dir).ok().map(|mut entries| {
        if entries.next().is_some() {
            format!("{} already exists and isn't empty.", r.dir.display())
        } else {
            format!("{} already exists.", r.dir.display())
        }
    });
    Ok(Plan { name: r.name, script: r.script, dir: r.dir.display().to_string(), steps: r.steps, folder_warning, needs_password: r.needs_password })
}

#[tauri::command]
fn export_script(state: State<AppState>, input: ProjectInput, path: String) -> Result<(), String> {
    let p = parse(&state, &input).map_err(|e| format!("{e:?}"))?;
    let script = script::render(&p, &steps::build(&p, &state.compat()));
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
    job: Job,
    password: Option<String>,
    resume: bool,
    on_event: Channel<RunEvent>,
) -> Result<String, String> {
    let p = resolve_err(&state, &job, resume)?;
    let saved = password_file(&app)?;
    let password = match password.filter(|p| !p.is_empty()).or_else(|| settings::admin_password(&saved)) {
        Some(pw) => pw,
        None if !p.needs_password => String::new(),
        None => return Err("Set an admin password in Project defaults or for this project.".into()),
    };
    let steps = p.steps;
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
    if valid { Ok(ide) } else { Err(format!("Set an IDE command in Project defaults, such as code or phpstorm (got {ide:?}).")) }
}

#[tauri::command]
async fn open_in_ide(app: AppHandle, state: State<'_, AppState>, job: Job) -> Result<(), String> {
    let p = resolve_project(&state, &job)?;
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
async fn login_link(state: State<'_, AppState>, job: Job) -> Result<String, String> {
    let p = resolve_project(&state, &job)?;
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

/// The name and folder of a job whose run already started.
fn resolve_project(state: &AppState, job: &Job) -> Result<Resolved, String> {
    resolve_err(state, job, true)
}

/// `ddev delete --omit-snapshot --yes` plus removing the folder. Refuses to remove a folder that
/// isn't this DDEV project, or one this app's run didn't start, so a wrong
/// path can't wipe unrelated files.
#[tauri::command]
async fn delete_project(app: AppHandle, state: State<'_, AppState>, job: Job) -> Result<(), String> {
    let p = resolve_project(&state, &job)?;
    let ddev_config = std::fs::read_to_string(p.dir.join(".ddev/config.yaml")).unwrap_or_default();
    let ours = ddev_config.lines().any(|l| l.trim() == format!("name: {}", p.name));
    let empty = std::fs::read_dir(&p.dir).map(|mut d| d.next().is_none()).unwrap_or(true);
    let state_file = run_state_file(&app, &p.name)?;
    let made_here = RunState::load(&state_file).completed.iter().any(|id| id == "mkdir" || id == "git-clone");
    if ours {
        let status = tokio::process::Command::new("ddev")
            .args(["delete", "--omit-snapshot", "--yes", &p.name])
            .env("PATH", &state.exec.path)
            .env("DDEV_NONINTERACTIVE", "true")
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("ddev delete exited with {status}"));
        }
    }
    if ours || empty || made_here {
        if p.dir.exists() {
            std::fs::remove_dir_all(&p.dir).map_err(|e| e.to_string())?;
        }
    } else {
        return Err(format!("{} is not the DDEV project {}, so it was left alone.", p.dir.display(), p.name));
    }
    let _ = std::fs::remove_file(state_file);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let compat = updates::load_compat(&compat_cache_file(app.handle())?, &update_pubkey(app.handle())?);
            app.manage(AppState {
                compat: RwLock::new(compat),
                exec: NativeExecutor { path: env::login_shell_path() },
                cancel: Mutex::new(None),
                update: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_compat,
            suggested_clone_name,
            check_compat_update,
            check_app_update,
            install_app_update,
            machine_name,
            load_settings,
            save_settings,
            preflight,
            existing_projects,
            port_user,
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
        ]);
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu).on_menu_event(|app, event| {
        if event.id() == APP_SETTINGS_MENU_ID {
            let _ = tauri::Emitter::emit(app, "open-app-settings", ());
        }
    });
    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(target_os = "macos")]
const APP_SETTINGS_MENU_ID: &str = "app-settings";

/// The default macOS menu with "Settings…" (⌘,) after About, where macOS apps put it.
#[cfg(target_os = "macos")]
fn app_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::default(app)?;
    let items = menu.items()?;
    if let Some(app_submenu) = items.first().and_then(|item| item.as_submenu()) {
        app_submenu.insert(&MenuItem::with_id(app, APP_SETTINGS_MENU_ID, "Settings…", true, Some("CmdOrCtrl+,"))?, 2)?;
        app_submenu.insert(&PredefinedMenuItem::separator(app)?, 3)?;
    }
    Ok(menu)
}
