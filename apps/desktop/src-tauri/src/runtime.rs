use std::{
    collections::VecDeque,
    env,
    error::Error,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
};

use serde::Serialize;
use tauri::{path::BaseDirectory, AppHandle, Manager, WebviewWindow};

#[cfg(not(debug_assertions))]
const DEV_RUNTIME_ROOT: &str = "../../../submodules/deepseek-harness";
const RUNTIME_SCRIPT: &str = "runtime/deepseek-harness-web.mjs";
const RUNTIME_ROOT_ENV: &str = "DEEPSEEK_HARNESS_RUNTIME_ROOT";
const NODE_BINARY_ENV: &str = "DEEPSEEK_HARNESS_NODE_BINARY";
const PNPM_BINARY_ENV: &str = "DEEPSEEK_HARNESS_PNPM_BINARY";
const WINDOW_LABEL: &str = "main";
const DEV_WEB_URL: &str = "http://127.0.0.1:1420";
const MAX_LOG_LINES: usize = 500;

#[derive(Clone, Serialize)]
pub struct RuntimeInfo {
    pub status: String,
    pub web_url: Option<String>,
    pub runtime_root: Option<String>,
    pub node_binary: Option<String>,
    pub pnpm_binary: Option<String>,
    pub dsh_home: String,
    pub last_error: Option<String>,
    pub log_tail: Vec<String>,
}

#[derive(Serialize)]
pub struct CommandResult {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

struct RuntimeInner {
    child: Option<Child>,
    status: String,
    web_url: Option<String>,
    runtime_root: Option<PathBuf>,
    node_binary: Option<PathBuf>,
    pnpm_binary: Option<PathBuf>,
    last_error: Option<String>,
    logs: VecDeque<String>,
}

pub struct RuntimeManager {
    app: AppHandle,
    inner: Mutex<RuntimeInner>,
}

impl RuntimeManager {
    pub fn new(app: AppHandle) -> Self {
        let runtime_root = resolve_runtime_root(&app).ok();
        let node_binary = resolve_node_binary(&app).ok();
        let pnpm_binary = resolve_pnpm_binary(&app).ok();
        let status = if cfg!(debug_assertions) {
            "running".to_string()
        } else {
            "stopped".to_string()
        };
        let web_url = if cfg!(debug_assertions) {
            Some(DEV_WEB_URL.to_string())
        } else {
            None
        };

        Self {
            app,
            inner: Mutex::new(RuntimeInner {
                child: None,
                status,
                web_url,
                runtime_root,
                node_binary,
                pnpm_binary,
                last_error: None,
                logs: VecDeque::new(),
            }),
        }
    }

