//! Saving the annotated capture as a PNG: straight into the save folder
//! (⌘S), or wherever the user points the save panel (⇧⌘S).

use std::path::{Path, PathBuf};

use base64::Engine;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, FilePath};

/// Settings key for the folder picked in the home window (null = default).
const SAVE_DIR: &str = "save_dir";

/// Writes the PNG into the save folder, or — when `ask` is set or that folder
/// is gone — wherever the user picks in the save panel. Returns the saved
/// path; `Ok(None)` means the user cancelled the panel.
pub async fn save_png(
    window: WebviewWindow,
    png_base64: String,
    file_name: String,
    ask: bool,
) -> Result<Option<PathBuf>, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png_base64)
        .map_err(|e| format!("The image data is corrupt: {e}"))?;
    let app = window.app_handle().clone();
    let folder = save_folder(&app);

    let path = if !ask && folder.is_dir() {
        unique_path(&folder, &file_name)
    } else {
        if !ask {
            crate::logging::info(format!(
                "save folder {} is missing; asking where to save",
                folder.display()
            ));
        }
        let start = first_existing_dir([Some(folder), default_folder(&app)]);
        let mut panel = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_file_name(file_name)
            .add_filter("PNG image", &["png"]);
        if let Some(dir) = start {
            panel = panel.set_directory(dir);
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        panel.save_file(move |picked| {
            let _ = tx.send(picked);
        });
        match wait_for(rx).await? {
            Some(picked) => with_png_extension(picked.into_path().map_err(|e| e.to_string())?),
            None => return Ok(None),
        }
    };

    std::fs::write(&path, bytes)
        .map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;
    crate::logging::info(format!("saved capture to {}", path.display()));
    Ok(Some(path))
}

/// Folder picker for the home window's Save folder setting. Returns the new
/// folder, or `Ok(None)` if the user cancelled.
pub async fn choose_folder(window: WebviewWindow) -> Result<Option<PathBuf>, String> {
    let app = window.app_handle().clone();
    let start = first_existing_dir([Some(save_folder(&app)), default_folder(&app)]);
    let mut panel = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Save folder")
        .set_can_create_directories(true);
    if let Some(dir) = start {
        panel = panel.set_directory(dir);
    }
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    panel.pick_folder(move |picked| {
        let _ = tx.send(picked);
    });
    let Some(picked) = wait_for(rx).await? else {
        return Ok(None);
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    crate::settings::set(&app, SAVE_DIR, dir.display().to_string().into())?;
    crate::logging::info(format!("save folder set to {}", dir.display()));
    Ok(Some(dir))
}

/// The panel answers on its own thread; wait for it off the async runtime.
async fn wait_for(
    rx: std::sync::mpsc::Receiver<Option<FilePath>>,
) -> Result<Option<FilePath>, String> {
    tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|e| e.to_string())
}

/// Where ⌘S saves: the folder picked in the home window, else the default.
/// The picked folder is returned even if it's gone (unplugged drive), so the
/// home window can say so instead of silently saving somewhere else.
pub fn save_folder(app: &AppHandle) -> PathBuf {
    crate::settings::get(app, SAVE_DIR)
        .and_then(|v| v.as_str().map(PathBuf::from))
        .or_else(|| default_folder(app))
        .or_else(|| app.path().home_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The folder macOS saves screenshots to, else the Desktop.
fn default_folder(app: &AppHandle) -> Option<PathBuf> {
    let screenshots = app
        .path()
        .home_dir()
        .ok()
        .and_then(|h| macos_screenshot_dir().map(|d| expand_tilde(&d, &h)));
    first_existing_dir([screenshots, app.path().desktop_dir().ok()])
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

/// `/Users/me/Desktop` → `~/Desktop`, for showing a folder in the UI.
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// The first candidate that is a folder that still exists.
fn first_existing_dir(candidates: impl IntoIterator<Item = Option<PathBuf>>) -> Option<PathBuf> {
    candidates.into_iter().flatten().find(|d| d.is_dir())
}

/// `dir/name`, or `dir/name (2).png`, `(3)`… if that's taken — two saves in
/// the same second must not overwrite each other.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = with_png_extension(dir.join(name));
    if !first.exists() {
        return first;
    }
    let stem = first
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).png")))
        .find(|p| !p.exists())
        .expect("an unused name")
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
    fn shows_home_folders_with_a_tilde() {
        let home = Some(Path::new("/Users/me"));
        assert_eq!(display_path(Path::new("/Users/me/Desktop"), home), "~/Desktop");
        assert_eq!(display_path(Path::new("/Users/me"), home), "~");
        assert_eq!(display_path(Path::new("/Users/meg/Desktop"), home), "/Users/meg/Desktop");
        assert_eq!(display_path(Path::new("/Volumes/Shots"), None), "/Volumes/Shots");
    }

    #[test]
    fn skips_missing_folders() {
        let tmp = std::env::temp_dir();
        let gone = tmp.join("prole-no-such-folder-3f9a");
        assert_eq!(first_existing_dir([None, Some(gone), Some(tmp.clone())]), Some(tmp));
        assert_eq!(first_existing_dir([None, None]), None);
    }

    #[test]
    fn never_overwrites_an_earlier_save() {
        let dir = std::env::temp_dir().join(format!("prole-unique-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = "Prole 2026-11-02 at 09.04.07.png";
        assert_eq!(unique_path(&dir, name), dir.join(name));
        std::fs::write(dir.join(name), b"x").unwrap();
        assert_eq!(unique_path(&dir, name), dir.join("Prole 2026-11-02 at 09.04.07 (2).png"));
        std::fs::write(dir.join("Prole 2026-11-02 at 09.04.07 (2).png"), b"x").unwrap();
        assert_eq!(unique_path(&dir, name), dir.join("Prole 2026-11-02 at 09.04.07 (3).png"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ensures_a_png_extension() {
        assert_eq!(with_png_extension("/a/Shot.png".into()), PathBuf::from("/a/Shot.png"));
        assert_eq!(with_png_extension("/a/Shot.PNG".into()), PathBuf::from("/a/Shot.PNG"));
        assert_eq!(with_png_extension("/a/Shot".into()), PathBuf::from("/a/Shot.png"));
        assert_eq!(with_png_extension("/a/v1.2".into()), PathBuf::from("/a/v1.2.png"));
    }
}
