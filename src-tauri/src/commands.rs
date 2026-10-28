use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::capture::CaptureError;
use crate::notify::{self, Action};

/// Path of the most recent capture.
pub type CaptureState = Mutex<Option<PathBuf>>;

/// Set while a screencapture session is open, so a double-click or a held
/// hotkey can't stack a second one on top.
static CAPTURING: AtomicBool = AtomicBool::new(false);

/// Fire-and-forget capture for the tray and the global shortcut. Failures are
/// surfaced inside `start_capture`, so there's nothing to handle here.
pub fn trigger_capture(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let _ = start_capture(app).await;
    });
}

#[tauri::command]
pub async fn start_capture(app: AppHandle) -> Result<(), String> {
    if CAPTURING.swap(true, Ordering::SeqCst) {
        crate::logging::info("capture ignored: one is already open");
        return Ok(());
    }
    let result = run_capture(&app).await;
    CAPTURING.store(false, Ordering::SeqCst);
    result
}

async fn run_capture(app: &AppHandle) -> Result<(), String> {
    if !crate::permission::has_screen_recording() {
        crate::permission::request_screen_recording();
        return Err(permission_needed(app));
    }

    crate::logging::info("capture started");
    // Get Prole's own windows out of the shot; put them back afterwards.
    let floating = crate::windows::floating_visible(app);
    let home = app
        .get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false));
    if floating {
        let _ = crate::windows::hide_floating(app);
    }
    if home {
        let _ = app.get_webview_window("main").map(|w| w.hide());
    }
    crate::windows::hide_hud(app);

    let hid_something = floating || home;
    let result = tauri::async_runtime::spawn_blocking(move || {
        if hid_something {
            // Let the window server drop our windows before the crosshair appears.
            std::thread::sleep(Duration::from_millis(150));
        }
        crate::capture::capture_region()
    })
    .await
    .map_err(|e| e.to_string())?;

    if floating {
        let _ = crate::windows::show_floating(app);
    }
    let reopen_home = || {
        if home {
            let _ = crate::windows::open_home(app);
        }
    };

    match result {
        Ok(path) => {
            crate::logging::info(format!("capture saved: {}", path.display()));
            let previous = app
                .state::<CaptureState>()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .replace(path);
            if let Some(old) = previous {
                let _ = std::fs::remove_file(old);
            }
            crate::windows::open_editor(app).map_err(|e| {
                let msg = e.to_string();
                notify::error(app, "Couldn't open the editor", &msg, None);
                msg
            })
        }
        Err(CaptureError::Cancelled) => {
            crate::logging::info("capture cancelled");
            reopen_home();
            Ok(())
        }
        Err(CaptureError::PermissionDenied) => Err(permission_needed(app)),
        Err(e) => {
            let msg = e.to_string();
            notify::error(app, "Capture failed", &msg, None);
            reopen_home();
            Err(msg)
        }
    }
}

/// Missing Screen Recording: bring up the home window, which explains the fix
/// step by step. Falls back to a toast if the window can't open.
fn permission_needed(app: &AppHandle) -> String {
    let msg = "Prole needs Screen Recording permission to capture.".to_string();
    crate::logging::error(&msg);
    let shown = crate::windows::open_home(app)
        .and_then(|()| app.emit_to("main", "permission-needed", ()));
    if let Err(e) = shown {
        notify::error(
            app,
            "Screen Recording is off",
            format!("Allow Prole in System Settings → Privacy & Security → Screen & System Audio Recording. ({e})"),
            Some(Action::OpenScreenRecording),
        );
    }
    msg
}

