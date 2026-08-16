#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod runtime;

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

use runtime::{CommandResult, RuntimeInfo, RuntimeManager};
use serde::{Deserialize, Serialize};
use tauri::{
    http::{header, Response, StatusCode},
    image::Image,
    menu::{
        AboutMetadataBuilder, CheckMenuItemBuilder, Menu, MenuBuilder, MenuItem, MenuItemBuilder,
        PredefinedMenuItem, SubmenuBuilder,
    },
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager, State, Url, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

#[cfg(not(debug_assertions))]
use tauri_plugin_updater::UpdaterExt;

const APP_NAME: &str = "Deepseek Harness";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAIN_WINDOW: &str = "main";
const SETTINGS_WINDOW: &str = "settings";
const TRAY_ID: &str = "main-tray";
const SETTINGS_SCHEME: &str = "dsh-settings";
const SETTINGS_HTML: &str = include_str!("../../src/settings.html");

#[derive(Serialize)]
struct AppInfo {
    app_name: &'static str,
    app_version: &'static str,
    upstream_version: String,
    upstream_commit: String,
}

#[derive(Serialize)]
struct ProfileSummary {
    name: String,
    path: String,
    dependencies: Vec<String>,
    bundles: Vec<String>,
    default_command: String,
}

#[derive(Serialize)]
struct PluginInstallState {
    profile: String,
    package_name: String,
    installed: bool,
    active: bool,
    state: String,
}

#[derive(Serialize)]
struct DesktopSnapshot {
    app_name: &'static str,
    app_version: &'static str,
    upstream_version: String,
    upstream_commit: String,
    current_profile: String,
    runtime: RuntimeInfo,
    profiles: Vec<ProfileSummary>,
}

struct DesktopState {
    current_profile: Mutex<String>,
}

impl DesktopState {
    fn new() -> Self {
        Self {
            current_profile: Mutex::new("web".to_string()),
        }
    }

    fn current_profile(&self) -> String {
        self.current_profile
            .lock()
            .map(|profile| profile.clone())
            .unwrap_or_else(|_| "web".to_string())
    }

    fn set_current_profile(&self, profile: String) -> Result<(), String> {
        if profile.is_empty()
            || profile == "."
            || profile == ".."
            || profile == "node_modules"
            || profile.contains('/')
            || profile.contains('\\')
        {
            return Err(format!("Invalid profile name: {profile}"));
        }
        let mut current = self
            .current_profile
            .lock()
            .map_err(|_| "desktop state poisoned".to_string())?;
        *current = profile;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginOperation {
    profile: String,
    package_spec: String,
    package_name: Option<String>,
}

fn show_message(app: &AppHandle, title: &str, message: impl Into<String>, kind: MessageDialogKind) {
    app.dialog()
        .message(message)
        .title(title)
        .kind(kind)
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}

#[cfg(not(debug_assertions))]
async fn trigger_update_check(app: AppHandle) {
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(err) => {
            show_message(
                &app,
                "Check for Updates",
                format!("Updater unavailable.\n\nCurrent version: {APP_VERSION}\n\n{err}"),
                MessageDialogKind::Error,
            );
            return;
        }
    };

    match updater.check().await {
        Ok(Some(update)) => {
            if let Err(err) = update.download_and_install(|_, _| {}, || {}).await {
                show_message(
                    &app,
                    "Check for Updates",
                    format!("Update install failed.\n\nCurrent version: {APP_VERSION}\n\n{err}"),
                    MessageDialogKind::Error,
                );
                return;
            }
            app.restart();
        }
        Ok(None) => show_message(
            &app,
            "Check for Updates",
            format!("You are using the latest version.\n\nCurrent version: {APP_VERSION}"),
            MessageDialogKind::Info,
        ),
        Err(err) => show_message(
            &app,
            "Check for Updates",
            format!("Update check failed.\n\nCurrent version: {APP_VERSION}\n\n{err}"),
            MessageDialogKind::Error,
        ),
    }
}

#[cfg(debug_assertions)]
async fn trigger_update_check(app: AppHandle) {
    show_message(
        &app,
        "Check for Updates",
        format!("Updater is available in release builds only.\n\nCurrent version: {APP_VERSION}"),
        MessageDialogKind::Info,
    );
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        app_name: APP_NAME,
        app_version: APP_VERSION,
        upstream_version: upstream_version(),
        upstream_commit: upstream_commit(),
    }
}

#[tauri::command]
fn get_runtime_info(runtime: State<'_, RuntimeManager>) -> RuntimeInfo {
    runtime.info()
}

#[tauri::command]
fn get_desktop_state(
    runtime: State<'_, RuntimeManager>,
    desktop: State<'_, DesktopState>,
) -> DesktopSnapshot {
    let runtime_info = runtime.info();
    let profiles_root = PathBuf::from(&runtime_info.dsh_home).join("profiles");
    let profiles = discover_profiles(&profiles_root);
    let mut current_profile = desktop.current_profile();
    if !profiles
        .iter()
        .any(|profile| profile.name == current_profile)
    {
        current_profile = "web".to_string();
        let _ = desktop.set_current_profile(current_profile.clone());
    }
    DesktopSnapshot {
        app_name: APP_NAME,
        app_version: APP_VERSION,
        upstream_version: upstream_version(),
        upstream_commit: upstream_commit(),
        current_profile,
        runtime: runtime_info,
        profiles,
    }
}

#[tauri::command]
fn set_current_profile(
    app: AppHandle,
    desktop: State<'_, DesktopState>,
    profile: String,
) -> Result<(), String> {
    desktop.set_current_profile(profile)?;
    refresh_tray_menu(&app).map_err(|err| err.to_string())
}

#[tauri::command]
fn open_settings(app: AppHandle, page: Option<String>) -> Result<(), String> {
    open_settings_window(&app, page.as_deref()).map_err(|err| err.to_string())
}

#[tauri::command]
fn show_web(app: AppHandle) -> Result<(), String> {
    show_main_window(&app).map_err(|err| err.to_string())
}

#[tauri::command]
fn open_web_in_browser(runtime: State<'_, RuntimeManager>) -> Result<(), String> {
    let info = runtime.info();
    let url = info.web_url.ok_or("Web runtime URL is not ready")?;
    open_external(&url).map_err(|err| err.to_string())
}

#[tauri::command]
fn restart_runtime(runtime: State<'_, RuntimeManager>) -> Result<(), String> {
    runtime.restart().map_err(|err| err.to_string())
}

#[tauri::command]
fn stop_runtime(runtime: State<'_, RuntimeManager>) -> Result<(), String> {
    runtime.stop().map_err(|err| err.to_string())
}

#[tauri::command]
fn copy_diagnostics(runtime: State<'_, RuntimeManager>) -> String {
    let info = runtime.info();
    format!(
        "App: {APP_NAME} {APP_VERSION}\nUpstream version: {}\nUpstream commit: {}\nRuntime status: {}\nWeb URL: {}\nRuntime root: {}\nNode: {}\npnpm: {}\nDSH_HOME: {}\nLast error: {}\n\nLogs:\n{}",
        upstream_version(),
        upstream_commit(),
        info.status,
        info.web_url.unwrap_or_else(|| "not ready".to_string()),
        info.runtime_root.unwrap_or_else(|| "unknown".to_string()),
        info.node_binary.unwrap_or_else(|| "node".to_string()),
        info.pnpm_binary.unwrap_or_else(|| "pnpm".to_string()),
        info.dsh_home,
        info.last_error.unwrap_or_else(|| "none".to_string()),
        info.log_tail.join("\n")
    )
}

#[tauri::command]
fn list_profiles(runtime: State<'_, RuntimeManager>) -> Vec<ProfileSummary> {
    let dsh_home = PathBuf::from(runtime.info().dsh_home);
    let profiles_root = dsh_home.join("profiles");
    discover_profiles(&profiles_root)
}

#[tauri::command]
fn get_plugin_state(
    runtime: State<'_, RuntimeManager>,
    profile: String,
    package_name: String,
) -> PluginInstallState {
    plugin_state_from_profile(&runtime.info().dsh_home, &profile, &package_name)
}

#[tauri::command]
fn install_plugin(
    runtime: State<'_, RuntimeManager>,
    operation: PluginOperation,
) -> Result<CommandResult, String> {
    let result = runtime
        .run_plugin_command(
            &operation.profile,
            &["add".to_string(), operation.package_spec.clone()],
        )
        .map_err(|err| err.to_string())?;
    Ok(result)
}

#[tauri::command]
fn uninstall_plugin(
    runtime: State<'_, RuntimeManager>,
    operation: PluginOperation,
) -> Result<CommandResult, String> {
    let package_name = operation.package_name.unwrap_or(operation.package_spec);
    let result = runtime
        .run_plugin_command(&operation.profile, &["remove".to_string(), package_name])
        .map_err(|err| err.to_string())?;
    Ok(result)
}

#[tauri::command]
fn open_cli_profile(runtime: State<'_, RuntimeManager>, profile: String) -> Result<(), String> {
    let dsh_home = PathBuf::from(runtime.info().dsh_home);
    open_profile_terminal(&dsh_home, &profile).map_err(|err| err.to_string())
}

#[tauri::command]
fn open_profile_shell(runtime: State<'_, RuntimeManager>, profile: String) -> Result<(), String> {
    let dsh_home = PathBuf::from(runtime.info().dsh_home);
    open_profile_terminal(&dsh_home, &profile).map_err(|err| err.to_string())
}

#[tauri::command]
fn open_profile_directory(
    runtime: State<'_, RuntimeManager>,
    profile: String,
) -> Result<(), String> {
    let dsh_home = PathBuf::from(runtime.info().dsh_home);
    let profiles_root = dsh_home.join("profiles");
    let profile_path = profiles_root.join(profile);
    let target = if profile_path.exists() {
        profile_path
    } else {
        profiles_root
    };
    open_external(&target.display().to_string()).map_err(|err| err.to_string())
}

fn setup_shell(app: &mut App) -> Result<(), Box<dyn Error>> {
    let handle = app.handle().clone();
    let runtime = RuntimeManager::new(handle.clone());
    app.manage(runtime);
    app.manage(DesktopState::new());
    handle.state::<RuntimeManager>().start_web()?;

    setup_menu(app)?;
    setup_tray(app)?;
    Ok(())
}

fn setup_menu(app: &mut App) -> Result<(), Box<dyn Error>> {
    let about = PredefinedMenuItem::about(
        app,
        Some("About Deepseek Harness"),
        Some(
            AboutMetadataBuilder::new()
                .name(Some(APP_NAME))
                .version(Some(APP_VERSION))
                .website(Some("https://github.com/javen-yan/deepseek-harness-app"))
                .website_label(Some("Project repository"))
                .copyright(Some("MIT"))
                .icon(Some(Image::from_bytes(include_bytes!(
                    "../icons/icon.png"
                ))?))
                .build(),
        ),
    )?;
    let settings = MenuItemBuilder::with_id("settings", "Settings...")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let plugins = MenuItemBuilder::with_id("plugins", "Plugins...")
        .accelerator("CmdOrCtrl+Shift+P")
        .build(app)?;
    let check_updates = MenuItemBuilder::with_id("check-updates", "Check for Updates...")
        .accelerator("CmdOrCtrl+Shift+U")
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit-app", "Quit Deepseek Harness")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "App")
        .item(&about)
        .separator()
        .item(&settings)
        .item(&plugins)
        .separator()
        .item(&check_updates)
        .separator()
        .item(&quit)
        .build()?;

    let show_web = MenuItemBuilder::with_id("show-web", "Open Web").build(app)?;
    let open_browser =
        MenuItemBuilder::with_id("open-browser", "Open Web in Browser").build(app)?;
    let open_cli = MenuItemBuilder::with_id("open-cli", "Open Profile Shell...").build(app)?;
    let restart_runtime =
        MenuItemBuilder::with_id("restart-runtime", "Restart Runtime").build(app)?;
    let stop_runtime = MenuItemBuilder::with_id("stop-runtime", "Stop Runtime").build(app)?;
    let copy_diagnostics =
        MenuItemBuilder::with_id("copy-diagnostics", "Copy Diagnostics").build(app)?;
    let diagnostics = MenuItemBuilder::with_id("diagnostics", "Diagnostics...").build(app)?;
    let runtime_menu = SubmenuBuilder::new(app, "Runtime")
        .item(&show_web)
        .item(&open_browser)
        .item(&open_cli)
        .separator()
        .item(&restart_runtime)
        .item(&stop_runtime)
        .separator()
        .item(&diagnostics)
        .item(&copy_diagnostics)
        .build()?;

    let undo = PredefinedMenuItem::undo(app, Some("Undo"))?;
    let redo = PredefinedMenuItem::redo(app, Some("Redo"))?;
    let cut = PredefinedMenuItem::cut(app, Some("Cut"))?;
    let copy = PredefinedMenuItem::copy(app, Some("Copy"))?;
    let paste = PredefinedMenuItem::paste(app, Some("Paste"))?;
    let select_all = PredefinedMenuItem::select_all(app, Some("Select All"))?;
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(&undo)
        .item(&redo)
        .separator()
        .item(&cut)
        .item(&copy)
        .item(&paste)
        .item(&select_all)
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&app_menu)
        .item(&runtime_menu)
        .item(&edit_menu)
        .build()?;
    app.set_menu(menu)?;

    app.on_menu_event(move |app_handle, event| {
        if let Err(err) = handle_action(app_handle, event.id().0.as_str()) {
            show_message(
                app_handle,
                "Deepseek Harness",
                err,
                MessageDialogKind::Error,
            );
        }
    });

    Ok(())
}

