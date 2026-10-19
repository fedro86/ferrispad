use fltk::{
    app,
    enums::{Color, FrameType},
    frame::Frame,
    menu::MenuBar,
    prelude::*,
    text::TextEditor,
    window::Window,
};

use super::color::ThemeRgb;
use super::dialogs::SCROLLBAR_SIZE;
use super::tab_bar::theme_colors_from_bg;

/// Width of all draggable panel dividers (tree, split, terminal).
pub const DIVIDER_WIDTH: i32 = 2;

/// Derive divider color from the syntax theme background.
/// It *is* the tab bar background, so dividers and the tab bar never drift apart.
pub fn divider_color_from_bg(theme_bg: (u8, u8, u8)) -> Color {
    theme_colors_from_bg(&ThemeRgb::from_tuple(theme_bg)).bar_bg
}

/// Per-channel offset that tints the diff tab (and the split-view chrome it
/// connects to) toward blue: added to the editor background on dark themes.
const DIFF_TAB_TINT_DARK: (u8, u8, u8) = (8, 4, 15);
/// Per-channel offset subtracted from the editor background on light themes.
const DIFF_TAB_TINT_LIGHT: (u8, u8, u8) = (2, 2, 0);

/// The diff tab's tinted background, shared by the tab bar's active diff tab
/// and the split panel's header / labels / action bar so they read as one.
pub fn diff_tab_tint_from_bg(theme_bg: (u8, u8, u8), is_dark: bool) -> Color {
    let (r, g, b) = theme_bg;
    if is_dark {
        let (dr, dg, db) = DIFF_TAB_TINT_DARK;
        Color::from_rgb(
            r.saturating_add(dr),
            g.saturating_add(dg),
            b.saturating_add(db),
        )
    } else {
        let (dr, dg, db) = DIFF_TAB_TINT_LIGHT;
        Color::from_rgb(
            r.saturating_sub(dr),
            g.saturating_sub(dg),
            b.saturating_sub(db),
        )
    }
}

/// Apply syntax theme colors (background/foreground) to the editor.
/// Used for live preview when changing syntax themes in settings.
pub fn apply_syntax_theme_colors(
    editor: &mut TextEditor,
    background: (u8, u8, u8),
    foreground: (u8, u8, u8),
) {
    editor.set_color(Color::from_rgb(background.0, background.1, background.2));

    // Adjust colors based on background brightness
    let brightness = (background.0 as u32 + background.1 as u32 + background.2 as u32) / 3;
    if brightness < 128 {
        // Dark background: use light text color (override if foreground is too dark)
        let fg_brightness = (foreground.0 as u32 + foreground.1 as u32 + foreground.2 as u32) / 3;
        let text_color = if fg_brightness < 128 {
            // Foreground is dark on dark background - use light fallback
            Color::from_rgb(220, 220, 220)
        } else {
            Color::from_rgb(foreground.0, foreground.1, foreground.2)
        };
        editor.set_text_color(text_color);
        editor.set_cursor_color(Color::from_rgb(255, 255, 255));
        editor.set_linenumber_bgcolor(Color::from_rgb(
            background.0.saturating_add(15),
            background.1.saturating_add(15),
            background.2.saturating_add(15),
        ));
        editor.set_linenumber_fgcolor(Color::from_rgb(150, 150, 150));
    } else {
        // Light background: use dark text color (override if foreground is too light)
        let fg_brightness = (foreground.0 as u32 + foreground.1 as u32 + foreground.2 as u32) / 3;
        let text_color = if fg_brightness >= 128 {
            // Foreground is light on light background - use dark fallback
            Color::from_rgb(30, 30, 30)
        } else {
            Color::from_rgb(foreground.0, foreground.1, foreground.2)
        };
        editor.set_text_color(text_color);
        editor.set_cursor_color(Color::from_rgb(0, 0, 0));
        editor.set_linenumber_bgcolor(Color::from_rgb(
            background.0.saturating_sub(15),
            background.1.saturating_sub(15),
            background.2.saturating_sub(15),
        ));
        editor.set_linenumber_fgcolor(Color::from_rgb(100, 100, 100));
    }

    // Style the vertical scrollbar to match the editor theme.
    // TextEditor doesn't expose scrollbar() directly, so we access
    // the child widgets via Fl_Group FFI (Fl_Text_Display inherits Fl_Group).
    // Child 0 = vertical scrollbar, child 1 = horizontal scrollbar.
    let bg_rgb = ThemeRgb::from_tuple(background);
    let (track, thumb) = if brightness < 128 {
        // Dark: track darker than bg, thumb lighter
        (
            bg_rgb.darken(0.70).to_fltk(),
            bg_rgb.lighten(0.20).to_fltk(),
        )
    } else {
        // Light: subtle darkening
        (bg_rgb.darken(0.95).to_fltk(), bg_rgb.darken(0.75).to_fltk())
    };

    editor.set_scrollbar_size(SCROLLBAR_SIZE);
    // SAFETY: TextEditor's underlying Fl_Text_Display inherits from Fl_Group.
    // The scrollbars are child widgets: child 0 = horizontal, child 1 = vertical.
    // The editor widget pointer is valid (we have a mutable reference).
    unsafe extern "C" {
        fn Fl_Group_children(grp: *mut std::ffi::c_void) -> std::ffi::c_int;
        fn Fl_Group_child(
            grp: *mut std::ffi::c_void,
            index: std::ffi::c_int,
        ) -> *mut std::ffi::c_void;
    }
    unsafe {
        use fltk::valuator::Scrollbar;
        let group_ptr = editor.as_widget_ptr() as *mut std::ffi::c_void;
        let nchildren = Fl_Group_children(group_ptr);
        // Style both scrollbars: child 0 = horizontal, child 1 = vertical
        for i in 0..nchildren.min(2) {
            let ptr = Fl_Group_child(group_ptr, i);
            if !ptr.is_null() {
                let mut sb = Scrollbar::from_widget_ptr(ptr as fltk::app::WidgetPtr);
                sb.set_frame(FrameType::FlatBox);
                sb.set_color(track);
                sb.set_slider_frame(FrameType::FlatBox);
                sb.set_selection_color(thumb);
            }
        }
    }

    // Override FL_BACKGROUND_COLOR so the corner square between scrollbars
    // uses the themed track color instead of FLTK's default gray.
    let bits = track.bits();
    app::background(
        ((bits >> 24) & 0xFF) as u8,
        ((bits >> 16) & 0xFF) as u8,
        ((bits >> 8) & 0xFF) as u8,
    );

    editor.redraw();
}

