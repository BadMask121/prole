//! macOS's own screenshot shortcuts. The system handles these before any
//! app's global hotkey, so Prole can only answer ⇧⌘4 once macOS's
//! "Save picture of selected area as a file" is switched off.

use std::io::Write;
use std::process::{Command, Stdio};

/// What Prole registers when "Use ⇧⌘4" is on.
pub const AREA_SHORTCUT: &str = "Cmd+Shift+4";

/// macOS's screenshot shortcuts: (AppleSymbolicHotKeys id, keycaps, ascii,
/// virtual key code, modifier mask).
const SCREENSHOT: [(&str, &str, u32, u32, u32); 5] = [
    ("28", "⇧⌘3", 51, 20, 0x12_0000),
    ("29", "⌃⇧⌘3", 51, 20, 0x16_0000),
    ("30", "⇧⌘4", 52, 21, 0x12_0000),
    ("31", "⌃⇧⌘4", 52, 21, 0x16_0000),
    ("184", "⇧⌘5", 53, 23, 0x12_0000),
];
const AREA_ID: &str = "30";

/// Whether a macOS symbolic hotkey is on. An entry that was never customised
/// is absent from the plist, which means it has its factory default: on.
fn enabled(id: &str) -> bool {
    let exported = match Command::new("/usr/bin/defaults")
        .args(["export", "com.apple.symbolichotkeys", "-"])
        .output()
    {
        Ok(out) if out.status.success() => out.stdout,
        _ => return true,
    };
    let Ok(mut plutil) = Command::new("/usr/bin/plutil")
        .args(["-extract", &format!("AppleSymbolicHotKeys.{id}.enabled"), "raw", "-o", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return true;
    };
    if let Some(mut stdin) = plutil.stdin.take() {
        let _ = stdin.write_all(&exported);
    }
    match plutil.wait_with_output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim() != "false",
        _ => true,
    }
}

pub fn macos_area_shortcut_enabled() -> bool {
    enabled(AREA_ID)
}

/// Switch macOS's ⇧⌘4 on or off and apply it without logging out.
pub fn set_macos_area_shortcut(on: bool) -> Result<(), String> {
    let (id, _, ascii, key, mods) = SCREENSHOT
        .iter()
        .find(|s| s.0 == AREA_ID)
        .copied()
        .expect("area shortcut is in the table");
    let entry = format!(
        "<dict><key>enabled</key><{on}/><key>value</key><dict><key>parameters</key>\
         <array><integer>{ascii}</integer><integer>{key}</integer><integer>{mods}</integer></array>\
         <key>type</key><string>standard</string></dict></dict>"
    );
    let out = Command::new("/usr/bin/defaults")
        .args(["write", "com.apple.symbolichotkeys", "AppleSymbolicHotKeys", "-dict-add", id, &entry])
        .output()
        .map_err(|e| format!("Couldn't change macOS's ⇧⌘4: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "Couldn't change macOS's ⇧⌘4: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    // Reloads keyboard shortcuts for the session; without it the change only
    // lands after logging out.
    let applied = Command::new(
        "/System/Library/PrivateFrameworks/SystemAdministration.framework/Resources/activateSettings",
    )
    .arg("-u")
    .status();
    if !matches!(applied, Ok(s) if s.success()) {
        crate::logging::error(format!("activateSettings -u failed ({applied:?}); ⇧⌘4 change applies after logout"));
    }
    crate::logging::info(format!("macOS ⇧⌘4 screenshot shortcut {}", if on { "restored" } else { "switched off" }));
    Ok(())
}

/// Normalised (modifiers, key) of an accelerator, e.g. "Shift+Cmd+4" →
/// (CMD|SHIFT, "4"). Modifier bits match macOS's mask layout.
fn parse(accelerator: &str) -> Option<(u32, String)> {
    let mut mods = 0;
    let mut key = None;
    for token in accelerator.split('+').map(|t| t.trim().to_uppercase()) {
        match token.as_str() {
            "SHIFT" => mods |= 0x2_0000,
            "CTRL" | "CONTROL" => mods |= 0x4_0000,
            "ALT" | "OPTION" => mods |= 0x8_0000,
            "CMD" | "COMMAND" | "SUPER" | "CMDORCTRL" | "CMDORCONTROL" | "COMMANDORCTRL"
            | "COMMANDORCONTROL" => mods |= 0x10_0000,
            k => key = Some(k.strip_prefix("DIGIT").unwrap_or(k).to_string()),
        }
    }
    Some((mods, key?))
}

/// True for any spelling of ⇧⌘4.
pub fn is_area_shortcut(accelerator: &str) -> bool {
    parse(accelerator) == Some((0x12_0000, "4".to_string()))
}

/// The macOS screenshot shortcut (still switched on) that would swallow this
/// accelerator before Prole sees it, as keycaps.
pub fn conflict(accelerator: &str) -> Option<&'static str> {
    conflict_with(accelerator, enabled)
}

fn conflict_with(accelerator: &str, is_on: impl Fn(&str) -> bool) -> Option<&'static str> {
    let (mods, key) = parse(accelerator)?;
    SCREENSHOT
        .iter()
        .find(|(id, _, ascii, _, mask)| {
            *mask == mods && key == char::from_u32(*ascii).unwrap_or_default().to_string() && is_on(id)
        })
        .map(|s| s.1)
}

/// ⌘ plus one key and nothing else — the space every app uses for its own
/// commands (⌘A, ⌘C, ⌘S…).
pub fn is_plain_command(accelerator: &str) -> bool {
    matches!(parse(accelerator), Some((0x10_0000, _)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_plain_command_shortcuts_only() {
        assert!(is_plain_command("Cmd+A"));
        assert!(is_plain_command("CmdOrCtrl+Slash"));
        assert!(!is_plain_command("Shift+Cmd+A"));
        assert!(!is_plain_command("Ctrl+Shift+A"));
        assert!(!is_plain_command("Alt+Cmd+A"));
    }

    #[test]
    fn spots_macos_screenshot_shortcuts_in_any_spelling() {
        let on = |_: &str| true;
        assert_eq!(conflict_with("Cmd+Shift+4", on), Some("⇧⌘4"));
        assert_eq!(conflict_with("Shift+Cmd+4", on), Some("⇧⌘4"));
        assert_eq!(conflict_with("CmdOrCtrl+Shift+Digit3", on), Some("⇧⌘3"));
        assert_eq!(conflict_with("Ctrl+Shift+Cmd+4", on), Some("⌃⇧⌘4"));
        assert_eq!(conflict_with("Shift+Cmd+5", on), Some("⇧⌘5"));
    }

    #[test]
    fn recognises_cmd_shift_4_however_spelled() {
        assert!(is_area_shortcut("Cmd+Shift+4"));
        assert!(is_area_shortcut("Shift+Cmd+Digit4"));
        assert!(is_area_shortcut("CmdOrCtrl+Shift+4"));
        assert!(!is_area_shortcut("Ctrl+Shift+Cmd+4"));
        assert!(!is_area_shortcut("Cmd+Shift+3"));
    }

    #[test]
    fn ignores_other_shortcuts_and_switched_off_ones() {
        let on = |_: &str| true;
        assert_eq!(conflict_with("Ctrl+Shift+A", on), None);
        assert_eq!(conflict_with("Alt+Shift+4", on), None);
        assert_eq!(conflict_with("Cmd+4", on), None);
        assert_eq!(conflict_with("Cmd+Shift+4", |id| id != "30"), None);
    }
}