    pub fn start_web(&self) -> Result<(), Box<dyn Error>> {
        if cfg!(debug_assertions) {
            self.set_debug_running();
            if let Some(window) = self.app.get_webview_window(WINDOW_LABEL) {
                open_url(&window, DEV_WEB_URL)?;
            }
            return Ok(());
        }

        let mut inner = self.inner.lock().map_err(|_| "runtime state poisoned")?;
        if inner.child.is_some() {
            return Ok(());
        }

        let runtime_root = resolve_runtime_root(&self.app)?;
        let launcher = resolve_runtime_script(&self.app)?;
        let node = resolve_node_binary(&self.app)?;
        let pnpm = resolve_pnpm_binary(&self.app).ok();

        if let Some(window) = self.app.get_webview_window(WINDOW_LABEL) {
            render_status(
                &window,
                "Launching harness",
                "Starting the bundled web profile.",
            )?;
        }

        let mut command = Command::new(&node);
        command
            .arg(&launcher)
            .arg("--profile")
            .arg("web")
            .arg("--port")
            .arg("0")
            .env(RUNTIME_ROOT_ENV, &runtime_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        apply_runtime_env(&mut command, &runtime_root, pnpm.as_ref());

        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or("runtime launcher stdout is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("runtime launcher stderr is unavailable")?;

        inner.status = "starting".to_string();
        inner.web_url = None;
        inner.runtime_root = Some(runtime_root);
        inner.node_binary = Some(node);
        inner.pnpm_binary = pnpm;
        inner.last_error = None;
        inner.child = Some(child);
        drop(inner);

        self.spawn_reader(stdout, true);
        self.spawn_reader(stderr, false);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), Box<dyn Error>> {
        let mut inner = self.inner.lock().map_err(|_| "runtime state poisoned")?;
        if let Some(mut child) = inner.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if !cfg!(debug_assertions) {
            inner.status = "stopped".to_string();
            inner.web_url = None;
        }
        Ok(())
    }

    pub fn restart(&self) -> Result<(), Box<dyn Error>> {
        self.stop()?;
        self.start_web()
    }

    pub fn info(&self) -> RuntimeInfo {
        let inner = self.inner.lock().expect("runtime state poisoned");
        RuntimeInfo {
            status: inner.status.clone(),
            web_url: inner.web_url.clone(),
            runtime_root: inner
                .runtime_root
                .as_ref()
                .map(|path| path.display().to_string()),
            node_binary: inner
                .node_binary
                .as_ref()
                .map(|path| path.display().to_string()),
            pnpm_binary: inner
                .pnpm_binary
                .as_ref()
                .map(|path| path.display().to_string()),
            dsh_home: dsh_home().display().to_string(),
            last_error: inner.last_error.clone(),
            log_tail: inner.logs.iter().cloned().collect(),
        }
    }

    pub fn run_plugin_command(
        &self,
        profile: &str,
        args: &[String],
    ) -> Result<CommandResult, Box<dyn Error>> {
        let runtime_root = resolve_runtime_root(&self.app)?;
        let launcher = resolve_runtime_script(&self.app)?;
        let node = resolve_node_binary(&self.app)?;
        let pnpm = resolve_pnpm_binary(&self.app).ok();

        let mut command = Command::new(node);
        command
            .arg(launcher)
            .arg("plugin")
            .arg("--profile")
            .arg(profile)
            .args(args)
            .env(RUNTIME_ROOT_ENV, &runtime_root)
            .current_dir(&runtime_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        apply_runtime_env(&mut command, &runtime_root, pnpm.as_ref());

        let output = command.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let result = CommandResult {
            success: output.status.success(),
            code: output.status.code(),
            stdout,
            stderr,
        };
        self.push_log(format!(
            "$ dsh plugin --profile {profile} {}\n{}{}",
            args.join(" "),
            result.stdout,
            result.stderr
        ));
        Ok(result)
    }

    fn spawn_reader<R>(&self, reader: R, watch_url: bool)
    where
        R: std::io::Read + Send + 'static,
    {
        let app = self.app.clone();
        thread::spawn(move || {
            let reader = BufReader::new(reader);
            for line in reader.lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(err) => {
                        set_runtime_failed(
                            &app,
                            format!("Deepseek Harness runtime stopped unexpectedly.\n\n{err}"),
                        );
                        return;
                    }
                };
                push_runtime_log(&app, line.clone());
                if watch_url {
                    if let Some(url) = extract_web_url(&line) {
                        set_runtime_running(&app, &url);
                        if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
                            let _ = open_url(&window, &url);
                        }
                    }
                }
            }
            if watch_url {
                set_runtime_failed(
                    &app,
                    "Deepseek Harness runtime exited before it reported a browser URL.".to_string(),
                );
            }
        });
    }

    fn set_debug_running(&self) {
        let mut inner = self.inner.lock().expect("runtime state poisoned");
        inner.status = "running".to_string();
        inner.web_url = Some(DEV_WEB_URL.to_string());
    }

    fn push_log(&self, line: String) {
        let mut inner = self.inner.lock().expect("runtime state poisoned");
        while inner.logs.len() >= MAX_LOG_LINES {
            inner.logs.pop_front();
        }
        inner.logs.push_back(line);
    }
}

impl Drop for RuntimeManager {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(mut child) = inner.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn set_runtime_running(app: &AppHandle, url: &str) {
    if let Some(manager) = app.try_state::<RuntimeManager>() {
        if let Ok(mut inner) = manager.inner.lock() {
            inner.status = "running".to_string();
            inner.web_url = Some(url.to_string());
            inner.last_error = None;
        }
    }
}

fn set_runtime_failed(app: &AppHandle, message: String) {
    if let Some(manager) = app.try_state::<RuntimeManager>() {
        if let Ok(mut inner) = manager.inner.lock() {
            inner.status = "failed".to_string();
            inner.last_error = Some(message.clone());
        }
    }
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = render_status(&window, "Deepseek Harness", &message);
    }
}

fn push_runtime_log(app: &AppHandle, line: String) {
    if let Some(manager) = app.try_state::<RuntimeManager>() {
        manager.push_log(line);
    }
}

fn resolve_runtime_root(app: &AppHandle) -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(root) = app
        .path()
        .resolve("runtime/deepseek-harness", BaseDirectory::Resource)
    {
        if root.exists() {
            return Ok(root);
        }
    }