pub fn apply_theme(
    editor: &mut TextEditor,
    window: &mut Window,
    menu: &mut MenuBar,
    banner: Option<&mut Frame>,
    is_dark: bool,
    theme_bg: (u8, u8, u8),
) {
    // Make menu bar flat, dropdown menus with subtle border
    menu.set_frame(FrameType::FlatBox);
    menu.set_down_frame(FrameType::FlatBox);
    // Use app's default font size (typically 14pt) for menu readability
    menu.set_text_size(fltk::app::font_size());

    // Menu colors derived from syntax theme (shared with context menus)
    let mc = menu_colors_from_bg(theme_bg);
    menu.set_color(mc.color);
    menu.set_text_color(mc.text_color);
    menu.set_selection_color(mc.selection_color);

    // Derive editor selection color from syntax theme background (same as tree panel).
    // Applied only to the editor — NOT globally, because FLTK's global Selection
    // color is also used for checkbox/radio marks where the subtle theme-derived
    // color would be invisible on light backgrounds.
    let bg_rgb = ThemeRgb::from_tuple(theme_bg);
    let sel_color = if is_dark {
        bg_rgb.lighten(0.10)
    } else {
        bg_rgb.darken(0.90)
    }
    .to_fltk();

    if is_dark {
        // Dark mode colors
        editor.set_color(Color::from_rgb(30, 30, 30));
        editor.set_text_color(Color::from_rgb(220, 220, 220));
        editor.set_cursor_color(Color::from_rgb(255, 255, 255));
        editor.set_selection_color(sel_color);
        editor.set_linenumber_bgcolor(Color::from_rgb(40, 40, 40));
        editor.set_linenumber_fgcolor(Color::from_rgb(150, 150, 150));
        window.set_color(Color::from_rgb(25, 25, 25));
        window.set_label_color(Color::from_rgb(220, 220, 220));
        if let Some(b) = banner {
            b.set_color(Color::from_rgb(139, 128, 0)); // Darker yellow/olive
            b.set_label_color(Color::White);
        }
    } else {
        // Light mode colors
        editor.set_color(Color::White);
        editor.set_text_color(Color::Black);
        editor.set_cursor_color(Color::Black);
        editor.set_selection_color(sel_color);
        editor.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240));
        editor.set_linenumber_fgcolor(Color::from_rgb(100, 100, 100));
        window.set_color(Color::from_rgb(240, 240, 240));
        window.set_label_color(Color::Black);
        if let Some(b) = banner {
            b.set_color(Color::from_rgb(255, 250, 205)); // Lemon chiffon
            b.set_label_color(Color::Black);
        }
    }

    editor.redraw();
    window.redraw();
    menu.redraw();
}