#[tauri::command]
pub fn get_capture(app: AppHandle) -> Result<String, String> {
    let path = app
        .state::<CaptureState>()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or("There's no capture to edit yet.")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("Couldn't read the capture: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tauri::command]
pub fn copy_image(app: AppHandle, png_base64: String) -> Result<(), String> {
    let copied = base64::engine::general_purpose::STANDARD
        .decode(png_base64)
        .map_err(|e| e.to_string())
        .and_then(|bytes| crate::clipboard::set_clipboard_image(&bytes));
    match copied {
        Ok(()) => {
            notify::success(&app, "Copied to clipboard", "Paste it anywhere with ⌘V");
            Ok(())
        }
        Err(e) => {
            crate::logging::error(format!("copy failed: {e}"));
            Err(format!("Couldn't copy the image: {e}"))
        }
    }
}

/// Everything the home window shows.
#[derive(Serialize)]
pub struct AppState {
    settings: serde_json::Value,
    /// What triggers a capture, for display (⇧⌘4 when Prole has it).
    capture_shortcut: String,
    screen_recording: bool,
    hotkey_error: Option<String>,
    floating_visible: bool,
    version: &'static str,
    log_path: String,
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> AppState {
    AppState {
        settings: crate::settings::read_all(&app),
        capture_shortcut: crate::settings::capture_shortcut(&app),
        screen_recording: crate::permission::has_screen_recording(),
        hotkey_error: app
            .state::<crate::apply::HotkeyError>()
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone(),
        floating_visible: crate::windows::floating_visible(&app),
        version: env!("CARGO_PKG_VERSION"),
        log_path: crate::logging::path().display().to_string(),
    }
}

#[tauri::command]
pub fn set_setting(app: AppHandle, key: String, value: serde_json::Value) -> Result<(), String> {
    crate::apply::change_setting(&app, &key, value).inspect_err(|e| {
        crate::logging::error(format!("setting {key}: {e}"));
    })
}

#[tauri::command]
pub fn set_hotkey_suspended(app: AppHandle, suspended: bool) -> Result<(), String> {
    crate::apply::suspend_hotkey(&app, suspended)
}

/// A page hit an error it can't show itself (or an uncaught exception).
#[tauri::command]
pub fn report_error(app: AppHandle, title: String, detail: String) {
    notify::error(&app, &title, detail, None);
}

/// A page is showing an error inline; just keep a record of it.
#[tauri::command]
pub fn log_error(message: String) {
    crate::logging::error(message);
}

#[tauri::command]
pub fn take_hud_toast(app: AppHandle) -> Option<notify::Toast> {
    app.state::<notify::Pending>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

#[tauri::command]
pub fn hide_hud(app: AppHandle) {
    crate::windows::hide_hud(&app);
}

#[tauri::command]
pub fn open_home(app: AppHandle) -> Result<(), String> {
    crate::windows::open_home(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_log() -> Result<(), String> {
    let path = crate::logging::path();
    if !path.exists() {
        crate::logging::info("log opened");
    }
    let status = std::process::Command::new("open")
        .args(["-a", "Console"])
        .arg(path)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Couldn't open {}", path.display()))
    }
}

#[tauri::command]
pub fn open_screen_recording_settings() -> Result<(), String> {
    crate::permission::request_screen_recording();
    crate::permission::open_screen_recording_settings()
        .map_err(|e| format!("Couldn't open System Settings: {e}"))
}

/// "Switched on but still refused": drop the stale entry, ask again (macOS
/// shows its prompt and re-lists Prole), and open the pane to switch it on.
#[tauri::command]
pub fn reset_screen_recording(app: AppHandle) -> Result<(), String> {
    let id = app.config().identifier.clone();
    crate::permission::reset_screen_recording(&id)
        .map_err(|e| format!("Couldn't reset the Screen Recording entry: {e}"))?;
    crate::logging::info(format!("reset Screen Recording entry for {id}"));
    crate::permission::request_screen_recording();
    crate::permission::open_screen_recording_settings()
        .map_err(|e| format!("Couldn't open System Settings: {e}"))
}

#[tauri::command]
pub fn restart(app: AppHandle) {
    crate::logging::info("restarting at the user's request");
    // restart() skips RunEvent::Exit; launch takes ⇧⌘4 again anyway.
    crate::apply::release_macos_shortcut_if_enabled(&app);
    app.restart();
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}