fn setup_tray(app: &mut App) -> Result<(), Box<dyn Error>> {
    let menu = build_tray_menu(app.handle())?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(include_bytes!(
            "../icons/tray-iconTemplate.png"
        ))?)
        .tooltip(APP_NAME)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            if let Err(err) = handle_action(app, event.id().0.as_str()) {
                show_message(app, "Deepseek Harness", err, MessageDialogKind::Error);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                let _ = show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn build_tray_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, Box<dyn Error>> {
    let runtime = app.state::<RuntimeManager>();
    let desktop = app.state::<DesktopState>();
    let current_profile = desktop.current_profile();
    let profiles_root = PathBuf::from(runtime.info().dsh_home).join("profiles");
    let profiles = discover_profiles(&profiles_root);

    let show_web = MenuItem::with_id(app, "show-web", "Open Deepseek Harness", true, None::<&str>)?;
    let open_browser = MenuItem::with_id(
        app,
        "open-browser",
        "Open Web in Browser",
        true,
        None::<&str>,
    )?;
    let open_cli = MenuItem::with_id(app, "open-cli", "Open Terminal", true, None::<&str>)?;
    let mut profile_menu =
        SubmenuBuilder::with_id(app, "profile-menu", format!("Profile: {current_profile}"));
    for profile in profiles {
        profile_menu = profile_menu.item(
            &CheckMenuItemBuilder::with_id(
                format!("profile:{}", profile.name),
                profile.name.clone(),
            )
            .checked(profile.name == current_profile)
            .build(app)?,
        );
    }
    let profile_menu = profile_menu.build()?;
    let plugins = MenuItem::with_id(app, "plugins", "Plugins...", true, None::<&str>)?;
    let restart_runtime = MenuItem::with_id(
        app,
        "restart-runtime",
        "Restart Runtime",
        true,
        None::<&str>,
    )?;
    let diagnostics = MenuItem::with_id(app, "diagnostics", "Diagnostics...", true, None::<&str>)?;
    let check_updates = MenuItem::with_id(
        app,
        "check-updates",
        "Check for Updates...",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit-app", "Quit", true, None::<&str>)?;
    Ok(Menu::with_items(
        app,
        &[
            &show_web,
            &open_browser,
            &open_cli,
            &profile_menu,
            &plugins,
            &restart_runtime,
            &diagnostics,
            &check_updates,
            &quit,
        ],
    )?)
}

fn refresh_tray_menu(app: &AppHandle) -> Result<(), Box<dyn Error>> {
    let menu = build_tray_menu(app)?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

fn handle_action(app: &AppHandle, action: &str) -> Result<(), String> {
    if let Some(profile) = action.strip_prefix("profile:") {
        app.state::<DesktopState>()
            .set_current_profile(profile.to_string())?;
        refresh_tray_menu(app).map_err(|err| err.to_string())?;
        return Ok(());
    }

    match action {
        "settings" => open_settings_window(app, Some("overview")).map_err(|err| err.to_string()),
        "plugins" => open_settings_window(app, Some("plugins")).map_err(|err| err.to_string()),
        "show-web" => show_main_window(app).map_err(|err| err.to_string()),
        "open-browser" => {
            let runtime = app.state::<RuntimeManager>();
            let url = runtime
                .info()
                .web_url
                .ok_or_else(|| "Web runtime URL is not ready".to_string())?;
            open_external(&url).map_err(|err| err.to_string())
        }
        "open-cli" => {
            let runtime = app.state::<RuntimeManager>();
            let desktop = app.state::<DesktopState>();
            let dsh_home = PathBuf::from(runtime.info().dsh_home);
            open_profile_terminal(&dsh_home, &desktop.current_profile())
                .map_err(|err| err.to_string())
        }
        "restart-runtime" => app
            .state::<RuntimeManager>()
            .restart()
            .map_err(|err| err.to_string()),
        "stop-runtime" => app
            .state::<RuntimeManager>()
            .stop()
            .map_err(|err| err.to_string()),
        "copy-diagnostics" => {
            let diagnostics = copy_diagnostics(app.state::<RuntimeManager>());
            show_message(app, "Diagnostics", diagnostics, MessageDialogKind::Info);
            Ok(())
        }
        "diagnostics" => {
            open_settings_window(app, Some("diagnostics")).map_err(|err| err.to_string())
        }
        "check-updates" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                trigger_update_check(handle).await;
            });
            Ok(())
        }
        "quit-app" => {
            app.exit(0);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn open_settings_window(app: &AppHandle, page: Option<&str>) -> Result<(), Box<dyn Error>> {
    let page = page.unwrap_or("overview");
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        window.show()?;
        window.set_focus()?;
        let page = serde_json::to_string(page)?;
        window.eval(&format!(
            "window.location.hash = {page}; window.dispatchEvent(new CustomEvent('settings-page', {{ detail: {page} }}));"
        ))?;
        return Ok(());
    }

    let url = Url::parse(&format!(
        "{SETTINGS_SCHEME}://localhost/settings.html#{page}"
    ))?;
    WebviewWindowBuilder::new(app, SETTINGS_WINDOW, WebviewUrl::CustomProtocol(url))
        .title("Deepseek Harness Settings")
        .inner_size(920.0, 660.0)
        .min_inner_size(780.0, 560.0)
        .resizable(true)
        .build()?;
    Ok(())
}

fn show_main_window(app: &AppHandle) -> Result<(), Box<dyn Error>> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

fn open_external(target: &str) -> Result<(), Box<dyn Error>> {
    if cfg!(target_os = "macos") {
        Command::new("open").arg(target).spawn()?;
    } else if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", "", target])
            .spawn()?;
    } else {
        Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}

fn open_profile_terminal(dsh_home: &Path, profile: &str) -> Result<(), Box<dyn Error>> {
    let profiles_root = dsh_home.join("profiles");
    fs::create_dir_all(&profiles_root)?;
    let profile_path = profiles_root.join(profile);
    let cwd = if profile_path.exists() {
        profile_path.clone()
    } else {
        profiles_root.clone()
    };
    let default_command = discover_profile_default_command(&profiles_root, profile);
    let command = profile_shell_command(&cwd, profile, &default_command);
    open_terminal(&command)
}

fn profile_shell_command(cwd: &Path, profile: &str, default_command: &str) -> String {
    if cfg!(target_os = "windows") {
        let cwd = cmd_quote(&cwd.display().to_string());
        let profile = profile.replace('"', "\"\"");
        let default_command = default_command.replace('"', "\"\"");
        return format!(
            "cd /d {cwd} && cls && echo DSH profile shell: {profile} && echo. && echo Detected default command: && echo   {default_command} && echo. && echo Profile commands: && echo   dsh plugin --profile {profile} list && echo   dsh plugin --profile {profile} add ^<package-or-path^> && echo   dsh plugin --profile {profile} remove ^<package^> && echo. && echo This shell does not boot the profile automatically."
        );
    }

    let cwd = shell_quote(&cwd.display().to_string());
    let profile_arg = shell_quote(profile);
    let command_arg = shell_quote(default_command);
    format!(
        "cd {cwd}; clear; printf 'DSH profile shell: %s\\n\\n' {profile_arg}; printf 'Detected default command:\\n  %s\\n\\n' {command_arg}; printf 'Profile commands:\\n'; printf '  dsh plugin --profile %s list\\n' {profile_arg}; printf '  dsh plugin --profile %s add <package-or-path>\\n' {profile_arg}; printf '  dsh plugin --profile %s remove <package>\\n\\n' {profile_arg}; printf 'This shell does not boot the profile automatically.\\n'"
    )
}

fn open_terminal(command: &str) -> Result<(), Box<dyn Error>> {
    if cfg!(target_os = "macos") {
        Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "tell application \"Terminal\" to do script {}",
                applescript_string(command)
            ))
            .spawn()?;
    } else if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", "cmd", "/K", command])
            .spawn()?;
    } else {
        Command::new("x-terminal-emulator")
            .args(["-e", "sh", "-lc", command])
            .spawn()?;
    }
    Ok(())
}

