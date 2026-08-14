use std::error::Error;

use tauri::AppHandle;

#[cfg(not(debug_assertions))]
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

#[cfg(not(debug_assertions))]
use tauri::{path::BaseDirectory, Manager, WebviewWindow};

#[cfg(not(debug_assertions))]
const DEV_RUNTIME_ROOT: &str = "../../../submodules/deepseek-harness";
#[cfg(not(debug_assertions))]
const RUNTIME_SCRIPT: &str = "runtime/deepseek-harness-web.mjs";
#[cfg(not(debug_assertions))]
const RUNTIME_ROOT_ENV: &str = "DEEPSEEK_HARNESS_RUNTIME_ROOT";
#[cfg(not(debug_assertions))]
const NODE_BINARY_ENV: &str = "DEEPSEEK_HARNESS_NODE_BINARY";
#[cfg(not(debug_assertions))]
const WINDOW_LABEL: &str = "main";

#[cfg(debug_assertions)]
pub struct RuntimeGuard;

#[cfg(not(debug_assertions))]
pub struct RuntimeGuard {
    child: Arc<Mutex<Option<Child>>>,
}

#[cfg(not(debug_assertions))]
impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

pub fn bootstrap(app: AppHandle) -> Result<Option<RuntimeGuard>, Box<dyn Error>> {
    #[cfg(debug_assertions)]
    {
        let _ = app;
        return Ok(None);
    }

    #[cfg(not(debug_assertions))]
    {
        let guard = spawn_release_runtime(app)?;
        Ok(Some(guard))
    }
}

#[cfg(not(debug_assertions))]
fn spawn_release_runtime(app: AppHandle) -> Result<RuntimeGuard, Box<dyn Error>> {
    let window = app
        .get_webview_window(WINDOW_LABEL)
        .ok_or("main window is missing")?;
    render_status(
        &window,
        "Launching harness",
        "Starting the bundled web profile.",
    )?;

    let runtime_root = runtime_root(&app)?;
    let launcher = runtime_script(&app)?;
    let node = node_binary(&app)?;

    let mut command = Command::new(node);
    command
        .arg(launcher)
        .arg("--profile")
        .arg("web")
        .arg("--port")
        .arg("0")
        .env(RUNTIME_ROOT_ENV, &runtime_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or("runtime launcher stdout is unavailable")?;
    let child = Arc::new(Mutex::new(Some(child)));
    let app_for_thread = app.clone();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(err) => {
                    show_runtime_error(
                        &app_for_thread,
                        &format!("Deepseek Harness runtime stopped unexpectedly.\n\n{err}"),
                    );
                    return;
                }
            };
            if let Some(url) = extract_web_url(&line) {
                if let Some(window) = app_for_thread.get_webview_window(WINDOW_LABEL) {
                    let _ = open_url(&window, &url);
                }
                return;
            }
        }

        show_runtime_error(
            &app_for_thread,
            "Deepseek Harness runtime exited before it reported a browser URL.",
        );
    });

    Ok(RuntimeGuard { child })
}

#[cfg(not(debug_assertions))]
fn runtime_root(app: &AppHandle) -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(root) = app
        .path()
        .resolve("runtime/deepseek-harness", BaseDirectory::Resource)
    {
        if root.exists() {
            return Ok(root);
        }
    }

    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(DEV_RUNTIME_ROOT);
    if source_root.exists() {
        return Ok(fs::canonicalize(source_root)?);
    }

    Err("Deepseek Harness runtime root was not bundled".into())
}

#[cfg(not(debug_assertions))]
fn runtime_script(app: &AppHandle) -> Result<PathBuf, Box<dyn Error>> {
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

#[cfg(not(debug_assertions))]
fn node_binary(app: &AppHandle) -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(binary) = app
        .path()
        .resolve(node_binary_relative_path(), BaseDirectory::Resource)
    {
        if binary.exists() {
            return Ok(binary);
        }
    }

    if let Ok(binary) = std::env::var(NODE_BINARY_ENV) {
        return Ok(PathBuf::from(binary));
    }

    Ok(PathBuf::from("node"))
}

#[cfg(not(debug_assertions))]
fn node_binary_relative_path() -> &'static str {
    if cfg!(windows) {
        "runtime/node/node.exe"
    } else {
        "runtime/node/node"
    }
}

#[cfg(not(debug_assertions))]
fn extract_web_url(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|token| token.starts_with("http://127.0.0.1:"))
        .map(ToOwned::to_owned)
}

#[cfg(not(debug_assertions))]
fn render_status(window: &WebviewWindow, title: &str, body: &str) -> Result<(), Box<dyn Error>> {
    let title = serde_json::to_string(title)?;
    let body = serde_json::to_string(body)?;
    window.eval(&format!(
    "document.title = {title};\nconst titleEl = document.getElementById('boot-title');\nconst bodyEl = document.getElementById('boot-body');\nif (titleEl) titleEl.textContent = {title};\nif (bodyEl) bodyEl.textContent = {body};",
  ))?;
    Ok(())
}

#[cfg(not(debug_assertions))]
fn show_runtime_error(app: &AppHandle, message: &str) {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = render_status(&window, "Deepseek Harness", message);
    }
}

#[cfg(not(debug_assertions))]
fn open_url(window: &WebviewWindow, url: &str) -> Result<(), Box<dyn Error>> {
    let url = serde_json::to_string(url)?;
    window.eval(&format!("window.location.replace({url});"))?;
    Ok(())
}
