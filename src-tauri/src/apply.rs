use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Why the capture shortcut isn't active, if it isn't. Shown in the home window.
#[derive(Default)]
pub struct HotkeyError(pub Mutex<Option<String>>);

/// Saves a setting and applies it. On failure the old value is restored, so
/// the stored setting never claims something that isn't in effect.
pub fn change_setting(app: &AppHandle, key: &str, value: serde_json::Value) -> Result<(), String> {
    match key {
        "floating_enabled" => {
            crate::windows::set_floating(app, value.as_bool().ok_or("expected true/false")?)
        }
        "launch_at_login" => {
            let on = value.as_bool().ok_or("expected true/false")?;
            let mgr = app.autolaunch();
            let applied = if on { mgr.enable() } else { mgr.disable() };
            applied.map_err(|e| format!("Couldn't change Launch at Login: {e}"))?;
            crate::settings::set(app, key, value)
        }
        "hotkey" => {
            let previous = crate::settings::hotkey(app);
            crate::settings::set(app, key, value)?;
            if let Err(e) = register_hotkey(app) {
                crate::settings::set(app, key, previous.into())?;
                let _ = register_hotkey(app);
                return Err(e);
            }
            Ok(())
        }
        _ => Err(format!("Unknown setting “{key}”")),
    }
}

/// (Re)registers the global capture shortcut from settings, recording the
/// outcome for the home window and the tray.
pub fn register_hotkey(app: &AppHandle) -> Result<(), String> {
    let result = try_register(app);
    *app.state::<HotkeyError>().0.lock().unwrap_or_else(|e| e.into_inner()) = result.clone().err();
    crate::tray::sync(app);
    result
}

fn try_register(app: &AppHandle) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all()
        .map_err(|e| format!("Couldn't clear the old shortcut: {e}"))?;
    let hk = crate::settings::hotkey(app);
    if hk.is_empty() {
        return Ok(());
    }
    gs.on_shortcut(hk.as_str(), |app, _shortcut, event| {
        // The handler sees both key-down and key-up; capturing on both
        // launched two overlapping screencapture sessions per press.
        if event.state() == ShortcutState::Pressed {
            crate::commands::trigger_capture(app.clone());
        }
    })
    .map_err(|e| format!("“{hk}” can't be used as the capture shortcut ({e}). It may be taken by another app."))
}

/// While the home window records a new shortcut, the current one must not
/// fire a capture.
pub fn suspend_hotkey(app: &AppHandle, suspended: bool) -> Result<(), String> {
    if suspended {
        app.global_shortcut()
            .unregister_all()
            .map_err(|e| e.to_string())
    } else {
        register_hotkey(app)
    }
}