fn applescript_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn cmd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn upstream_manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../submodules/deepseek-harness/package.json")
}

fn upstream_version() -> String {
    let Ok(content) = fs::read_to_string(upstream_manifest_path()) else {
        return "unknown".to_string();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return "unknown".to_string();
    };
    value
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown")
        .to_string()
}

fn upstream_commit() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../submodules/deepseek-harness");
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--short", "HEAD"])
        .output();
    output
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn read_profile_summary(profiles_root: &Path, name: &str) -> ProfileSummary {
    let path = profiles_root.join(name);
    let manifest = read_profile_manifest(&path);
    let bundles = profile_bundles(&manifest);
    ProfileSummary {
        name: name.to_string(),
        path: path.display().to_string(),
        dependencies: manifest
            .get("dependencies")
            .and_then(|value| value.as_object())
            .map(|deps| deps.keys().cloned().collect())
            .unwrap_or_default(),
        bundles,
        default_command: discover_profile_default_command(profiles_root, name),
    }
}

fn discover_profiles(profiles_root: &Path) -> Vec<ProfileSummary> {
    let mut names = vec!["web".to_string(), "headless".to_string()];
    if let Ok(entries) = fs::read_dir(profiles_root) {
        let mut local_names = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if !path.is_dir() {
                    return None;
                }
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(ToOwned::to_owned)
            })
            .filter(|name| name != "web" && name != "headless" && name != "node_modules")
            .collect::<Vec<_>>();
        local_names.sort();
        names.extend(local_names);
    }
    names
        .into_iter()
        .map(|name| read_profile_summary(profiles_root, &name))
        .collect()
}

