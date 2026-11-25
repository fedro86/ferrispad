//! Main-window geometry: validation of the stored rect, the close-time "what
//! to save" decision, and Wayland backend detection.
//!
//! Everything here is pure (no FLTK calls) so it can be unit-tested with
//! screens passed in as plain rects. The FLTK glue lives in
//! `ui::main_window`.

use crate::app::domain::settings::WindowGeometry;

/// Size used for a fresh install and whenever the stored size is rejected.
pub const DEFAULT_WIDTH: i32 = 640;
pub const DEFAULT_HEIGHT: i32 = 480;
/// Position used when nothing (meaningful) is stored.
pub const DEFAULT_X: i32 = 100;
pub const DEFAULT_Y: i32 = 100;
/// Anything smaller than this is treated as nonsense and reset to the default.
pub const MIN_WIDTH: i32 = 200;
pub const MIN_HEIGHT: i32 = 150;
/// Title-bar height assumed before the window is shown, when the real
/// decoration size (`decorated_h() - h()`) is not known yet.
pub const TITLE_BAR_ESTIMATE: i32 = 32;

/// A rectangle in FLTK screen units: a window's client area or a screen's
/// work area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl ScreenRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    fn same_size(&self, other: &ScreenRect) -> bool {
        self.w == other.w && self.h == other.h
    }
}

impl From<WindowGeometry> for ScreenRect {
    fn from(g: WindowGeometry) -> Self {
        Self::new(g.x, g.y, g.width, g.height)
    }
}

fn to_i32(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// Area (in square units) shared by two rects given as i64 `(x, y, w, h)`.
fn overlap_area(a: (i64, i64, i64, i64), b: (i64, i64, i64, i64)) -> i64 {
    let w = (a.0 + a.2).min(b.0 + b.2) - a.0.max(b.0);
    let h = (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1);
    if w <= 0 || h <= 0 { 0 } else { w * h }
}

/// Validate a stored window rect against the available screens' work areas.
///
/// - A size below `MIN_WIDTH`×`MIN_HEIGHT` (zero, negative, tiny) falls back
///   to the 640×480 default.
/// - Negative coordinates are fine: a monitor left of / above the primary one
///   has them.
/// - The rect (including a `title_bar_h` strip above the client area, since
///   FLTK's `y` is the client origin) is placed on the screen it overlaps
///   most; with no overlap at all it is centred on screen 0.
/// - It is then shrunk to fit that screen's work area and shifted fully
///   inside it, so the title bar always stays grabbable.
///
/// Screens with an empty work area are ignored; with no usable screen only
/// the size check applies.
pub fn clamp_onto_screens(
    rect: ScreenRect,
    screens: &[ScreenRect],
    title_bar_h: i32,
) -> ScreenRect {
    let (mut w, mut h) = if rect.w < MIN_WIDTH || rect.h < MIN_HEIGHT {
        (DEFAULT_WIDTH, DEFAULT_HEIGHT)
    } else {
        (rect.w, rect.h)
    };

    let usable: Vec<ScreenRect> = screens
        .iter()
        .copied()
        .filter(|s| s.w > 0 && s.h > 0)
        .collect();
    let Some(&first) = usable.first() else {
        return ScreenRect::new(rect.x, rect.y, w, h);
    };

    let title = i64::from(title_bar_h.max(0));
    let decorated = (
        i64::from(rect.x),
        i64::from(rect.y) - title,
        i64::from(w),
        i64::from(h) + title,
    );
    let (best, best_overlap) = usable
        .iter()
        .map(|s| {
            let area = (
                i64::from(s.x),
                i64::from(s.y),
                i64::from(s.w),
                i64::from(s.h),
            );
            (*s, overlap_area(decorated, area))
        })
        .fold((first, 0), |acc, cur| if cur.1 > acc.1 { cur } else { acc });

    let (sx, sy, sw, sh) = (
        i64::from(best.x),
        i64::from(best.y),
        i64::from(best.w),
        i64::from(best.h),
    );

    // Shrink to fit (keeping at least 1 unit so the bounds below stay sane).
    let avail_h = (sh - title).max(1);
    w = to_i32(i64::from(w).min(sw));
    h = to_i32(i64::from(h).min(avail_h));
    let (w64, h64) = (i64::from(w), i64::from(h));

    let (min_x, max_x) = (sx, sx + sw - w64);
    let (min_y, max_y) = (sy + title, sy + sh - h64);
    let (x, y) = if best_overlap == 0 {
        // Nowhere on any screen (monitor gone): centre on screen 0.
        (sx + (sw - w64) / 2, sy + title + (sh - title - h64) / 2)
    } else {
        (i64::from(rect.x), i64::from(rect.y))
    };
    // `max(lo).min(hi)` rather than `clamp`, which panics when lo > hi.
    let x = x.max(min_x).min(max_x);
    let y = y.max(min_y).min(max_y);

    ScreenRect::new(to_i32(x), to_i32(y), w, h)
}

/// Startup rect and maximized flag from the stored geometry: the 640×480
/// default when nothing is stored (or it was rejected while loading), always
/// passed through `clamp_onto_screens`.
pub fn restore_geometry(
    stored: Option<WindowGeometry>,
    screens: &[ScreenRect],
    title_bar_h: i32,
) -> (ScreenRect, bool) {
    let (rect, maximized) = match stored {
        Some(g) => (ScreenRect::from(g), g.maximized),
        None => (
            ScreenRect::new(DEFAULT_X, DEFAULT_Y, DEFAULT_WIDTH, DEFAULT_HEIGHT),
            false,
        ),
    };
    (clamp_onto_screens(rect, screens, title_bar_h), maximized)
}

/// The window's state at the moment the user asked to close it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseSnapshot {
    /// `false` when minimized (FLTK hides an iconified window).
    pub visible: bool,
    pub maximized: bool,
    /// `x()/y()/w()/h()` at close: the maximized size if maximized.
    pub current: ScreenRect,
    /// Last known normal (un-maximized) rect, if one was seen.
    pub last_normal: Option<ScreenRect>,
}

