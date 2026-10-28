mod apply;
mod capture;
mod clipboard;
mod commands;
mod logging;
mod notify;
mod permission;
mod placement;
mod settings;
mod tray;
mod windows;

use tauri::{AppHandle, RunEvent};

/// Passed by the login item, so a login launch can stay quiet.
const AUTOSTART_ARG: &str = "--autostart";

pub fn run() {
    install_panic_hook();
    let autostart = std::env::args().any(|a| a == AUTOSTART_ARG);
    logging::info(format!(
        "Prole {} starting (pid {}, autostart: {autostart})",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    ));

    let built = tauri::Builder::default()
        .manage(commands::CaptureState::default())
        .manage(notify::Pending::default())
        .manage(apply::HotkeyError::default())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            start(app.handle(), autostart);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_capture,
            commands::get_capture,
            commands::copy_image,
            commands::get_state,
            commands::set_setting,
            commands::set_hotkey_suspended,
            commands::report_error,
            commands::log_error,
            commands::take_hud_toast,
            commands::hide_hud,
            commands::open_home,
            commands::open_log,
            commands::open_screen_recording_settings,
            commands::restart,
            commands::quit,
        ])
        .build(tauri::generate_context!());

    let app = match built {
        Ok(app) => app,
        Err(e) => {
            let msg = format!("Prole couldn't start: {e}");
            logging::error(&msg);
            notify::native_alert("Prole couldn't start", &format!("{e}\n\nLog: {}", logging::path().display()));
            std::process::exit(1);
        }
    };

    app.run(|app, event| match event {
        // Menu-bar app: closing the last window must not quit. Tauri sends
        // code=None for last-window-closed and Some(_) for app.exit() (Quit).
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        // Opening Prole again (Finder, Spotlight, Dock) while it's running.
        // Always answer with a window — a menu-bar icon alone can be hidden
        // behind the notch, which made a running Prole look dead.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => {
            logging::info("reopened");
            show_home(app);
            if settings::floating_enabled(app) {
                let _ = windows::show_floating(app);
            }
        }
        _ => {}
    });
}

/// Launch-time setup. Nothing in here may abort launch: each piece that fails
/// is reported and the rest still starts.
fn start(app: &AppHandle, autostart: bool) {
    if let Err(e) = tray::build(app) {
        notify::error(app, "Menu bar icon unavailable", e.to_string(), None);
    }
    if settings::floating_enabled(app) {
        if let Err(e) = windows::show_floating(app) {
            notify::error(app, "Floating button unavailable", e.to_string(), None);
        }
    }
    if let Err(e) = apply::register_hotkey(app) {
        notify::error(app, "Capture shortcut unavailable", e, Some(notify::Action::OpenHome));
    }
    // Opened by hand → show the home window so it's obvious Prole is running.
    // At login stay quiet; the pill and menu bar icon are enough.
    if !autostart {
        show_home(app);
    }
}

fn show_home(app: &AppHandle) {
    if let Err(e) = windows::open_home(app) {
        notify::error(app, "Couldn't open Prole", e.to_string(), None);
    }
}

/// Panics are logged and raised as a native alert instead of vanishing.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        logging::error(format!("panic: {info}"));
        notify::native_alert(
            "Prole hit an unexpected error",
            &format!("{info}\n\nLog: {}", logging::path().display()),
        );
        default(info);
    }));
}