    #[cfg(not(debug_assertions))]
    {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(DEV_RUNTIME_ROOT);
        if source_root.exists() {
            return Ok(fs::canonicalize(source_root)?);
        }
    }

    let dev_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../submodules/deepseek-harness");
    if dev_root.exists() {
        return Ok(fs::canonicalize(dev_root)?);
    }

    Err("Deepseek Harness runtime root was not bundled".into())
}

fn resolve_runtime_script(app: &AppHandle) -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(script) = app.path().resolve(RUNTIME_SCRIPT, BaseDirectory::Resource) {
        if script.exists() {
            return Ok(script);
        }
    }

    let source_script = Path::new(env!("CARGO_MANIFEST_DIR")).join(RUNTIME_SCRIPT);
    if source_script.exists() {
        return Ok(fs::canonicalize(source_script)?);
    }

    Err("Deepseek Harness runtime launcher script was not bundled".into())
}

fn resolve_node_binary(app: &AppHandle) -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(binary) = app
        .path()
        .resolve(node_binary_relative_path(), BaseDirectory::Resource)
    {
        if binary.exists() {
            return Ok(binary);
        }
    }

    if let Ok(binary) = env::var(NODE_BINARY_ENV) {
        return Ok(PathBuf::from(binary));
    }

    Ok(PathBuf::from("node"))
}

fn resolve_pnpm_binary(app: &AppHandle) -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(binary) = app
        .path()
        .resolve(pnpm_binary_relative_path(), BaseDirectory::Resource)
    {
        if binary.exists() {
            return Ok(binary);
        }
    }

    if let Ok(binary) = env::var(PNPM_BINARY_ENV) {
        return Ok(PathBuf::from(binary));
    }

    Ok(PathBuf::from(if cfg!(windows) {
        "pnpm.cmd"
    } else {
        "pnpm"
    }))
}

fn apply_runtime_env(command: &mut Command, runtime_root: &Path, pnpm: Option<&PathBuf>) {
    let mut paths = Vec::new();
    paths.push(runtime_root.join("node_modules/.bin"));
    if let Some(parent) = pnpm.and_then(|path| path.parent()) {
        paths.push(parent.to_path_buf());
    }
    if let Ok(existing) = env::var("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(path) = env::join_paths(paths) {
        command.env("PATH", path);
    }
    command.env("DSH_HOME", dsh_home());
}

fn dsh_home() -> PathBuf {
    if let Ok(home) = env::var("DSH_HOME") {
        if !home.is_empty() {
            return PathBuf::from(home);
        }
    }
    home_dir().join(".dsh")
}

fn home_dir() -> PathBuf {
    env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn node_binary_relative_path() -> &'static str {
    if cfg!(windows) {
        "runtime/node/node.exe"
    } else {
        "runtime/node/node"
    }
}

fn pnpm_binary_relative_path() -> &'static str {
    if cfg!(windows) {
        "runtime/pnpm/pnpm.cmd"
    } else {
        "runtime/pnpm/pnpm"
    }
}

fn extract_web_url(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|token| token.starts_with("http://127.0.0.1:"))
        .map(ToOwned::to_owned)
}

fn render_status(window: &WebviewWindow, title: &str, body: &str) -> Result<(), Box<dyn Error>> {
    let title = serde_json::to_string(title)?;
    let body = serde_json::to_string(body)?;
    window.eval(&format!(
    "document.title = {title};\nconst titleEl = document.getElementById('boot-title');\nconst bodyEl = document.getElementById('boot-body');\nif (titleEl) titleEl.textContent = {title};\nif (bodyEl) bodyEl.textContent = {body};",
  ))?;
    Ok(())
}

fn open_url(window: &WebviewWindow, url: &str) -> Result<(), Box<dyn Error>> {
    let url = serde_json::to_string(url)?;
    window.eval(&format!("window.location.replace({url});"))?;
    Ok(())
}