/// What to write at close, or `None` to keep the stored value untouched.
///
/// - Minimized: `None` (Windows reports it at about -32000,-32000).
/// - Maximized: the last normal rect plus `maximized = true`, so
///   un-maximizing after the next start lands on a sane size.
/// - Normal: the current rect.
/// - Wayland: the compositor owns the position, so `x`/`y` are kept from the
///   stored geometry (e.g. one saved under X11) instead of Wayland's `0,0`.
pub fn geometry_to_save(
    snapshot: &CloseSnapshot,
    stored: Option<WindowGeometry>,
    wayland: bool,
) -> Option<WindowGeometry> {
    if !snapshot.visible {
        return None;
    }
    let rect = if snapshot.maximized {
        snapshot
            .last_normal
            .or_else(|| stored.map(ScreenRect::from))
            .unwrap_or(ScreenRect::new(
                snapshot.current.x,
                snapshot.current.y,
                DEFAULT_WIDTH,
                DEFAULT_HEIGHT,
            ))
    } else {
        snapshot.current
    };
    let (x, y) = if wayland {
        stored.map_or((DEFAULT_X, DEFAULT_Y), |g| (g.x, g.y))
    } else {
        (rect.x, rect.y)
    };
    Some(WindowGeometry {
        x,
        y,
        width: rect.w,
        height: rect.h,
        maximized: snapshot.maximized,
    })
}

/// In-memory record of the window's normal rect, fed by the window's resize
/// callback (event-driven: no timer, no disk I/O).
///
/// Keeps the previous *size* too: on X11 the `_NET_WM_STATE` maximized flag
/// can arrive after the maximizing resize, so that resize may be recorded as
/// "normal". When the latest record has the maximized size, the previous one
/// is the real normal rect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NormalRectHistory {
    last: Option<ScreenRect>,
    prev: Option<ScreenRect>,
}

