use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

/// Menu items whose state changes after launch.
pub struct Items {
    capture: MenuItem<Wry>,
    floating: CheckMenuItem<Wry>,
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let capture = MenuItem::with_id(app, "capture", "Capture Region", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Prole…", true, None::<&str>)?;
    let floating = CheckMenuItem::with_id(
        app,
        "floating",
        "Show Floating Button",
        true,
        crate::settings::floating_enabled(app),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Prole", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(app, &[&capture, &sep()?, &open, &floating, &sep()?, &quit])?;

    TrayIconBuilder::with_id("prole")
        .icon(Image::from_bytes(include_bytes!("../icons/tray-template.png"))?)
        .icon_as_template(true)
        .tooltip("Prole")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "capture" => crate::commands::trigger_capture(app.clone()),
            "open" => {
                if let Err(e) = crate::windows::open_home(app) {
                    crate::notify::error(app, "Couldn't open Prole", e.to_string(), None);
                }
            }
            "floating" => {
                let visible = !crate::windows::floating_visible(app);
                if let Err(e) = crate::windows::set_floating(app, visible) {
                    crate::notify::error(app, "Floating button", e, None);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    app.manage(Items { capture, floating });
    sync(app);
    Ok(())
}

/// Reflect current settings (pill visibility, shortcut) in the menu.
pub fn sync(app: &AppHandle) {
    let Some(items) = app.try_state::<Items>() else { return };
    let _ = items.floating.set_checked(crate::settings::floating_enabled(app));
    let hotkey = crate::settings::hotkey(app);
    // The accelerator here is display-only (menu key equivalent); the real
    // global shortcut is registered in apply.rs. An unparseable one is skipped.
    if items.capture.set_accelerator(Some(hotkey.as_str())).is_err() {
        let _ = items.capture.set_accelerator(None::<&str>);
    }
}