/// Colors for styling any menu widget (MenuBar or MenuButton) to match the app theme.
pub(crate) struct MenuColors {
    pub color: Color,           // Background color
    pub text_color: Color,      // Text color
    pub selection_color: Color, // Hover/selection color
}

/// Compute menu colors from the syntax theme background.
/// Reused by: main menu bar, tab bar context menu, tree panel context menu.
pub(crate) fn menu_colors_from_bg(theme_bg: (u8, u8, u8)) -> MenuColors {
    let theme_rgb = ThemeRgb::from_tuple(theme_bg);
    let tab_colors = theme_colors_from_bg(&theme_rgb);
    let base = ThemeRgb::from_fltk(tab_colors.active_bg)
        .blend(&ThemeRgb::from_fltk(tab_colors.inactive_bg), 0.5);
    let color = if theme_rgb.brightness() < 128 {
        base.blend(&ThemeRgb::WHITE, 0.15)
    } else {
        base.blend(&ThemeRgb::new(120, 120, 120), 0.15)
    }
    .to_fltk();
    MenuColors {
        color,
        text_color: tab_colors.active_text,
        selection_color: tab_colors.inactive_bg,
    }
}

/// NSRect — written-only (passed to setFrame:, never read back).
/// Avoids the x86_64 `objc_msgSend_stret` requirement for NSRect returns.
#[cfg(target_os = "macos")]
#[repr(C)]
struct NSRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[cfg(target_os = "macos")]
unsafe impl objc2::encode::Encode for NSRect {
    const ENCODING: objc2::encode::Encoding = objc2::encode::Encoding::Struct(
        "CGRect",
        &[
            objc2::encode::Encoding::Struct(
                "CGPoint",
                &[
                    objc2::encode::Encoding::Double,
                    objc2::encode::Encoding::Double,
                ],
            ),
            objc2::encode::Encoding::Struct(
                "CGSize",
                &[
                    objc2::encode::Encoding::Double,
                    objc2::encode::Encoding::Double,
                ],
            ),
        ],
    );
}

/// Tag used to identify our custom title NSTextField across calls.
/// Arbitrary value unlikely to clash with any FLTK or AppKit tag.
#[cfg(target_os = "macos")]
const CUSTOM_TITLE_TAG: isize = 0x4670; // "Fp"

