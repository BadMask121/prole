use std::sync::atomic::{AtomicBool, Ordering};
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
            // macOS takes its screenshot shortcuts before any app does, so
            // registering one would "work" and then never fire.
            if let Some(caps) = value.as_str().and_then(crate::system_shortcuts::conflict) {
                let hint = if caps == "⇧⌘4" { " Turn on “Use ⇧⌘4” to hand it to Prole." } else { "" };
                return Err(format!("macOS uses {caps} for screenshots.{hint}"));
            }
            // ⌘ + one key is every app's own shortcut (⌘A Select All, ⌘C
            // Copy…); taking one globally breaks it everywhere.
            if value.as_str().is_some_and(crate::system_shortcuts::is_plain_command) {
                return Err("Apps already use ⌘ + a single key (⌘A is Select All). Add ⇧ or ⌥.".into());
            }
            let previous = crate::settings::hotkey(app);
            crate::settings::set(app, key, value)?;
            if let Err(e) = register_hotkey(app) {
                crate::settings::set(app, key, previous.into())?;
                let _ = register_hotkey(app);
                return Err(e);
            }
            Ok(())
        }
        "use_macos_shortcut" => {
            let on = value.as_bool().ok_or("expected true/false")?;
            set_macos_area_shortcut_for_prole(on)?;
            crate::settings::set(app, key, on.into())?;
            if let Err(e) = register_hotkey(app) {
                // Hand ⇧⌘4 back to macOS rather than leave it doing nothing.
                let _ = set_macos_area_shortcut_for_prole(false);
                crate::settings::set(app, key, false.into())?;
                let _ = register_hotkey(app);
                return Err(e);
            }
            Ok(())
        }
        _ => Err(format!("Unknown setting “{key}”")),
    }
}

/// Prole takes ⇧⌘4 by switching macOS's off (and gives it back by switching
/// it on). Skips the system call when it's already in the wanted state.
fn set_macos_area_shortcut_for_prole(prole_has_it: bool) -> Result<(), String> {
    let macos_on = crate::system_shortcuts::macos_area_shortcut_enabled();
    if macos_on == prole_has_it {
        crate::system_shortcuts::set_macos_area_shortcut(!prole_has_it)?;
    }
    Ok(())
}

/// Launch: if Prole is meant to have ⇧⌘4, make sure macOS isn't holding it
/// (it's handed back on quit, and could have been re-enabled in Settings).
pub fn claim_macos_shortcut_if_enabled(app: &AppHandle) {
    if crate::settings::use_macos_shortcut(app) {
        if let Err(e) = set_macos_area_shortcut_for_prole(true) {
            crate::notify::error(app, "Couldn't take over ⇧⌘4", e, Some(crate::notify::Action::OpenHome));
        }
    }
}

/// Quit: give ⇧⌘4 back to macOS so it still takes screenshots while Prole
/// isn't running.
pub fn release_macos_shortcut_if_enabled(app: &AppHandle) {
    if crate::settings::use_macos_shortcut(app) {
        if let Err(e) = set_macos_area_shortcut_for_prole(false) {
            crate::logging::error(format!("couldn't hand ⇧⌘4 back to macOS on quit: {e}"));
        }
    }
}

/// True while the home window is recording a new shortcut (the current one is
/// unregistered meanwhile).
static SUSPENDED: AtomicBool = AtomicBool::new(false);

/// (Re)registers the global capture shortcut from settings, recording the
/// outcome for the home window and the tray.
pub fn register_hotkey(app: &AppHandle) -> Result<(), String> {
    SUSPENDED.store(false, Ordering::SeqCst);
    let result = try_register(app);
    match &result {
        Ok(keys) if keys.is_empty() => crate::logging::info("capture shortcut: off"),
        Ok(keys) => crate::logging::info(format!("capture shortcut: {} registered", keys.join(", "))),
        Err(e) => crate::logging::error(format!("capture shortcut: {e}")),
    }
    let result = result.map(|_| ());
    *app.state::<HotkeyError>().0.lock().unwrap_or_else(|e| e.into_inner()) = result.clone().err();
    crate::tray::sync(app);
    result
}

/// Registers the custom shortcut and, if enabled, ⇧⌘4. Returns what's active.
fn try_register(app: &AppHandle) -> Result<Vec<String>, String> {
    let gs = app.global_shortcut();
    gs.unregister_all()
        .map_err(|e| format!("Couldn't clear the old shortcut: {e}"))?;
    let custom = crate::settings::hotkey(app);
    let mut keys: Vec<String> = Vec::new();
    if !custom.is_empty() {
        keys.push(custom);
    }
    if crate::settings::use_macos_shortcut(app)
        && !keys.iter().any(|k| crate::system_shortcuts::is_area_shortcut(k))
    {
        keys.push(crate::system_shortcuts::AREA_SHORTCUT.to_string());
    }
    for hk in &keys {
        gs.on_shortcut(hk.as_str(), |app, shortcut, event| {
            // The handler sees both key-down and key-up; capturing on both
            // launched two overlapping screencapture sessions per press.
            if event.state() == ShortcutState::Pressed {
                crate::logging::info(format!("capture shortcut pressed ({shortcut:?})"));
                crate::commands::trigger_capture(app.clone());
            }
        })
        .map_err(|e| format!("“{hk}” can't be used as the capture shortcut ({e}). It may be taken by another app."))?;
    }
    Ok(keys)
}

/// While the home window records a new shortcut, the current one must not
/// fire a capture.
pub fn suspend_hotkey(app: &AppHandle, suspended: bool) -> Result<(), String> {
    if suspended {
        app.global_shortcut()
            .unregister_all()
            .map_err(|e| e.to_string())?;
        SUSPENDED.store(true, Ordering::SeqCst);
        crate::logging::info("capture shortcut suspended while recording a new one");
        Ok(())
    } else {
        register_hotkey(app)
    }
}

/// Safety net for the recorder: if the home window loses focus or closes
/// while a recording is open, put the shortcut back so it's never left off.
pub fn resume_hotkey_if_suspended(app: &AppHandle) {
    if SUSPENDED.load(Ordering::SeqCst) {
        crate::logging::info("capture shortcut resumed (recorder left open)");
        if let Err(e) = register_hotkey(app) {
            crate::notify::error(app, "Capture shortcut unavailable", e, Some(crate::notify::Action::OpenHome));
        }
    }
}
