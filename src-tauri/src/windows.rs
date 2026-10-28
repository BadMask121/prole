use tauri::{
    AppHandle, Emitter, Manager, TitleBarStyle, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

use crate::placement::{self, Rect};

/// Window footprints in logical points. The pill and HUD windows are
/// transparent; the extra room around their content is for the CSS shadow.
pub const FLOATING_SIZE: (f64, f64) = (156.0, 64.0);
const HUD_SIZE: (f64, f64) = (420.0, 168.0);

/// Brings a window forward. An accessory (menu-bar) app isn't active by
/// default, so `set_focus` is what actually puts the window in front.
fn present(w: &WebviewWindow) -> tauri::Result<()> {
    w.show()?;
    w.unminimize()?;
    w.set_focus()
}

/// The home window: capture button, permission status and settings. Shown on
/// launch and whenever Prole is opened again, so the app is never invisible.
pub fn open_home(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("main") {
        present(&w)?;
        w.emit("home-shown", ())?;
        return Ok(());
    }
    let w = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("src/home.html".into()))
        .title("Prole")
        .inner_size(440.0, 600.0)
        .resizable(false)
        .maximizable(false)
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .center()
        .focused(true)
        .build()?;
    let handle = app.clone();
    w.on_window_event(move |event| {
        if matches!(event, WindowEvent::Focused(false) | WindowEvent::Destroyed) {
            crate::apply::resume_hotkey_if_suspended(&handle);
        }
    });
    Ok(())
}

pub fn open_editor(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("editor") {
        present(&w)?;
        w.emit("capture-updated", ())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "editor", WebviewUrl::App("src/editor.html".into()))
        .title("Prole — Markup")
        .inner_size(1040.0, 760.0)
        .min_inner_size(720.0, 480.0)
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .center()
        .focused(true)
        .build()?;
    Ok(())
}

/// Logical bounds of every connected screen, primary first.
fn screens(app: &AppHandle) -> Vec<Rect> {
    let to_rect = |m: &tauri::Monitor| {
        let (p, s) = (m.position(), m.size());
        placement::logical_rect(p.x, p.y, s.width, s.height, m.scale_factor())
    };
    let primary = app.primary_monitor().ok().flatten().map(|m| to_rect(&m));
    let others = app.available_monitors().unwrap_or_default();
    primary
        .into_iter()
        .chain(others.iter().map(to_rect).filter(|r| Some(*r) != primary))
        .collect()
}

pub fn show_floating(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("floating") {
        w.show()?;
        w.emit("floating-shown", ())?;
        return Ok(());
    }
    let saved = crate::settings::floating_position(app);
    let (x, y) = placement::floating_position(&screens(app), saved, FLOATING_SIZE);
    if saved.is_some() && saved != Some((x, y)) {
        crate::logging::info(format!("floating pill: saved {saved:?} is off-screen, moved to ({x}, {y})"));
    }
    let w = WebviewWindowBuilder::new(app, "floating", WebviewUrl::App("src/floating.html".into()))
        .title("Prole")
        .inner_size(FLOATING_SIZE.0, FLOATING_SIZE.1)
        .position(x, y)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        // First click on the pill should capture, not just focus the window.
        .accept_first_mouse(true)
        .build()?;

    // Save in logical points. The old JS handler saved onMoved's
    // PhysicalPosition, which the builder then read back as logical — on a
    // Retina screen the pill jumped to 2× its position each launch, off-screen.
    let win = w.clone();
    w.on_window_event(move |event| {
        if let WindowEvent::Moved(pos) = event {
            let p = pos.to_logical::<f64>(win.scale_factor().unwrap_or(1.0));
            crate::settings::remember_floating_position(win.app_handle(), p.x, p.y);
        }
    });
    Ok(())
}

pub fn hide_floating(app: &AppHandle) -> tauri::Result<()> {
    match app.get_webview_window("floating") {
        Some(w) => w.hide(),
        None => Ok(()),
    }
}

pub fn floating_visible(app: &AppHandle) -> bool {
    app.get_webview_window("floating")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Shows or hides the pill and persists the choice. hide()/show() on one
/// persistent window rather than close/rebuild: `close()` tears down
/// asynchronously, so a rebuild can race it and fail with "a window with
/// label `floating` already exists".
pub fn set_floating(app: &AppHandle, visible: bool) -> Result<(), String> {
    let shown = if visible { show_floating(app) } else { hide_floating(app) };
    shown.map_err(|e| format!("Couldn't {} the floating button: {e}", if visible { "show" } else { "hide" }))?;
    crate::settings::set(app, "floating_enabled", visible.into())?;
    crate::tray::sync(app);
    Ok(())
}

/// The toast window. Created hidden on first use, then reused.
pub fn show_hud(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let w = match app.get_webview_window("hud") {
        Some(w) => w,
        None => WebviewWindowBuilder::new(app, "hud", WebviewUrl::App("src/hud.html".into()))
            .title("Prole")
            .inner_size(HUD_SIZE.0, HUD_SIZE.1)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .accept_first_mouse(true)
            .visible(false)
            .build()?,
    };
    if let Some(screen) = screens(app).first() {
        let (x, y) = placement::top_center(*screen, HUD_SIZE);
        w.set_position(tauri::LogicalPosition::new(x, y))?;
    }
    w.show()?;
    Ok(w)
}

pub fn hide_hud(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("hud") {
        let _ = w.hide();
    }
}