fn discover_profile_default_command(profiles_root: &Path, name: &str) -> String {
    let profile_path = profiles_root.join(name);
    let manifest = read_profile_manifest(&profile_path);
    if let Some(command) = manifest_default_command(&manifest) {
        return command;
    }

    let bundles = profile_bundles(&manifest);
    for bundle in &bundles {
        if let Some(command) = bundle_default_command(&profile_path, bundle) {
            return command;
        }
    }

    let has_web = name == "web"
        || bundles
            .iter()
            .any(|bundle| bundle == "@deepseek-ai/dsh-web-app");
    if has_web {
        return format!("dsh --profile {} --port 0", shell_quote(name));
    }

    let has_headless = name == "headless"
        || bundles
            .iter()
            .any(|bundle| bundle == "@deepseek-ai/dsh-headless");
    if has_headless {
        return format!("dsh --profile {} \"<task>\"", shell_quote(name));
    }

    format!("dsh --profile {}", shell_quote(name))
}

fn manifest_default_command(manifest: &serde_json::Value) -> Option<String> {
    let profile = manifest.get("dsh")?.get("profile")?;
    ["defaultCommand", "defaultCmd", "command", "cmd"]
        .iter()
        .find_map(|key| profile.get(key)?.as_str())
        .map(ToOwned::to_owned)
}

