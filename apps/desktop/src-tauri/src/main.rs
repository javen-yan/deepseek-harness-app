#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod runtime;

use tauri::{
    image::Image,
    menu::{
        AboutMetadataBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder,
    },
    App, AppHandle, Manager,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

#[cfg(not(debug_assertions))]
use tauri_plugin_updater::UpdaterExt;

const APP_NAME: &str = "Deepseek Harness";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

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

fn setup_shell(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
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
    let check_updates = MenuItemBuilder::with_id("check-updates", "Check for Updates")
        .accelerator("CmdOrCtrl+Shift+U")
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit-app", "Quit Deepseek Harness")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "App")
        .item(&about)
        .separator()
        .item(&check_updates)
        .separator()
        .item(&quit)
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
        .item(&edit_menu)
        .build()?;
    app.set_menu(menu)?;

    app.on_menu_event(move |app_handle, event| match event.id().0.as_str() {
        "check-updates" => {
            let handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                trigger_update_check(handle).await;
            });
        }
        "quit-app" => app_handle.exit(0),
        _ => {}
    });

    if let Some(runtime) = runtime::bootstrap(app.handle().clone())? {
        app.manage(runtime);
    }

    Ok(())
}

fn main() {
    let builder = tauri::Builder::default();

    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    let builder = builder.plugin(tauri_plugin_dialog::init());

    builder
        .setup(setup_shell)
        .run(tauri::generate_context!())
        .expect("error while running Deepseek Harness")
}
