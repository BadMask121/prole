use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture cancelled")]
    Cancelled,
    #[error("Prole doesn't have Screen Recording permission")]
    PermissionDenied,
    #[error("couldn't start screencapture: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("screencapture failed: {0}")]
    Failed(String),
}

/// Seam so tests can run without invoking the real binary.
pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<Output>;
}

pub struct ScreencaptureRunner;
impl CommandRunner for ScreencaptureRunner {
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<Output> {
        std::process::Command::new(program).args(args).output()
    }
}

/// Runs interactive region capture to `out_path`. Returns the path if a
/// non-empty file was produced. With no file, decides *why*: missing
/// permission, a reported failure, or the user pressing Esc.
pub fn capture_region_with<R: CommandRunner>(
    runner: &R,
    out_path: &Path,
    has_permission: impl Fn() -> bool,
) -> Result<PathBuf, CaptureError> {
    // A leftover file from an earlier capture would otherwise read as success
    // when this one is cancelled.
    let _ = std::fs::remove_file(out_path);
    let path_str = out_path
        .to_str()
        .ok_or_else(|| CaptureError::Failed("temp path is not valid UTF-8".into()))?;
    let out = runner.run("/usr/sbin/screencapture", &["-i", "-x", path_str])?;
    match std::fs::metadata(out_path) {
        Ok(meta) if meta.len() > 0 => return Ok(out_path.to_path_buf()),
        _ => {}
    }
    if !has_permission() {
        return Err(CaptureError::PermissionDenied);
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() && !stderr.is_empty() {
        return Err(CaptureError::Failed(stderr));
    }
    Err(CaptureError::Cancelled)
}

/// Captures to a fresh temp file; the name is unique per capture so an open
/// editor never reads a half-written replacement.
pub fn capture_region() -> Result<PathBuf, CaptureError> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("prole-{}-{n}.png", std::process::id()));
    capture_region_with(&ScreencaptureRunner, &out, crate::permission::has_screen_recording)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    struct FakeRunner {
        writes_file: bool,
        code: i32,
        stderr: &'static str,
    }
    impl CommandRunner for FakeRunner {
        fn run(&self, _p: &str, args: &[&str]) -> std::io::Result<Output> {
            if self.writes_file {
                std::fs::write(args.last().unwrap(), b"\x89PNGfake")?;
            }
            Ok(Output {
                status: ExitStatus::from_raw(self.code << 8),
                stdout: Vec::new(),
                stderr: self.stderr.as_bytes().to_vec(),
            })
        }
    }

    fn out(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("prole_capture_tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn returns_path_when_file_written() {
        let path = out("ok.png");
        let runner = FakeRunner { writes_file: true, code: 0, stderr: "" };
        let got = capture_region_with(&runner, &path, || true).unwrap();
        assert_eq!(got, path);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn cancelled_when_no_file() {
        let runner = FakeRunner { writes_file: false, code: 1, stderr: "" };
        assert!(matches!(
            capture_region_with(&runner, &out("none.png"), || true),
            Err(CaptureError::Cancelled)
        ));
    }

    #[test]
    fn stale_file_from_an_earlier_capture_is_not_success() {
        let path = out("stale.png");
        std::fs::write(&path, b"old capture").unwrap();
        let runner = FakeRunner { writes_file: false, code: 1, stderr: "" };
        assert!(matches!(
            capture_region_with(&runner, &path, || true),
            Err(CaptureError::Cancelled)
        ));
    }

    #[test]
    fn missing_permission_is_reported() {
        let runner = FakeRunner { writes_file: false, code: 1, stderr: "" };
        assert!(matches!(
            capture_region_with(&runner, &out("denied.png"), || false),
            Err(CaptureError::PermissionDenied)
        ));
    }

    #[test]
    fn screencapture_errors_are_reported() {
        let runner = FakeRunner { writes_file: false, code: 1, stderr: "could not create image from display" };
        match capture_region_with(&runner, &out("failed.png"), || true) {
            Err(CaptureError::Failed(msg)) => assert!(msg.contains("could not create image")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
