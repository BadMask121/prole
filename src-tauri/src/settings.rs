use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Wry};
use tauri_plugin_store::{Store, StoreExt};

const STORE: &str = "settings.json";

fn defaults() -> Value {
    json!({
        "floating_enabled": true,
        "floating_x": null,
        "floating_y": null,
        "hotkey": "CmdOrCtrl+Shift+A",
        "use_macos_shortcut": false,
        "launch_at_login": false
    })
}

/// The store, or `None` (logged) if it can't be opened — e.g. a corrupt
/// settings.json. Callers fall back to defaults instead of panicking at launch.
fn store(app: &AppHandle) -> Option<Arc<Store<Wry>>> {
    match app.store(STORE) {
        Ok(s) => Some(s),
        Err(e) => {
            crate::logging::error(format!("couldn't open {STORE}: {e}"));
            None
        }
    }
}

/// All settings = defaults overlaid with any stored overrides.
pub fn read_all(app: &AppHandle) -> Value {
    let mut out = defaults();
    if let (Some(store), Value::Object(map)) = (store(app), &mut out) {
        for (k, v) in map.iter_mut() {
            if let Some(stored) = store.get(k) {
                *v = stored;
            }
        }
    }
    out
}

pub fn get(app: &AppHandle, key: &str) -> Option<Value> {
    read_all(app).get(key).cloned()
}

pub fn set(app: &AppHandle, key: &str, value: Value) -> Result<(), String> {
    let store = store(app).ok_or("Settings file couldn't be opened — see the log")?;
    store.set(key, value);
    store
        .save()
        .map_err(|e| format!("Couldn't save settings: {e}"))
}

pub fn floating_enabled(app: &AppHandle) -> bool {
    get(app, "floating_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

pub fn hotkey(app: &AppHandle) -> String {
    get(app, "hotkey")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// Whether Prole answers ⇧⌘4 (in place of macOS's area screenshot).
pub fn use_macos_shortcut(app: &AppHandle) -> bool {
    get(app, "use_macos_shortcut")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// The shortcut to show in the UI: ⇧⌘4 when Prole has it, else the custom one.
pub fn capture_shortcut(app: &AppHandle) -> String {
    if use_macos_shortcut(app) {
        crate::system_shortcuts::AREA_SHORTCUT.to_string()
    } else {
        hotkey(app)
    }
}

/// Records the pill position (logical points). Called for every `Moved` event
/// during a drag, so it leans on the store's 100 ms debounced auto-save
/// instead of writing the file each time.
pub fn remember_floating_position(app: &AppHandle, x: f64, y: f64) {
    if let Some(store) = store(app) {
        store.set("floating_x", x);
        store.set("floating_y", y);
    }
}

/// Saved pill position in logical points, if any.
pub fn floating_position(app: &AppHandle) -> Option<(f64, f64)> {
    let x = get(app, "floating_x")?.as_f64()?;
    let y = get(app, "floating_y")?.as_f64()?;
    Some((x, y))
}