/// Set macOS title bar appearance to match the syntax theme:
///   - Background color = tab bar background (via `setTitlebarAppearsTransparent`)
///   - Title text: custom NSTextField, centered, regular weight, theme foreground color
///
/// The native NSTextField managed by AppKit uses Auto Layout constraints that prevent
/// `setFrame:` and `setAlignment:` from taking effect. The reliable approach is to:
///   1. Hide the native title with `setTitleVisibility: NSWindowTitleHidden`
///   2. Add a custom `[NSTextField labelWithString:]` to the title bar view — this has
///      no Auto Layout constraints, so frame and alignment are fully controllable.
///
/// The custom label is tagged with `CUSTOM_TITLE_TAG` and replaced on each call
/// (e.g. on theme changes) to avoid duplicates.
///
/// Navigation to the title bar view uses the close button as a reliable anchor:
///   `standardWindowButton(NSWindowCloseButton).superview.superview`
/// gives the view that contains both the traffic lights group and the title area.
///
/// # Important: FLTK Widget Lifecycle
///
/// Must be called AFTER `window.show()`. `raw_handle()` panics on an unshown window.
#[cfg(target_os = "macos")]
pub fn set_macos_titlebar_color(window: &Window, theme_bg: (u8, u8, u8), theme_fg: (u8, u8, u8)) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    if !window.shown() {
        return;
    }

    // --- Background color: tab bar bar_bg blended 30% toward white (slightly lighter) ---
    let rgb = ThemeRgb::from_tuple(theme_bg);
    let tc = theme_colors_from_bg(&rgb);
    let title_bg = ThemeRgb::from_fltk(tc.bar_bg).blend(&ThemeRgb::WHITE, 0.1);
    let r_f: f64 = title_bg.r as f64 / 255.0;
    let g_f: f64 = title_bg.g as f64 / 255.0;
    let b_f: f64 = title_bg.b as f64 / 255.0;

    // --- Foreground color for the title text ---
    let fg_r: f64 = theme_fg.0 as f64 / 255.0;
    let fg_g: f64 = theme_fg.1 as f64 / 255.0;
    let fg_b: f64 = theme_fg.2 as f64 / 255.0;

    let window_w = window.width() as f64;

    // SAFETY: raw_handle() returns a valid NSWindow* after window.show().
    // NSColor, NSFont, NSTextField, and NSWindow are AppKit classes present on all
    // supported macOS versions. Errors here are non-fatal (cosmetic feature only).
    unsafe {
        let ns_window: *mut AnyObject = window.raw_handle().cast();

        // Step 1: remove the title bar vibrancy layer so backgroundColor shows through.
        let _: () = msg_send![ns_window, setTitlebarAppearsTransparent: true];

        // Step 2: set the background color (now visible in the title bar area).
        let bg_color: *mut AnyObject = msg_send![
            class!(NSColor),
            colorWithRed: r_f green: g_f blue: b_f alpha: 1.0_f64
        ];
        let _: () = msg_send![ns_window, setBackgroundColor: bg_color];

        // Step 3: hide the native title — its NSTextField uses Auto Layout constraints
        // that prevent setFrame:/setAlignment: from taking effect.
        // NSWindowTitleHidden = 1
        let _: () = msg_send![ns_window, setTitleVisibility: 1_isize];

        // Step 4: navigate to the title bar container via the close button.
        // close → NSWindowButtonGroup → NSTitlebarView (macOS 12+)
        let close: *mut AnyObject = msg_send![ns_window, standardWindowButton: 0_usize];
        if close.is_null() {
            return;
        }
        let btn_group: *mut AnyObject = msg_send![close, superview];
        let titlebar_view: *mut AnyObject = msg_send![btn_group, superview];

        // Step 5: remove any previously added custom title label to avoid duplicates.
        let existing: *mut AnyObject = msg_send![titlebar_view, viewWithTag: CUSTOM_TITLE_TAG];
        if !existing.is_null() {
            let _: () = msg_send![existing, removeFromSuperview];
        }

        // Step 6: build the custom title label.
        // [NSTextField labelWithString:] creates a non-editable, non-selectable,
        // transparent label with no Auto Layout constraints — fully frame-controllable.
        let win_title: *mut AnyObject = msg_send![ns_window, title];
        let label: *mut AnyObject = msg_send![class!(NSTextField), labelWithString: win_title];

        let text_color: *mut AnyObject = msg_send![
            class!(NSColor),
            colorWithRed: fg_r green: fg_g blue: fg_b alpha: 1.0_f64
        ];
        // NSFontWeightRegular = 0.0
        let font: *mut AnyObject =
            msg_send![class!(NSFont), systemFontOfSize: 13.0_f64 weight: 0.0_f64];

        let _: () = msg_send![label, setTextColor: text_color];
        let _: () = msg_send![label, setFont: font];
        // NSTextAlignmentCenter = 1 on macOS 15+ (unified with iOS: Left=0, Center=1, Right=2)
        let _: () = msg_send![label, setAlignment: 1_isize];
        let _: () = msg_send![label, setTag: CUSTOM_TITLE_TAG];

        // Center the label vertically within the 28 pt title bar.
        // y = 6, h = 16 leaves equal 6 pt margins top and bottom, regardless of
        // whether the parent view uses a flipped or non-flipped coordinate system.
        let frame = NSRect {
            x: 0.0,
            y: 6.0,
            w: window_w,
            h: 16.0,
        };
        let _: () = msg_send![label, setFrame: frame];

        // NSViewWidthSizable (2): stretch the label horizontally when the window resizes,
        // so the centered title stays centered without a fixed pixel width.
        let _: () = msg_send![label, setAutoresizingMask: 2_usize];

        // Step 7: insert the label BELOW the traffic lights group in z-order.
        // This ensures the close/minimize/maximize buttons remain on top and
        // receive mouse events before the label intercepts them.
        // NSWindowBelow = -1
        let _: () = msg_send![
            titlebar_view,
            addSubview: label
            positioned: -1_isize
            relativeTo: btn_group
        ];
    }
}

/// Update the text of the custom macOS title label without recreating it.
///
/// Called whenever the window title changes (file open, save, dirty state toggle)
/// so the custom NSTextField stays in sync with the FLTK window label.
#[cfg(target_os = "macos")]
pub fn update_macos_title_label(window: &Window) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    if !window.shown() {
        return;
    }

    unsafe {
        let ns_window: *mut AnyObject = window.raw_handle().cast();
        let close: *mut AnyObject = msg_send![ns_window, standardWindowButton: 0_usize];
        if close.is_null() {
            return;
        }
        let btn_group: *mut AnyObject = msg_send![close, superview];
        let titlebar_view: *mut AnyObject = msg_send![btn_group, superview];
        let label: *mut AnyObject = msg_send![titlebar_view, viewWithTag: CUSTOM_TITLE_TAG];
        if label.is_null() {
            return;
        }
        let win_title: *mut AnyObject = msg_send![ns_window, title];
        let _: () = msg_send![label, setStringValue: win_title];
    }
}

