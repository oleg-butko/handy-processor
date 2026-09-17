use handy_processor_launcher::{runtime::RuntimePaths, template_settings::TemplateHotkeySettings};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{App, AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

#[cfg(windows)]
use handy_processor_launcher::windows_templates::{
    restore_and_paste, FocusTarget, HookEvent, HotkeyService,
};
#[cfg(windows)]
use tauri::Emitter;

struct Data {
    settings: TemplateHotkeySettings,
    error: Option<String>,
    activation: Option<u64>,
    #[cfg(windows)]
    next_activation: u64,
    #[cfg(windows)]
    target: Option<FocusTarget>,
}

pub struct TemplateDesktop {
    data: Mutex<Data>,
    #[cfg(windows)]
    directory: PathBuf,
    #[cfg(windows)]
    service: Option<HotkeyService>,
    #[cfg(windows)]
    settings_operation: Mutex<()>,
    pasting: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSnapshot {
    supported: bool,
    settings: TemplateHotkeySettings,
    error: Option<String>,
    can_paste: bool,
    activation: Option<u64>,
}

impl TemplateDesktop {
    fn snapshot(&self) -> DesktopSnapshot {
        let data = self.data.lock().unwrap();
        #[cfg(windows)]
        let can_paste = data.target.as_ref().is_some_and(FocusTarget::is_valid);
        #[cfg(not(windows))]
        let can_paste = false;
        DesktopSnapshot {
            supported: cfg!(windows),
            settings: data.settings.clone(),
            error: data.error.clone(),
            can_paste,
            activation: data.activation,
        }
    }

    pub fn clear_target(&self) {
        #[cfg(windows)]
        {
            self.data.lock().unwrap().target = None;
        }
    }

    pub fn stop(&self) {
        #[cfg(windows)]
        if let Some(service) = &self.service {
            service.stop();
        }
    }
}

pub fn initialize(app: &mut App, directory: PathBuf) {
    let (settings, mut error) = match TemplateHotkeySettings::load(&directory) {
        Ok(settings) => (settings, None),
        Err(error) => (
            TemplateHotkeySettings {
                enabled: false,
                ..Default::default()
            },
            Some(format!("{error:#}")),
        ),
    };
    #[cfg(windows)]
    let (events, receiver) = std::sync::mpsc::channel();
    #[cfg(windows)]
    let service = match HotkeyService::start(settings.clone(), events) {
        Ok(service) => Some(service),
        Err(reason) => {
            error = Some(reason);
            None
        }
    };
    #[cfg(not(windows))]
    {
        let _ = &mut error;
    }

    let state = Arc::new(TemplateDesktop {
        data: Mutex::new(Data {
            settings,
            error,
            activation: None,
            #[cfg(windows)]
            next_activation: 0,
            #[cfg(windows)]
            target: None,
        }),
        #[cfg(windows)]
        directory,
        #[cfg(windows)]
        service,
        #[cfg(windows)]
        settings_operation: Mutex::new(()),
        pasting: AtomicBool::new(false),
    });
    app.manage(state.clone());
    #[cfg(windows)]
    {
        let app = app.handle().clone();
        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                match event {
                    HookEvent::Error(error) => state.data.lock().unwrap().error = Some(error),
                    HookEvent::Activate(target) => {
                        if state.pasting.load(Ordering::Acquire) {
                            continue;
                        }
                        let id = {
                            let mut data = state.data.lock().unwrap();
                            if target.is_some() {
                                data.target = target;
                            }
                            data.next_activation += 1;
                            let id = data.next_activation;
                            data.activation = Some(id);
                            id
                        };
                        if let Some(window) = app.get_webview_window(super::MAIN_WINDOW) {
                            if let Err(error) = window
                                .unminimize()
                                .and_then(|()| window.show())
                                .and_then(|()| window.set_focus())
                            {
                                state.data.lock().unwrap().error =
                                    Some(format!("Не удалось показать окно: {error}"));
                            }
                            let _ = window.emit("templates-activated", id);
                        }
                    }
                }
            }
        });
    }
}

#[tauri::command]
pub fn templates_desktop_state(
    window: WebviewWindow,
    paths: State<'_, RuntimePaths>,
    state: State<'_, Arc<TemplateDesktop>>,
) -> Result<DesktopSnapshot, String> {
    super::validate_window(&window, &paths)?;
    Ok(state.snapshot())
}

