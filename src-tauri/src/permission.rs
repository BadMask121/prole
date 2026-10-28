#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Whether macOS currently grants Prole Screen Recording. A real TCC check,
/// not a guess from a failed capture.
pub fn has_screen_recording() -> bool {
    #[cfg(target_os = "macos")]
    // SAFETY: no arguments; documented as safe to call from any thread.
    unsafe {
        CGPreflightScreenCaptureAccess()
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Asks macOS for Screen Recording. The first call shows the system prompt and
/// adds Prole to the Settings list; later calls return the current state.
pub fn request_screen_recording() -> bool {
    #[cfg(target_os = "macos")]
    // SAFETY: as above.
    unsafe {
        CGRequestScreenCaptureAccess()
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Forgets Prole's Screen Recording entry. macOS keys the grant to the code
/// signature, so after an update an ad-hoc build can show as "on" in Settings
/// while the new binary is still refused. Clearing the entry lets the next
/// request create one that matches this build.
pub fn reset_screen_recording(bundle_id: &str) -> std::io::Result<()> {
    let out = std::process::Command::new("/usr/bin/tccutil")
        .args(["reset", "ScreenCapture", bundle_id])
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

/// Opens System Settings at the Screen Recording pane.
pub fn open_screen_recording_settings() -> std::io::Result<()> {
    let status = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("`open` exited with {status}")))
    }
}
