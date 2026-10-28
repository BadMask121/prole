//! Every user-facing failure goes through here so nothing fails silently:
//! it's logged, shown in the HUD toast, and — if even the HUD can't be shown —
//! raised as a native alert.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Success,
    Error,
}

/// Button shown on the toast; the HUD maps each to a command. The names are
/// the serialized ids hud.ts matches on, hence the shared prefix.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub enum Action {
    OpenScreenRecording,
    OpenLog,
    OpenHome,
}

#[derive(Clone, Debug, Serialize)]
pub struct Toast {
    pub kind: Kind,
    pub title: String,
    pub message: String,
    pub action: Option<Action>,
}

/// Latest toast not yet picked up by the HUD page. A freshly built HUD webview
/// misses events emitted before its script loads, so it pulls this on load.
#[derive(Default)]
pub struct Pending(pub Mutex<Option<Toast>>);

pub fn error(app: &AppHandle, title: &str, detail: impl AsRef<str>, action: Option<Action>) {
    let detail = detail.as_ref();
    crate::logging::error(format!("{title}: {detail}"));
    show(
        app,
        Toast {
            kind: Kind::Error,
            title: title.into(),
            message: detail.into(),
            action: action.or(Some(Action::OpenLog)),
        },
    );
}

pub fn success(app: &AppHandle, title: &str, message: &str) {
    crate::logging::info(format!("{title}: {message}"));
    show(
        app,
        Toast {
            kind: Kind::Success,
            title: title.into(),
            message: message.into(),
            action: None,
        },
    );
}

fn show(app: &AppHandle, toast: Toast) {
    *app.state::<Pending>().0.lock().unwrap_or_else(|e| e.into_inner()) = Some(toast.clone());
    let shown = crate::windows::show_hud(app).and_then(|w| w.emit("hud-toast", &toast));
    if let Err(e) = shown {
        crate::logging::error(format!("HUD unavailable ({e}); falling back to a native alert"));
        if matches!(toast.kind, Kind::Error) {
            native_alert(&toast.title, &toast.message);
        }
    }
}

/// System alert via osascript. Works with no AppHandle and no event loop, so
/// it's the channel for launch failures and panics. Text goes in as argv,
/// never spliced into the script, so quotes in messages can't break it.
pub fn native_alert(title: &str, message: &str) {
    let spawned = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display alert (item 1 of argv) message (item 2 of argv) as critical",
            "-e",
            "end run",
            title,
            message,
        ])
        .spawn();
    if let Err(e) = spawned {
        crate::logging::error(format!("native alert failed: {e}"));
    }
}
