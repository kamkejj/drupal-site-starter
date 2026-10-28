use crate::steps::{config_sync_line, Action, Check, Retry, Step, DDEV_INCLUDE_MARKER, SETTINGS_PHP};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(••••)")
    }
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: &'static str,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub stdin: Option<String>,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    Exit(i32),
    Spawn(String),
    TimedOut,
    Cancelled,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::Exit(code) => write!(f, "exited with status {code}"),
            CommandError::Spawn(e) => write!(f, "could not start: {e}"),
            CommandError::TimedOut => f.write_str("timed out"),
            CommandError::Cancelled => f.write_str("cancelled"),
        }
    }
}

/// Native today; a WSL executor slots in here for Windows.
pub trait Executor: Send + Sync {
    fn run(
        &self,
        spec: CommandSpec,
        on_line: &mut (dyn FnMut(String) + Send),
        cancel: watch::Receiver<bool>,
    ) -> impl Future<Output = Result<(), CommandError>> + Send;
}

pub struct NativeExecutor {
    pub path: String,
}

impl Executor for NativeExecutor {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: &mut (dyn FnMut(String) + Send),
        mut cancel: watch::Receiver<bool>,
    ) -> Result<(), CommandError> {
        let mut child = Command::new(spec.program)
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .env("PATH", &self.path)
            .env("DDEV_NONINTERACTIVE", "true")
            .stdin(if spec.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| CommandError::Spawn(e.to_string()))?;

        if let (Some(input), Some(mut stdin)) = (spec.stdin, child.stdin.take()) {
            stdin
                .write_all(format!("{input}\n").as_bytes())
                .await
                .map_err(|e| CommandError::Spawn(e.to_string()))?;
        }

        let mut out = BufReader::new(child.stdout.take().expect("piped")).lines();
        let mut err = BufReader::new(child.stderr.take().expect("piped")).lines();
        let (mut out_open, mut err_open) = (true, true);
        let deadline = tokio::time::sleep(spec.timeout);
        tokio::pin!(deadline);

        while out_open || err_open {
            tokio::select! {
                line = out.next_line(), if out_open => match line {
                    Ok(Some(l)) => on_line(l),
                    _ => out_open = false,
                },
                line = err.next_line(), if err_open => match line {
                    Ok(Some(l)) => on_line(l),
                    _ => err_open = false,
                },
                Ok(_) = cancel.wait_for(|c| *c) => return Err(CommandError::Cancelled),
                _ = &mut deadline => return Err(CommandError::TimedOut),
            }
        }

        let status = child.wait().await.map_err(|e| CommandError::Spawn(e.to_string()))?;
        if status.success() { Ok(()) } else { Err(CommandError::Exit(status.code().unwrap_or(-1))) }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunState {
    pub completed: BTreeSet<String>,
}

impl RunState {
    pub fn load(path: &Path) -> RunState {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, serde_json::to_string_pretty(self).expect("serializable"));
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum StepStatus {
    Running,
    Done,
    Skipped,
    Failed { message: String, retry: Retry },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunEvent {
    Step { id: String, #[serde(flatten)] status: StepStatus },
    Log { id: String, line: String },
    Finished { ok: bool, cancelled: bool },
}

pub struct Run<'a, E: Executor> {
    pub exec: &'a E,
    pub dir: &'a Path,
    pub steps: &'a [Step],
    pub secret: &'a Secret,
    pub state_path: &'a Path,
    pub cancel: watch::Receiver<bool>,
}

fn redact(line: String, secret: &Secret) -> String {
    if secret.0.is_empty() { line } else { line.replace(&secret.0, "••••••") }
}

fn check_passes(dir: &Path, check: &Check) -> bool {
    match check {
        Check::None => false,
        Check::Exists { path } => dir.join(path).exists(),
        Check::Contains { path, needle } => std::fs::read_to_string(dir.join(path))
            .is_ok_and(|s| s.contains(needle.as_str())),
    }
}

pub fn insert_config_sync_dir(settings: &str, dir: &str) -> Result<String, String> {
    let at = settings
        .find(DDEV_INCLUDE_MARKER)
        .ok_or_else(|| format!("{SETTINGS_PHP} has no DDEV include to insert above."))?;
    Ok(format!("{}{}\n\n{}", &settings[..at], config_sync_line(dir), &settings[at..]))
}

impl<E: Executor> Run<'_, E> {
    async fn perform(&self, id: &str, action: &Action, timeout: Duration, emit: &(dyn Fn(RunEvent) + Sync)) -> Result<(), String> {
        let command = |program, args: &Vec<String>, stdin| CommandSpec {
            program,
            args: args.clone(),
            cwd: self.dir.to_path_buf(),
            stdin,
            timeout,
        };
        let spec = match action {
            Action::Ddev { args, stdin_secret } => {
                command("ddev", args, stdin_secret.then(|| self.secret.0.clone()))
            }
            Action::Git { args } => command("git", args, None),
            Action::CreateProjectDir => {
                return std::fs::create_dir_all(self.dir).map_err(|e| e.to_string());
            }
            Action::Mkdir { path } => {
                return std::fs::create_dir_all(self.dir.join(path)).map_err(|e| e.to_string());
            }
            Action::WriteFile { path, contents } => {
                let target = self.dir.join(path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                return std::fs::write(target, contents).map_err(|e| e.to_string());
            }
            Action::SetConfigSyncDir { dir } => {
                let path = self.dir.join(SETTINGS_PHP);
                let current = std::fs::read_to_string(&path).map_err(|e| format!("{SETTINGS_PHP}: {e}"))?;
                return std::fs::write(&path, insert_config_sync_dir(&current, dir)?).map_err(|e| e.to_string());
            }
        };
        let mut on_line = |line: String| {
            emit(RunEvent::Log { id: id.to_string(), line: redact(line, self.secret) })
        };
        self.exec
            .run(spec, &mut on_line, self.cancel.clone())
            .await
            .map_err(|e| e.to_string())
    }

    /// Runs every step not already recorded as done. When resuming onto a
    /// step whose retry rule needs a cleanup, the cleanup runs first.
    pub async fn execute(&self, emit: &(dyn Fn(RunEvent) + Sync)) -> bool {
        let mut state = RunState::load(self.state_path);
        let mut first_pending = true;
        for s in self.steps {
            if *self.cancel.borrow() {
                emit(RunEvent::Finished { ok: false, cancelled: true });
                return false;
            }
            if state.completed.contains(&s.id) || check_passes(self.dir, &s.check) {
                emit(RunEvent::Step { id: s.id.clone(), status: StepStatus::Skipped });
                continue;
            }
            emit(RunEvent::Step { id: s.id.clone(), status: StepStatus::Running });
            let timeout = Duration::from_secs(s.timeout_secs);
            let mut result = Ok(());
            if first_pending
                && !state.completed.is_empty()
                && let Retry::CleanupFirst { cleanup } = &s.retry
            {
                result = self.perform(&s.id, cleanup, timeout, emit).await;
            }
            first_pending = false;
            if result.is_ok() {
                result = self.perform(&s.id, &s.action, timeout, emit).await;
            }
            match result {
                Ok(()) => {
                    state.completed.insert(s.id.clone());
                    state.save(self.state_path);
                    emit(RunEvent::Step { id: s.id.clone(), status: StepStatus::Done });
                }
                Err(message) => {
                    let cancelled = *self.cancel.borrow();
                    emit(RunEvent::Step { id: s.id.clone(), status: StepStatus::Failed { message, retry: s.retry.clone() } });
                    emit(RunEvent::Finished { ok: false, cancelled });
                    return false;
                }
            }
        }
        emit(RunEvent::Finished { ok: true, cancelled: false });
        true
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records each command; fails the first command whose args contain `fail_on`.
    #[derive(Default)]
    pub struct Recorder {
        pub calls: Mutex<Vec<(String, Vec<String>, Option<String>)>>,
        pub fail_on: Mutex<Option<String>>,
    }

    impl Executor for Recorder {
        async fn run(
            &self,
            spec: CommandSpec,
            on_line: &mut (dyn FnMut(String) + Send),
            _cancel: watch::Receiver<bool>,
        ) -> Result<(), CommandError> {
            if let Some(secret) = &spec.stdin {
                on_line(format!("echoed {secret}"));
            }
            self.calls.lock().unwrap().push((spec.program.into(), spec.args.clone(), spec.stdin.clone()));
            let mut fail = self.fail_on.lock().unwrap();
            if fail.as_ref().is_some_and(|f| spec.args.contains(f)) {
                *fail = None;
                return Err(CommandError::Exit(1));
            }
            Ok(())
        }
    }

    fn ddev_step(id: &str, arg: &str, retry: Retry) -> Step {
        Step {
            id: id.into(),
            title: id.into(),
            action: Action::Ddev { args: vec![arg.into()], stdin_secret: id == "pw" },
            timeout_secs: 5,
            check: Check::None,
            retry,
        }
    }

    fn steps() -> Vec<Step> {
        vec![
            ddev_step("start", "start", Retry::Rerun),
            ddev_step("install", "site:install", Retry::CleanupFirst {
                cleanup: Action::Ddev { args: vec!["sql:drop".into()], stdin_secret: false },
            }),
            ddev_step("pw", "exec", Retry::Rerun),
        ]
    }

    #[tokio::test]
    async fn resume_skips_done_steps_and_cleans_up_before_retrying() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        let exec = Recorder::default();
        *exec.fail_on.lock().unwrap() = Some("site:install".into());
        let (_tx, cancel) = watch::channel(false);
        let secret = Secret("hunter2".into());
        let steps = steps();
        let run = Run { exec: &exec, dir: tmp.path(), steps: &steps, secret: &secret, state_path: &state_path, cancel };
        let events = Mutex::new(Vec::new());
        let emit = |e: RunEvent| events.lock().unwrap().push(e);

        assert!(!run.execute(&emit).await);
        assert_eq!(RunState::load(&state_path).completed, BTreeSet::from(["start".to_string()]));

        assert!(run.execute(&emit).await);
        let args: Vec<String> = exec.calls.lock().unwrap().iter().map(|c| c.1[0].clone()).collect();
        assert_eq!(args, ["start", "site:install", "sql:drop", "site:install", "exec"]);

        let calls = exec.calls.lock().unwrap();
        assert_eq!(calls.last().unwrap().2.as_deref(), Some("hunter2"), "password goes to stdin");
        assert!(calls.iter().all(|c| !c.1.iter().any(|a| a.contains("hunter2"))), "password never in argv");
        let logged = events.lock().unwrap();
        assert!(logged.contains(&RunEvent::Log { id: "pw".into(), line: "echoed ••••••".into() }), "log is redacted");
    }

    #[test]
    fn config_sync_line_goes_above_the_ddev_include() {
        let settings = format!("<?php\n$databases = [];\n\n{DDEV_INCLUDE_MARKER}\nrequire 'x';\n");
        let out = insert_config_sync_dir(&settings, "../config/sync").unwrap();
        let sync = out.find("$settings['config_sync_directory'] = '../config/sync';").unwrap();
        assert!(sync < out.find(DDEV_INCLUDE_MARKER).unwrap());
        assert!(insert_config_sync_dir("<?php\n", "x").is_err());
    }
}

/// Real DDEV run. `SMOKE_PARENT=/some/dir SMOKE_DRUPAL=11 cargo test smoke -- --ignored --nocapture`
#[cfg(test)]
mod smoke {
    use super::*;
    use crate::compat::Compat;
    use crate::config::{parse, tests::input};

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn smoke_creates_a_real_site() {
        let parent = PathBuf::from(std::env::var("SMOKE_PARENT").expect("SMOKE_PARENT"));
        let mut i = input(&parent);
        i.drupal = std::env::var("SMOKE_DRUPAL").unwrap_or_else(|_| "11".into());
        i.name = format!("ddevcreator-smoke-{}", i.drupal);
        i.services = vec!["mailpit".into(), "adminer".into(), "redis".into()];
        let compat = Compat::bundled();
        let p = parse(&i, &compat).unwrap();
        let steps = crate::steps::build(&p, &compat);
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
    }
}
