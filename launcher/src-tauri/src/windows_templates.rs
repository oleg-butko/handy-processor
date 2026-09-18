//! Windows-only keyboard observation and clipboard paste injection.
//! The hook owns its state on one dedicated thread; it never waits on the UI.
use std::{
    cell::RefCell,
    mem::size_of,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

use crate::{
    double_tap::DoubleTap,
    paste_shortcut::send_paste,
    template_settings::{key_name, TemplateHotkeySettings},
};

const INJECTED_TAG: usize = 0x48505450;
const COMMAND_MESSAGE: u32 = WM_APP + 17;

#[derive(Clone, Debug)]
pub struct FocusTarget {
    hwnd: usize,
    focus: usize,
    pid: u32,
    captured: Instant,
}

impl FocusTarget {
    pub fn is_valid(&self) -> bool {
        let hwnd = HWND(self.hwnd as *mut _);
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            IsWindow(Some(hwnd)).as_bool()
                && pid == self.pid
                && self.captured.elapsed() < Duration::from_secs(600)
        }
    }
}

pub enum HookEvent {
    Activate(Option<FocusTarget>),
    Error(String),
}

enum Command {
    Settings(TemplateHotkeySettings, Sender<()>),
    Record(usize, Sender<Result<String, String>>),
    Cancel,
    Stop,
}

pub struct HotkeyService {
    sender: Sender<Command>,
    thread_id: u32,
}

impl HotkeyService {
    pub fn start(
        settings: TemplateHotkeySettings,
        events: Sender<HookEvent>,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        thread::spawn(move || unsafe {
            run(settings, receiver, events, ready);
        });
        let thread_id = started
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())??;
        Ok(Self { sender, thread_id })
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.sender
            .send(command)
            .map_err(|_| "Поток хоткея остановлен".to_string())?;
        unsafe { PostThreadMessageW(self.thread_id, COMMAND_MESSAGE, WPARAM(0), LPARAM(0)) }
            .map_err(|e| e.to_string())
    }

    pub fn update(&self, settings: TemplateHotkeySettings) -> Result<(), String> {
        let (sender, receiver) = mpsc::channel();
        self.send(Command::Settings(settings, sender))?;
        receiver
            .recv_timeout(Duration::from_secs(2))
            .map_err(|e| e.to_string())
    }

    pub fn record(&self, window: usize) -> Result<String, String> {
        let (sender, receiver) = mpsc::channel();
        self.send(Command::Record(window, sender))?;
        receiver
            .recv_timeout(Duration::from_secs(12))
            .map_err(|e| e.to_string())?
    }

    pub fn cancel_recording(&self) {
        let _ = self.send(Command::Cancel);
    }
    pub fn stop(&self) {
        let _ = self.send(Command::Stop);
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Recording {
    window: usize,
    result: Sender<Result<String, String>>,
    started: Instant,
    key: Option<u16>,
}
struct HookContext {
    settings: TemplateHotkeySettings,
    events: Sender<HookEvent>,
    detector: DoubleTap,
    held: [bool; 256],
    chord: [bool; 256],
    swallowed: [bool; 256],
    mask_release: [bool; 256],
    recording: Option<Recording>,
}

thread_local! { static CONTEXT: RefCell<Option<HookContext>> = const { RefCell::new(None) }; }

fn keyboard_input(key: u16, release: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(key),
                wScan: 0,
                dwFlags: (if release {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                }) | if matches!(
                    key,
                    0x5b | 0x5c | 0xa3 | 0xa5 | 0x21..=0x28 | 0x2d | 0x2e
                ) {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: INJECTED_TAG,
            },
        },
    }
}

fn inject(inputs: &[INPUT]) -> Result<(), String> {
    let count = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    if count as usize != inputs.len() {
        return Err("Windows отклонила ввод. Возможно, целевое приложение запущено с правами администратора".into());
    }
    Ok(())
}

/// Mask the menu action, then release the modifier in the SAME SendInput batch.
/// E8 is an unassigned VK, used only to mark a Win/Alt press as a combination.
/// The original physical key-up is swallowed only when the batch was delivered.
fn mask_menu_release(key: u16) -> bool {
    inject(&[
        keyboard_input(0xe8, false),
        keyboard_input(0xe8, true),
        keyboard_input(key, true),
    ])
    .is_ok()
}

fn modifier(key: u16) -> bool {
    matches!(key, 0x5b | 0x5c | 0xa0..=0xa5)
}
fn menu_modifier(key: u16) -> bool {
    matches!(key, 0x5b | 0x5c | 0xa4 | 0xa5)
}

