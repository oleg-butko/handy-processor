#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::thread;
mod templates_desktop;
use templates_desktop::*;

use handy_processor_launcher::{
    history_export::download_history_export, runtime::RuntimePaths, supervisor::Supervisor,
};
use tauri::{
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager, RunEvent, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::StateFlags;

const MAIN_WINDOW: &str = "main";
const MENU_SHOW: &str = "show";
const MENU_BROWSER: &str = "browser";
const MENU_COPY_API: &str = "copy-api";
const MENU_DATA: &str = "data";
const MENU_RESTART: &str = "restart";
const MENU_EXIT: &str = "exit";
const DISPLAY_VERSION: &str = env!("HANDY_PROCESSOR_DISPLAY_VERSION");

fn show_action_error(app: &AppHandle, action: &str, error: impl std::fmt::Display) {
    app.dialog()
        .message(format!("{action}.\n\n{error}"))
        .title("Handy Processor")
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

fn show_main_window(app: &AppHandle) {
    if let Some(state) = app.try_state::<std::sync::Arc<TemplateDesktop>>() {
        state.clear_target();
    }
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };

    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };

    let visible = window.is_visible().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    if visible && !minimized {
        let _ = window.hide();
    } else {
        show_main_window(app);
    }
}

fn configure_window(window: &WebviewWindow) {
    let managed_window = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = managed_window.hide();
        }
        WindowEvent::Resized(_) if managed_window.is_minimized().unwrap_or(false) => {
            let _ = managed_window.unminimize();
            let _ = managed_window.hide();
        }
        _ => {}
    });
}

#[tauri::command]
async fn save_history_export(
    window: WebviewWindow,
    paths: State<'_, RuntimePaths>,
) -> Result<String, String> {
    validate_window(&window, &paths)?;

    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        download_history_export(&paths.directory, paths.port)
    })
    .await
    .map_err(|error| format!("history export task failed: {error}"))?
    .map(|path| path.to_string_lossy().into_owned())
    .map_err(|error| format!("{error:#}"))
}

fn validate_window(window: &WebviewWindow, paths: &RuntimePaths) -> Result<(), String> {
    if window.label() != MAIN_WINDOW {
        return Err("command is available only from the main window".into());
    }
    let actual_url = window.url().map_err(|error| error.to_string())?;
    let expected_url = tauri::Url::parse(&paths.web_url()).map_err(|error| error.to_string())?;
    if actual_url.origin() != expected_url.origin() {
        return Err("command came from an unexpected origin".into());
    }

    Ok(())
}

fn create_tray(app: &App, paths: RuntimePaths, supervisor: Supervisor) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text(MENU_SHOW, "Открыть Handy Processor")
        .text(MENU_BROWSER, "Открыть в браузере")
        .text(MENU_COPY_API, "Скопировать API URL")
        .text(MENU_DATA, "Открыть папку данных")
        .separator()
        .text(MENU_RESTART, "Перезапустить сервер")
        .separator()
        .text(MENU_EXIT, "Выход")
        .build()?;

    let menu_paths = paths.clone();
    let menu_supervisor = supervisor.clone();
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("launcher icon".into()))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Handy Processor")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => toggle_main_window(tray.app_handle()),
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main_window(tray.app_handle()),
            _ => {}
        })
        .on_menu_event(move |app, event| match event.id().as_ref() {
            MENU_SHOW => show_main_window(app),
            MENU_BROWSER => {
                if let Err(error) = app.opener().open_url(menu_paths.web_url(), None::<&str>) {
                    show_action_error(app, "Не удалось открыть браузер", error);
                }
            }
            MENU_COPY_API => {
                if let Err(error) = app.clipboard().write_text(menu_paths.api_url()) {
                    show_action_error(app, "Не удалось скопировать API URL", error);
                }
            }
            MENU_DATA => {
                if let Err(error) = app.opener().open_path(
                    menu_paths.directory.to_string_lossy().into_owned(),
                    None::<&str>,
                ) {
                    show_action_error(app, "Не удалось открыть папку данных", error);
                }
            }
            MENU_RESTART => {
                let restart_supervisor = menu_supervisor.clone();
                let restart_app = app.clone();
                thread::spawn(move || match restart_supervisor.restart() {
                    Ok(()) => {
                        if let Some(window) = restart_app.get_webview_window(MAIN_WINDOW) {
                            let _ = window.eval("window.location.reload()");
                        }
                    }
                    Err(error) => show_action_error(
                        &restart_app,
                        "Не удалось перезапустить сервер",
                        format!("{error:#}"),
                    ),
                });
            }
            MENU_EXIT => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn initialize_app(app: &mut App) -> anyhow::Result<()> {
    let paths = RuntimePaths::discover()?;
    let supervisor = Supervisor::initialize(paths.clone())?;
    let desktop_result = (|| -> anyhow::Result<()> {
        let web_url = tauri::Url::parse(&paths.web_url())?;
        let window = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::External(web_url))
            .title(format!("Handy Processor v{DISPLAY_VERSION}"))
            .inner_size(1100.0, 760.0)
            .min_inner_size(720.0, 480.0)
            .visible(false)
            .center()
            .build()?;

        configure_window(&window);
        create_tray(app, paths.clone(), supervisor.clone())?;
        Ok(())
    })();
    if let Err(error) = desktop_result {
        supervisor.shutdown();
        return Err(error);
    }

    let directory = paths.directory.clone();
    app.manage(paths);
    app.manage(supervisor);
    templates_desktop::initialize(app, directory);
    Ok(())
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            |app, _arguments, _cwd| {
                show_main_window(app);
            },
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::POSITION | StateFlags::SIZE)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            save_history_export,
            templates_desktop_state,
            ack_templates_activation,
            set_template_hotkey,
            record_template_key,
            cancel_template_recording,
            choose_templates_file,
            copy_template,
            paste_template
        ])
        .setup(|app| {
            if let Err(error) = initialize_app(app) {
                let app_handle = app.handle().clone();
                app.dialog()
                    .message(format!(
                        "Не удалось запустить Handy Processor.\n\n{error:#}"
                    ))
                    .title("Ошибка запуска Handy Processor")
                    .kind(MessageDialogKind::Error)
                    .show(move |_| app_handle.exit(1));
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to initialize Handy Processor launcher");

    app.run(|app, event| {
        if matches!(event, RunEvent::ExitRequested { .. }) {
            if let Some(state) = app.try_state::<std::sync::Arc<TemplateDesktop>>() {
                state.stop();
            }
            if let Some(supervisor) = app.try_state::<Supervisor>() {
                supervisor.shutdown();
            }
        }
    });
}