#[tauri::command]
pub fn ack_templates_activation(
    window: WebviewWindow,
    paths: State<'_, RuntimePaths>,
    state: State<'_, Arc<TemplateDesktop>>,
    id: u64,
) -> Result<(), String> {
    super::validate_window(&window, &paths)?;
    let mut data = state.data.lock().unwrap();
    if data.activation == Some(id) {
        data.activation = None;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_template_hotkey(
    window: WebviewWindow,
    paths: State<'_, RuntimePaths>,
    state: State<'_, Arc<TemplateDesktop>>,
    settings: TemplateHotkeySettings,
) -> Result<DesktopSnapshot, String> {
    super::validate_window(&window, &paths)?;
    settings.validate().map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    {
        let _ = state;
        Err("Глобальный хоткей доступен только в Windows".into())
    }
    #[cfg(windows)]
    {
        let state = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _operation = state.settings_operation.lock().unwrap();
            let service = state
                .service
                .as_ref()
                .ok_or("Перехватчик клавиатуры недоступен; перезапустите лаунчер")?;
            let previous = state.data.lock().unwrap().settings.clone();
            service.update(settings.clone())?;
            if let Err(error) = settings.save(&state.directory) {
                let rollback = service.update(previous);
                if rollback.is_err() {
                    state.data.lock().unwrap().error =
                        Some("Не удалось восстановить хоткей; перезапустите лаунчер".into());
                }
                return Err(format!("{error:#}"));
            }
            {
                let mut data = state.data.lock().unwrap();
                data.settings = settings;
                data.error = None;
            }
            Ok(state.snapshot())
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[tauri::command]
pub async fn record_template_key(
    window: WebviewWindow,
    paths: State<'_, RuntimePaths>,
    state: State<'_, Arc<TemplateDesktop>>,
) -> Result<String, String> {
    super::validate_window(&window, &paths)?;
    #[cfg(not(windows))]
    {
        let _ = state;
        Err("Запись хоткея доступна только в Windows".into())
    }
    #[cfg(windows)]
    {
        let owner = window.hwnd().map_err(|e| e.to_string())?.0 as usize;
        let state = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            state
                .service
                .as_ref()
                .ok_or("Перехватчик клавиатуры недоступен")?
                .record(owner)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[tauri::command]
pub fn cancel_template_recording(
    window: WebviewWindow,
    paths: State<'_, RuntimePaths>,
    state: State<'_, Arc<TemplateDesktop>>,
) -> Result<(), String> {
    super::validate_window(&window, &paths)?;
    #[cfg(windows)]
    if let Some(service) = &state.service {
        service.cancel_recording();
    }
    #[cfg(not(windows))]
    let _ = state;
    Ok(())
}

#[tauri::command]
pub async fn choose_templates_file(
    app: AppHandle,
    window: WebviewWindow,
    paths: State<'_, RuntimePaths>,
) -> Result<Option<String>, String> {
    super::validate_window(&window, &paths)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Файл шаблонов UTF-8")
            .add_filter("Текст", &["txt"])
            .blocking_pick_file()
            .map(|path| {
                path.into_path()
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}

fn validate_text(text: &str) -> Result<(), String> {
    if text.is_empty() || text.len() > 2 * 1024 * 1024 || text.contains('\0') {
        return Err("Некорректный текст шаблона".into());
    }
    Ok(())
}

#[tauri::command]
pub fn copy_template(
    app: AppHandle,
    window: WebviewWindow,
    paths: State<'_, RuntimePaths>,
    text: String,
) -> Result<(), String> {
    super::validate_window(&window, &paths)?;
    validate_text(&text)?;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct PasteResult {
    pasted: bool,
    message: String,
}

struct PasteGuard(Arc<TemplateDesktop>);
impl Drop for PasteGuard {
    fn drop(&mut self) {
        self.0.pasting.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub async fn paste_template(
    app: AppHandle,
    window: WebviewWindow,
    paths: State<'_, RuntimePaths>,
    state: State<'_, Arc<TemplateDesktop>>,
    text: String,
) -> Result<PasteResult, String> {
    super::validate_window(&window, &paths)?;
    validate_text(&text)?;
    if state
        .pasting
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Вставка уже выполняется".into());
    }
    let guard = PasteGuard(state.inner().clone());
    app.clipboard()
        .write_text(text.clone())
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    {
        let _ = guard;
        Ok(PasteResult {
            pasted: false,
            message: "Скопировано. Автоматическая вставка доступна в Windows".into(),
        })
    }
    #[cfg(windows)]
    {
        let target = state.data.lock().unwrap().target.clone();
        let Some(target) = target.filter(FocusTarget::is_valid) else {
            return Ok(PasteResult {
                pasted: false,
                message:
                    "Скопировано; вызовите Шаблоны хоткеем из нужного поля или вставьте вручную"
                        .into(),
            });
        };
        window.hide().map_err(|e| e.to_string())?;
        let result = tauri::async_runtime::spawn_blocking(move || {
            let _guard = guard;
            restore_and_paste(&target, || {
                if app.clipboard().read_text().map_err(|e| e.to_string())? != text {
                    return Err(
                        "Содержимое буфера изменилось; автоматическая вставка отменена".into(),
                    );
                }
                Ok(())
            })
        })
        .await
        .map_err(|e| e.to_string())?;
        match result {
            Ok(()) => Ok(PasteResult {
                pasted: true,
                message: "Команда вставки отправлена в исходное окно".into(),
            }),
            Err(error) => {
                let _ = window.show();
                let _ = window.set_focus();
                Ok(PasteResult {
                    pasted: false,
                    message: format!("Скопировано; вставьте вручную. {error}"),
                })
            }
        }
    }
}