impl NormalRectHistory {
    /// Record a normal-state rect. A move (same size) replaces the latest
    /// entry; a resize pushes it.
    pub fn record(&mut self, rect: ScreenRect) {
        match self.last {
            Some(last) if last.same_size(&rect) => self.last = Some(rect),
            _ => {
                self.prev = self.last;
                self.last = Some(rect);
            }
        }
    }

    /// The normal rect to save for a window currently maximized at
    /// `maximized_rect`.
    pub fn normal_for(&self, maximized_rect: ScreenRect) -> Option<ScreenRect> {
        match self.last {
            Some(last) if last.same_size(&maximized_rect) => self.prev,
            other => other,
        }
    }
}

/// Per-run geometry state shared by the window callbacks and the quit path.
#[derive(Debug, Clone, Default)]
pub struct WindowGeometryTracker {
    stored: Option<WindowGeometry>,
    wayland: bool,
    normal: NormalRectHistory,
    snapshot: Option<CloseSnapshot>,
}

impl WindowGeometryTracker {
    /// `stored` is the geometry loaded at startup; `initial` the (validated)
    /// normal rect the window was created at.
    pub fn new(stored: Option<WindowGeometry>, wayland: bool, initial: ScreenRect) -> Self {
        let mut normal = NormalRectHistory::default();
        normal.record(initial);
        Self {
            stored,
            wayland,
            normal,
            snapshot: None,
        }
    }

    /// Resize/move callback: only a visible, non-maximized rect is a normal one.
    pub fn record_resize(&mut self, visible: bool, maximized: bool, rect: ScreenRect) {
        if visible && !maximized {
            self.normal.record(rect);
        }
    }

    /// Capture the window state when a close is requested (before the
    /// window is hidden). A later capture replaces an earlier one.
    pub fn capture_close(&mut self, visible: bool, maximized: bool, current: ScreenRect) {
        let last_normal = if maximized {
            self.normal.normal_for(current)
        } else {
            None
        };
        self.snapshot = Some(CloseSnapshot {
            visible,
            maximized,
            current,
            last_normal,
        });
    }

    /// The geometry to write for the captured close, if any.
    pub fn take_geometry_to_save(&mut self) -> Option<WindowGeometry> {
        let snapshot = self.snapshot.take()?;
        geometry_to_save(&snapshot, self.stored, self.wayland)
    }
}

/// Whether FLTK picks its Wayland backend, from the environment the way FLTK
/// itself decides: `FLTK_BACKEND` wins (`wayland` / `x11`), otherwise a
/// Wayland display must be advertised. Env values are parameters so this is
/// testable; see `running_on_wayland` for the real lookup.
pub fn is_wayland_backend(fltk_backend: Option<&str>, wayland_display: Option<&str>) -> bool {
    match fltk_backend {
        Some(backend) => backend == "wayland",
        None => wayland_display.is_some_and(|d| !d.is_empty()),
    }
}