fn bundle_default_command(profile_path: &Path, package_name: &str) -> Option<String> {
    let manifest = read_package_manifest(&profile_path.join("node_modules"), package_name);
    manifest_default_command(&manifest)
        .or_else(|| {
            manifest
                .get("dsh")?
                .get("bundle")?
                .get("defaultCommand")?
                .as_str()
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            manifest
                .get("dsh")?
                .get("app")?
                .get("defaultCommand")?
                .as_str()
                .map(ToOwned::to_owned)
        })
}

fn read_package_manifest(node_modules: &Path, package_name: &str) -> serde_json::Value {
    let path = if let Some((scope, name)) = package_name
        .strip_prefix('@')
        .and_then(|value| value.split_once('/'))
    {
        node_modules.join(format!("@{scope}")).join(name)
    } else {
        node_modules.join(package_name)
    };
    read_profile_manifest(&path)
}

fn profile_bundles(manifest: &serde_json::Value) -> Vec<String> {
    manifest
        .get("dsh")
        .and_then(|value| value.get("profile"))
        .and_then(|value| value.get("bundles"))
        .and_then(|value| value.as_array())
        .map(|bundles| {
            bundles
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn plugin_state_from_profile(
    dsh_home: &str,
    profile: &str,
    package_name: &str,
) -> PluginInstallState {
    let profile_path = Path::new(dsh_home).join("profiles").join(profile);
    let manifest = read_profile_manifest(&profile_path);
    let dependencies = manifest
        .get("dependencies")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let bundles = manifest
        .get("dsh")
        .and_then(|value| value.get("profile"))
        .and_then(|value| value.get("bundles"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let installed = dependencies.contains_key(package_name);
    let active = bundles
        .iter()
        .any(|value| value.as_str() == Some(package_name));
    let state = if active {
        "active"
    } else if installed {
        "installed-inactive"
    } else {
        "not-installed"
    };
    PluginInstallState {
        profile: profile.to_string(),
        package_name: package_name.to_string(),
        installed,
        active,
        state: state.to_string(),
    }
}

fn read_profile_manifest(profile_path: &Path) -> serde_json::Value {
    let path = profile_path.join("package.json");
    let Ok(content) = fs::read_to_string(path) else {
        return serde_json::json!({});
    };
    serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
}

fn main() {
    let builder = tauri::Builder::default().register_uri_scheme_protocol(
        SETTINGS_SCHEME,
        |_context, request| match request.uri().path() {
            "/settings.html" | "/" | "" => Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(SETTINGS_HTML.as_bytes().to_vec())
                .unwrap(),
            _ => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(b"Not found".to_vec())
                .unwrap(),
        },
    );

    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    let builder = builder.plugin(tauri_plugin_dialog::init());

    builder
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            get_runtime_info,
            get_desktop_state,
            set_current_profile,
            open_settings,
            show_web,
            open_web_in_browser,
            restart_runtime,
            stop_runtime,
            copy_diagnostics,
            list_profiles,
            get_plugin_state,
            install_plugin,
            uninstall_plugin,
            open_cli_profile,
            open_profile_shell,
            open_profile_directory,
        ])
        .setup(setup_shell)
        .run(tauri::generate_context!())
        .expect("error while running Deepseek Harness")
}
