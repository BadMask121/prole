//! Window placement in logical points (what `WebviewWindowBuilder::position`
//! takes). Kept free of Tauri types so it can be unit-tested.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Keep the pill clear of the screen edge and the menu bar.
const EDGE: f64 = 24.0;
const MENU_BAR: f64 = 40.0;

/// Logical bounds of a monitor from Tauri's physical values. On macOS tao
/// reports `position` and `size` as logical × that monitor's scale factor.
pub fn logical_rect(x: i32, y: i32, w: u32, h: u32, scale: f64) -> Rect {
    let s = if scale > 0.0 { scale } else { 1.0 };
    Rect {
        x: x as f64 / s,
        y: y as f64 / s,
        w: w as f64 / s,
        h: h as f64 / s,
    }
}

/// True if the whole window lies on one screen. Checking only the top-left
/// corner let a pill hang off the bottom-right edge, invisible.
pub fn fits(screens: &[Rect], win: Rect) -> bool {
    screens.iter().any(|s| {
        win.x >= s.x && win.y >= s.y && win.x + win.w <= s.x + s.w && win.y + win.h <= s.y + s.h
    })
}

/// Where the floating pill goes: its saved spot if that's still fully on a
/// connected screen, else the right edge of the primary screen (`screens[0]`).
pub fn floating_position(screens: &[Rect], saved: Option<(f64, f64)>, size: (f64, f64)) -> (f64, f64) {
    let (w, h) = size;
    if let Some((x, y)) = saved {
        if screens.is_empty() || fits(screens, Rect { x, y, w, h }) {
            return (x, y);
        }
    }
    match screens.first() {
        Some(s) => (s.x + s.w - w - EDGE, s.y + ((s.h - h) / 2.0).max(MENU_BAR)),
        None => (80.0, 80.0),
    }
}

/// Top-centre of a screen, just under the menu bar (HUD toasts).
pub fn top_center(screen: Rect, size: (f64, f64)) -> (f64, f64) {
    (screen.x + (screen.w - size.0) / 2.0, screen.y + MENU_BAR)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 14" MacBook Pro: 3456×2234 px at 2× = 1728×1117 pt.
    fn laptop() -> Rect {
        logical_rect(0, 0, 3456, 2234, 2.0)
    }

    #[test]
    fn converts_physical_monitor_to_points() {
        assert_eq!(laptop(), Rect { x: 0.0, y: 0.0, w: 1728.0, h: 1117.0 });
    }

    #[test]
    fn keeps_a_saved_position_that_is_on_screen() {
        let got = floating_position(&[laptop()], Some((300.0, 200.0)), (156.0, 64.0));
        assert_eq!(got, (300.0, 200.0));
    }

    #[test]
    fn rejects_physical_pixels_saved_as_points() {
        // The real bug: (3292, 2052) was saved from a PhysicalPosition and then
        // restored as logical points — far off a 1728×1117 pt screen.
        let got = floating_position(&[laptop()], Some((3292.0, 2052.0)), (156.0, 64.0));
        assert_eq!(got, (1728.0 - 156.0 - 24.0, (1117.0 - 64.0) / 2.0));
    }

    #[test]
    fn rejects_a_window_hanging_off_the_edge() {
        let got = floating_position(&[laptop()], Some((1700.0, 500.0)), (156.0, 64.0));
        assert_ne!(got, (1700.0, 500.0));
    }

    #[test]
    fn accepts_a_position_on_a_secondary_screen() {
        let external = logical_rect(-3840, 0, 3840, 2160, 1.0);
        let got = floating_position(&[laptop(), external], Some((-1000.0, 400.0)), (156.0, 64.0));
        assert_eq!(got, (-1000.0, 400.0));
    }

    #[test]
    fn hud_sits_under_the_menu_bar() {
        assert_eq!(top_center(laptop(), (400.0, 120.0)), (664.0, 40.0));
    }
}