/// Set Windows title bar theme (Windows 10 build 1809+).
///
/// # Important: FLTK Widget Lifecycle
///
/// This function MUST be called AFTER `window.show()` because:
/// - `window.raw_handle()` returns the native HWND
/// - The HWND is only valid after the window is displayed
/// - Calling before show() returns an invalid handle, causing crashes
///
/// See `docs/temp/0.1.6/02_WINDOWS_TITLE_BAR_DEBUGGING_JOURNEY.md` for details.
#[cfg(target_os = "windows")]
pub fn set_windows_titlebar_theme(window: &Window, theme_bg: (u8, u8, u8), theme_fg: (u8, u8, u8)) {
    use std::mem::size_of;
    use std::ptr::from_ref;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};

    // Guard: raw_handle() panics if called before window.show().
    // During initial apply_settings(), the window isn't shown yet — skip silently.
    if !window.shown() {
        return;
    }

    // Compute dark mode from theme background brightness (same logic as tab_bar)
    let rgb = ThemeRgb::from_tuple(theme_bg);
    let is_dark = rgb.brightness() < 128;

    // Compute titlebar background: tab bar bar_bg blended 10% toward white (same as macOS)
    let tc = theme_colors_from_bg(&rgb);
    let title_bg = ThemeRgb::from_fltk(tc.bar_bg).blend(&ThemeRgb::WHITE, 0.1);

    // COLORREF format: 0x00BBGGRR
    let caption_color: u32 =
        (title_bg.r as u32) | ((title_bg.g as u32) << 8) | ((title_bg.b as u32) << 16);
    let text_color: u32 =
        (theme_fg.0 as u32) | ((theme_fg.1 as u32) << 8) | ((theme_fg.2 as u32) << 16);

    // SAFETY: We call Windows DWM API to set title bar attributes.
    // Preconditions:
    //   - window.show() has been called (HWND is valid)
    //   - window has not been destroyed
    // The DwmSetWindowAttribute call is safe:
    //   - Invalid HWNDs cause the call to fail silently (returns error code)
    //   - Invalid attribute IDs are ignored by Windows
    //   - We try both attribute 19 and 20 for version compatibility
    // Results are ignored because older Windows versions may not support these.
    unsafe {
        let hwnd = HWND(window.raw_handle());

        let on: i32 = if is_dark { 1 } else { 0 };

        // Try attribute 20 (Windows 11 / Windows 10 2004+)
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(20), // DWMWA_USE_IMMERSIVE_DARK_MODE
            from_ref(&on).cast(),
            size_of::<i32>() as u32,
        );

        // Also try attribute 19 (Windows 10 1809-1903)
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(19),
            from_ref(&on).cast(),
            size_of::<i32>() as u32,
        );

        // DWMWA_CAPTION_COLOR (35) — custom titlebar background (Windows 11 build 22000+)
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(35),
            from_ref(&caption_color).cast(),
            size_of::<u32>() as u32,
        );

        // DWMWA_TEXT_COLOR (36) — custom title text color (Windows 11 build 22000+)
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(36),
            from_ref(&text_color).cast(),
            size_of::<u32>() as u32,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider_matches_tab_bar_background() {
        for bg in [(40, 44, 52), (30, 30, 30), (253, 246, 227), (255, 255, 255)] {
            let bar_bg = theme_colors_from_bg(&ThemeRgb::from_tuple(bg)).bar_bg;
            assert_eq!(divider_color_from_bg(bg).to_rgb(), bar_bg.to_rgb());
        }
    }

    #[test]
    fn diff_tab_tint_offsets_and_saturates() {
        assert_eq!(
            diff_tab_tint_from_bg((40, 44, 52), true).to_rgb(),
            (48, 48, 67)
        );
        assert_eq!(
            diff_tab_tint_from_bg((250, 253, 245), true).to_rgb(),
            (255, 255, 255)
        );
        assert_eq!(
            diff_tab_tint_from_bg((253, 246, 227), false).to_rgb(),
            (251, 244, 227)
        );
        assert_eq!(diff_tab_tint_from_bg((1, 0, 9), false).to_rgb(), (0, 0, 9));
    }
}
