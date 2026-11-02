//! Saving the annotated capture to a PNG the user picks in the save panel.

use std::path::{Path, PathBuf};

use base64::Engine;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// Settings key for the folder the last save went to.
const SAVE_DIR: &str = "save_dir";

/// Shows the save panel as a sheet on `window` and writes the PNG there.
/// `Ok(None)` means the user cancelled.
pub async fn save_png(
    window: WebviewWindow,
    png_base64: String,
    file_name: String,
) -> Result<Option<PathBuf>, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png_base64)
        .map_err(|e| format!("The image data is corrupt: {e}"))?;
    let app = window.app_handle().clone();

    let mut panel = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_file_name(file_name)
        .add_filter("PNG image", &["png"]);
    if let Some(dir) = start_dir(&app) {
        panel = panel.set_directory(dir);
    }
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    panel.save_file(move |picked| {
        let _ = tx.send(picked);
    });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|e| e.to_string())?;
    let Some(picked) = picked else {
        return Ok(None);
    };

    let path = with_png_extension(picked.into_path().map_err(|e| e.to_string())?);
    std::fs::write(&path, bytes)
        .map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;
    crate::logging::info(format!("saved capture to {}", path.display()));
    if let Some(dir) = path.parent() {
        let _ = crate::settings::set(&app, SAVE_DIR, dir.display().to_string().into());
    }
    Ok(Some(path))
}

/// Where the panel opens: the folder last saved to, else the folder macOS
/// saves screenshots to, else the Desktop.
fn start_dir(app: &AppHandle) -> Option<PathBuf> {
    let home = app.path().home_dir().ok();
    let last = crate::settings::get(app, SAVE_DIR)
        .and_then(|v| v.as_str().map(PathBuf::from));
    let screenshots = home
        .as_deref()
        .and_then(|h| macos_screenshot_dir().map(|d| expand_tilde(&d, h)));
    first_existing_dir([last, screenshots, app.path().desktop_dir().ok()])
}

/// `defaults read com.apple.screencapture location`; unset on a stock Mac.
fn macos_screenshot_dir() -> Option<String> {
    let out = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "com.apple.screencapture", "location"])
        .output()
        .ok()?;
    let dir = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !dir.is_empty()).then_some(dir)
}

fn expand_tilde(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(path),
    }
}

/// The first candidate that is a folder that still exists (a saved folder
/// can be deleted, or sit on a drive that's been unplugged).
fn first_existing_dir(candidates: impl IntoIterator<Item = Option<PathBuf>>) -> Option<PathBuf> {
    candidates.into_iter().flatten().find(|d| d.is_dir())
}

/// The panel filters to PNG, but a name typed without an extension can still
/// come back bare, and a file Finder can't open as an image is no use.
fn with_png_extension(path: PathBuf) -> PathBuf {
    let is_png = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"));
    if is_png {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".png");
        PathBuf::from(name)
    }
}

/// Selects `path` in a Finder window.
pub fn reveal(path: &Path) -> Result<(), String> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Couldn't show {} in Finder", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_a_leading_tilde() {
        let home = Path::new("/Users/me");
        assert_eq!(expand_tilde("~/Pictures/Shots", home), PathBuf::from("/Users/me/Pictures/Shots"));
        assert_eq!(expand_tilde("~", home), PathBuf::from("/Users/me"));
        assert_eq!(expand_tilde("/Volumes/X", home), PathBuf::from("/Volumes/X"));
    }

    #[test]
    fn skips_missing_folders() {
        let tmp = std::env::temp_dir();
        let gone = tmp.join("prole-no-such-folder-3f9a");
        assert_eq!(first_existing_dir([None, Some(gone), Some(tmp.clone())]), Some(tmp));
        assert_eq!(first_existing_dir([None, None]), None);
    }

    #[test]
    fn ensures_a_png_extension() {
        assert_eq!(with_png_extension("/a/Shot.png".into()), PathBuf::from("/a/Shot.png"));
        assert_eq!(with_png_extension("/a/Shot.PNG".into()), PathBuf::from("/a/Shot.PNG"));
        assert_eq!(with_png_extension("/a/Shot".into()), PathBuf::from("/a/Shot.png"));
        assert_eq!(with_png_extension("/a/v1.2".into()), PathBuf::from("/a/v1.2.png"));
    }
}