fn physical_key(event: &KBDLLHOOKSTRUCT) -> u16 {
    match event.vkCode {
        0x10 => {
            if event.scanCode == 0x36 {
                0xa1
            } else {
                0xa0
            }
        }
        0x11 => {
            if event.flags.contains(LLKHF_EXTENDED) {
                0xa3
            } else {
                0xa2
            }
        }
        0x12 => {
            if event.flags.contains(LLKHF_EXTENDED) {
                0xa5
            } else {
                0xa4
            }
        }
        code => code as u16,
    }
}

impl HookContext {
    fn check_recording_window(&mut self, window: usize) {
        if self
            .recording
            .as_ref()
            .is_some_and(|rec| rec.window != window)
        {
            self.cancel_recording("Запись отменена: окно потеряло фокус");
        }
    }

    fn cancel_recording(&mut self, message: &str) {
        if let Some(recording) = self.recording.take() {
            let _ = recording.result.send(Err(message.into()));
        }
        self.detector.reset();
    }

    fn key(&mut self, event: &KBDLLHOOKSTRUCT, down: bool) -> bool {
        let key = physical_key(event);
        let index = key as usize;
        if index >= self.held.len() {
            return false;
        }
        let repeat = down && self.held[index];
        let other_held = self
            .held
            .iter()
            .enumerate()
            .any(|(i, held)| i != index && *held);
        self.held[index] = down;
        let window = unsafe { GetForegroundWindow() }.0 as usize;
        // UI cancellation is asynchronous, so guard the first foreign key too.
        self.check_recording_window(window);
        let recording = self.recording.is_some();
        let selected = self.settings.enabled && self.settings.matches(key);

        if down && !repeat {
            // A combination must never become a double tap on its last key-up.
            for (i, held) in self.held.iter().enumerate() {
                if *held && i != index {
                    self.chord[i] = true;
                }
            }
            self.chord[index] = other_held;
            if recording {
                self.detector.reset();
                if other_held {
                    self.cancel_recording("Выберите одну клавишу без сочетания");
                } else if let Some(rec) = self.recording.as_mut() {
                    rec.key = Some(key);
                }
            } else if selected && !other_held {
                self.detector
                    .press(key, event.time, window, self.settings.interval_ms);
            } else {
                self.detector.reset();
            }

            // Modifiers pass through so Win+E etc. retain native ordering.
            // Only the menu action of a bare Win/Alt release is masked.
            self.swallowed[index] = (recording || selected) && !modifier(key) && !other_held;
            self.mask_release[index] = (recording || selected) && menu_modifier(key);
        }

        if !down {
            let consume = std::mem::take(&mut self.swallowed[index]);
            let mask = std::mem::take(&mut self.mask_release[index]) && !self.chord[index];
            let masked = mask && mask_menu_release(key);
            if mask && !masked {
                let _ = self.events.send(HookEvent::Error(
                    "Не удалось подавить действие одиночной клавиши Win/Alt".into(),
                ));
            }
            if let Some(rec) = self.recording.as_ref() {
                if rec.key == Some(key) {
                    let rec = self.recording.take().unwrap();
                    let result =
                        key_name(key).ok_or_else(|| "Эта клавиша не поддерживается".to_string());
                    let _ = rec.result.send(result);
                }
            } else if selected && !self.chord[index] && !other_held {
                if self
                    .detector
                    .release(key, event.time, window, self.settings.interval_ms)
                {
                    let _ = self.events.send(HookEvent::Activate(capture_target()));
                }
            } else {
                self.detector.reset();
            }
            self.chord[index] = false;
            return consume || masked;
        }
        self.swallowed[index]
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = &*(data.0 as *const KBDLLHOOKSTRUCT);
        if !event.flags.contains(LLKHF_INJECTED) && event.dwExtraInfo != INJECTED_TAG {
            let message = message.0 as u32;
            if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP) {
                let down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
                let consume = CONTEXT.with(|context| {
                    context
                        .borrow_mut()
                        .as_mut()
                        .is_some_and(|state| state.key(event, down))
                });
                if consume {
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, message, data)
}

unsafe fn run(
    settings: TemplateHotkeySettings,
    commands: Receiver<Command>,
    events: Sender<HookEvent>,
    ready: Sender<Result<u32, String>>,
) {
    let module = match GetModuleHandleW(None) {
        Ok(module) => module,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };
    let hook = match SetWindowsHookExW(
        WH_KEYBOARD_LL,
        Some(keyboard_hook),
        Some(HINSTANCE(module.0)),
        0,
    ) {
        Ok(hook) => hook,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };
    let timer = SetTimer(None, 0, 100, None);
    if timer == 0 {
        let _ = UnhookWindowsHookEx(hook);
        let _ = ready.send(Err("Не удалось создать таймер хоткея".into()));
        return;
    }
    CONTEXT.with(|context| {
        *context.borrow_mut() = Some(HookContext {
            settings,
            events: events.clone(),
            detector: DoubleTap::default(),
            held: std::array::from_fn(|key| GetAsyncKeyState(key as i32) < 0),
            chord: [false; 256],
            swallowed: [false; 256],
            mask_release: [false; 256],
            recording: None,
        })
    });
    let _ = ready.send(Ok(GetCurrentThreadId()));
    let mut message = MSG::default();
    'messages: loop {
        let result = GetMessageW(&mut message, None, 0, 0).0;
        if result <= 0 {
            break;
        }
        while let Ok(command) = commands.try_recv() {
            if matches!(command, Command::Stop) {
                break 'messages;
            }
            CONTEXT.with(|context| {
                let mut context = context.borrow_mut();
                let state = context.as_mut().unwrap();
                match command {
                    Command::Settings(settings, reply) => {
                        state.cancel_recording("Настройки изменены");
                        state.settings = settings;
                        let _ = reply.send(());
                    }
                    Command::Record(window, result) => {
                        state.cancel_recording("Начата новая запись");
                        if window != 0 && GetForegroundWindow().0 as usize == window {
                            state.recording = Some(Recording {
                                window,
                                result,
                                started: Instant::now(),
                                key: None,
                            });
                        } else {
                            let _ = result.send(Err("Запись отменена: окно потеряло фокус".into()));
                        }
                    }
                    Command::Cancel => state.cancel_recording("Запись отменена"),
                    Command::Stop => {}
                }
            });
        }
        CONTEXT.with(|context| {
            let mut context = context.borrow_mut();
            let state = context.as_mut().unwrap();
            state.check_recording_window(GetForegroundWindow().0 as usize);
            if state
                .recording
                .as_ref()
                .is_some_and(|rec| rec.started.elapsed() > Duration::from_secs(10))
            {
                state.cancel_recording("Время записи истекло");
            }
            // Release events can be missed while the secure desktop is active.
            for key in 0..256 {
                // Swallowed key-downs never enter Windows' async state.
                if state.held[key] && !state.swallowed[key] && GetAsyncKeyState(key as i32) >= 0 {
                    state.held[key] = false;
                    state.chord[key] = false;
                    state.swallowed[key] = false;
                    state.mask_release[key] = false;
                    state.detector.reset();
                }
            }
        });
        if message.message != COMMAND_MESSAGE {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    CONTEXT.with(|context| {
        if let Some(mut state) = context.borrow_mut().take() {
            state.cancel_recording("Хоткей остановлен");
        }
    });
    let _ = KillTimer(None, timer);
    let _ = UnhookWindowsHookEx(hook);
}

fn capture_target() -> Option<FocusTarget> {
    let hwnd = unsafe { GetForegroundWindow() };
    let mut pid = 0;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if hwnd.0.is_null() || pid == 0 || pid == std::process::id() {
        return None;
    }
    let mut info = GUITHREADINFO {
        cbSize: size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let focus = if unsafe { GetGUIThreadInfo(thread, &mut info) }.is_ok() {
        info.hwndFocus.0 as usize
    } else {
        0
    };
    Some(FocusTarget {
        hwnd: hwnd.0 as usize,
        focus,
        pid,
        captured: Instant::now(),
    })
}

pub fn restore_and_paste(
    target: &FocusTarget,
    verify_clipboard: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if !target.is_valid() {
        return Err("Исходное окно закрыто или цель вставки устарела".into());
    }
    let hwnd = HWND(target.hwnd as *mut _);
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    let deadline = Instant::now() + Duration::from_millis(1000);
    loop {
        if !target.is_valid() {
            return Err("Исходное окно закрыто".into());
        }
        let foreground = unsafe { GetForegroundWindow() } == hwnd;
        let released = [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c]
            .iter()
            .all(|key| unsafe { GetAsyncKeyState(*key) >= 0 });
        if foreground && released {
            break;
        }
        if Instant::now() >= deadline {
            return Err("Не удалось вернуть фокус или дождаться отпускания модификаторов".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(80));
    if !target.is_valid() || unsafe { GetForegroundWindow() } != hwnd {
        return Err("Фокус изменился перед вставкой".into());
    }
    if [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c]
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(*key) < 0 })
    {
        return Err("Перед вставкой была нажата клавиша-модификатор".into());
    }
    if target.focus != 0 {
        let mut info = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        unsafe { GetGUIThreadInfo(0, &mut info) }.map_err(|e| e.to_string())?;
        if info.hwndFocus.0 as usize != target.focus {
            return Err("Фокус внутри исходного окна изменился".into());
        }
    }
    verify_clipboard()?;
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err("Фокус изменился перед вставкой".into());
    }
    send_paste(|keys| {
        let inputs: Vec<_> = keys
            .iter()
            .map(|&(key, release)| keyboard_input(key, release))
            .collect();
        unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) as usize }
    })
}