/// `is_wayland_backend` for this process (always `false` off Linux).
pub fn running_on_wayland() -> bool {
    #[cfg(target_os = "linux")]
    {
        let backend = std::env::var("FLTK_BACKEND").ok();
        let display = std::env::var("WAYLAND_DISPLAY").ok();
        is_wayland_backend(backend.as_deref(), display.as_deref())
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: ScreenRect = ScreenRect::new(0, 0, 1920, 1050);
    const LEFT: ScreenRect = ScreenRect::new(-1920, 0, 1920, 1050);
    const TITLE: i32 = 30;

    fn geom(x: i32, y: i32, width: i32, height: i32, maximized: bool) -> WindowGeometry {
        WindowGeometry {
            x,
            y,
            width,
            height,
            maximized,
        }
    }

    #[test]
    fn window_geometry_clamped_onto_screen() {
        let r = ScreenRect::new;
        let both = [PRIMARY, LEFT];
        let cases: &[(&str, ScreenRect, &[ScreenRect], ScreenRect)] = &[
            (
                "on-screen rect is unchanged",
                r(200, 150, 1100, 750),
                &[PRIMARY],
                r(200, 150, 1100, 750),
            ),
            (
                "secondary monitor at negative x is unchanged",
                r(-1500, 100, 1100, 750),
                &both,
                r(-1500, 100, 1100, 750),
            ),
            (
                "far off-screen is moved fully onto screen 0 (centred)",
                r(20000, 20000, 800, 600),
                &both,
                r(560, 240, 800, 600),
            ),
            (
                "partly off the right edge is shifted inside",
                r(1500, 100, 800, 600),
                &[PRIMARY],
                r(1120, 100, 800, 600),
            ),
            (
                "larger than the work area is shrunk to fit",
                r(0, 0, 5000, 4000),
                &[PRIMARY],
                r(0, TITLE, 1920, 1050 - TITLE),
            ),
            (
                "zero size falls back to the default",
                r(100, 100, 0, 0),
                &[PRIMARY],
                r(100, 100, DEFAULT_WIDTH, DEFAULT_HEIGHT),
            ),
            (
                "negative size falls back to the default",
                r(100, 100, -800, -600),
                &[PRIMARY],
                r(100, 100, DEFAULT_WIDTH, DEFAULT_HEIGHT),
            ),
            (
                "below the minimum falls back to the default",
                r(100, 100, MIN_WIDTH - 1, 400),
                &[PRIMARY],
                r(100, 100, DEFAULT_WIDTH, DEFAULT_HEIGHT),
            ),
            (
                "title-bar offset: y at the work-area top is pushed down",
                r(100, 0, 800, 600),
                &[PRIMARY],
                r(100, TITLE, 800, 600),
            ),
            (
                "work area not at the origin (top panel)",
                r(100, 10, 800, 600),
                &[ScreenRect::new(0, 27, 1920, 1023)],
                r(100, 27 + TITLE, 800, 600),
            ),
            (
                "extreme values do not overflow",
                r(i32::MAX, i32::MIN, i32::MAX, i32::MAX),
                &[PRIMARY],
                r(0, TITLE, 1920, 1050 - TITLE),
            ),
            (
                "no usable screen: only the size is checked",
                r(-30000, 7, 10, 10),
                &[ScreenRect::new(0, 0, 0, 0)],
                r(-30000, 7, DEFAULT_WIDTH, DEFAULT_HEIGHT),
            ),
        ];
        for (name, input, screens, expected) in cases {
            assert_eq!(
                clamp_onto_screens(*input, screens, TITLE),
                *expected,
                "{name}"
            );
        }
    }

    #[test]
    fn window_geometry_restore_defaults_and_maximized_flag() {
        assert_eq!(
            restore_geometry(None, &[PRIMARY], TITLE),
            (ScreenRect::new(100, 100, 640, 480), false)
        );
        assert_eq!(
            restore_geometry(
                Some(geom(-1500, 100, 1100, 750, true)),
                &[PRIMARY, LEFT],
                TITLE
            ),
            (ScreenRect::new(-1500, 100, 1100, 750), true)
        );
        // The monitor on the left is gone: the window lands on the primary.
        let (rect, maximized) =
            restore_geometry(Some(geom(-1500, 100, 1100, 750, true)), &[PRIMARY], TITLE);
        assert!(rect.x >= 0 && rect.x + rect.w <= PRIMARY.w, "{rect:?}");
        assert!(maximized);
    }

    #[test]
    fn window_geometry_ignored_when_minimized() {
        let stored = Some(geom(50, 60, 900, 700, false));
        let normal = ScreenRect::new(200, 150, 1100, 750);
        let maxed = ScreenRect::new(0, 30, 1920, 1020);

        // Minimized: keep the stored value (nothing to write).
        let minimized = CloseSnapshot {
            visible: false,
            maximized: false,
            current: ScreenRect::new(-32000, -32000, 160, 28),
            last_normal: None,
        };
        assert_eq!(geometry_to_save(&minimized, stored, false), None);

        // Maximized: last normal rect + maximized = true.
        let maximized = CloseSnapshot {
            visible: true,
            maximized: true,
            current: maxed,
            last_normal: Some(normal),
        };
        assert_eq!(
            geometry_to_save(&maximized, stored, false),
            Some(geom(200, 150, 1100, 750, true))
        );

        // Maximized with no normal rect seen: fall back to the stored one.
        let maximized_unknown = CloseSnapshot {
            last_normal: None,
            ..maximized
        };
        assert_eq!(
            geometry_to_save(&maximized_unknown, stored, false),
            Some(geom(50, 60, 900, 700, true))
        );

        // Normal: the current rect.
        let normal_close = CloseSnapshot {
            visible: true,
            maximized: false,
            current: normal,
            last_normal: None,
        };
        assert_eq!(
            geometry_to_save(&normal_close, stored, false),
            Some(geom(200, 150, 1100, 750, false))
        );
    }

    #[test]
    fn window_geometry_tracker_records_only_normal_rects() {
        let initial = ScreenRect::new(100, 100, 640, 480);
        let mut t = WindowGeometryTracker::new(None, false, initial);

        // Resize + move while normal.
        t.record_resize(true, false, ScreenRect::new(100, 100, 1100, 750));
        t.record_resize(true, false, ScreenRect::new(300, 200, 1100, 750));
        // Minimized and maximized rects are not normal.
        t.record_resize(false, false, ScreenRect::new(-32000, -32000, 160, 28));
        let maxed = ScreenRect::new(0, 30, 1920, 1020);
        t.record_resize(true, true, maxed);

        t.capture_close(true, true, maxed);
        assert_eq!(
            t.take_geometry_to_save(),
            Some(geom(300, 200, 1100, 750, true))
        );
        // Consumed: a second quit without a new capture writes nothing.
        assert_eq!(t.take_geometry_to_save(), None);
    }

    #[test]
    fn window_geometry_tracker_late_x11_maximize_flag() {
        // X11 may resize to the maximized size before the maximized flag is
        // set, so that resize gets recorded as "normal".
        let mut t = WindowGeometryTracker::new(None, false, ScreenRect::new(100, 100, 640, 480));
        t.record_resize(true, false, ScreenRect::new(250, 120, 1100, 750));
        let maxed = ScreenRect::new(0, 30, 1920, 1020);
        t.record_resize(true, false, ScreenRect::new(0, 0, 1920, 1020));
        t.record_resize(true, false, maxed); // a move at the maximized size

        t.capture_close(true, true, maxed);
        assert_eq!(
            t.take_geometry_to_save(),
            Some(geom(250, 120, 1100, 750, true))
        );
    }

    #[test]
    fn window_geometry_backend_detection() {
        assert!(!is_wayland_backend(Some("x11"), Some("wayland-0")));
        assert!(is_wayland_backend(None, Some("wayland-0")));
        assert!(is_wayland_backend(Some("wayland"), None));
        assert!(!is_wayland_backend(None, None));
        assert!(!is_wayland_backend(None, Some("")));

        // On Wayland the decision keeps the previously stored x/y.
        let stored = Some(geom(-1500, 80, 900, 700, false));
        let snapshot = CloseSnapshot {
            visible: true,
            maximized: false,
            current: ScreenRect::new(0, 0, 1100, 750),
            last_normal: None,
        };
        assert_eq!(
            geometry_to_save(&snapshot, stored, true),
            Some(geom(-1500, 80, 1100, 750, false))
        );
        // Nothing stored yet: the default position, never Wayland's 0,0.
        assert_eq!(
            geometry_to_save(&snapshot, None, true),
            Some(geom(DEFAULT_X, DEFAULT_Y, 1100, 750, false))
        );
    }
}
