use fltk::{
    app,
    enums::{Color, FrameType},
    frame::Frame,
    menu::MenuBar,
    prelude::*,
    text::TextEditor,
    window::Window,
};

use super::dialogs::{SCROLLBAR_SIZE, darken, lighten};
use super::tab_bar::{ThemeRgb, theme_colors_from_bg};

/// Width of all draggable panel dividers (tree, split, terminal).
pub const DIVIDER_WIDTH: i32 = 2;

/// Derive divider color from the syntax theme background.
/// Uses the same darkening as the tab bar background for visual consistency.
pub fn divider_color_from_bg(theme_bg: (u8, u8, u8)) -> Color {
    let rgb = ThemeRgb::from_tuple(theme_bg);
    let bar_bg = if rgb.brightness() < 128 {
        rgb.darken(0.65)
    } else {
        rgb.darken(0.85)
    };
    bar_bg.to_fltk()
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
    let (track, thumb) = if brightness < 128 {
        // Dark: track darker than bg, thumb lighter
        let (tr, tg, tb) = darken(background.0, background.1, background.2, 0.70);
        let (thr, thg, thb) = lighten(background.0, background.1, background.2, 0.20);
        (Color::from_rgb(tr, tg, tb), Color::from_rgb(thr, thg, thb))
    } else {
        // Light: subtle darkening
        let (tr, tg, tb) = darken(background.0, background.1, background.2, 0.95);
        let (thr, thg, thb) = darken(background.0, background.1, background.2, 0.75);
        (Color::from_rgb(tr, tg, tb), Color::from_rgb(thr, thg, thb))
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

    if is_dark {
        // Dark mode colors
        editor.set_color(Color::from_rgb(30, 30, 30));
        editor.set_text_color(Color::from_rgb(220, 220, 220));
        editor.set_cursor_color(Color::from_rgb(255, 255, 255));
        editor.set_selection_color(Color::from_rgb(70, 70, 100));
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
        editor.set_selection_color(Color::from_rgb(173, 216, 230));
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
    let base = blend_colors(tab_colors.active_bg, tab_colors.inactive_bg, 0.5);
    let is_dark = (theme_bg.0 as u32 + theme_bg.1 as u32 + theme_bg.2 as u32) / 3 < 128;
    let color = if is_dark {
        blend_colors(base, Color::from_rgb(255, 255, 255), 0.15)
    } else {
        blend_colors(base, Color::from_rgb(120, 120, 120), 0.15)
    };
    MenuColors {
        color,
        text_color: tab_colors.active_text,
        selection_color: tab_colors.inactive_bg,
    }
}

/// Blend two FLTK colors by a factor (0.0 = first color, 1.0 = second color)
pub(crate) fn blend_colors(c1: Color, c2: Color, factor: f32) -> Color {
    let (r1, g1, b1) = c1.to_rgb();
    let (r2, g2, b2) = c2.to_rgb();

    let r = (r1 as f32 + (r2 as f32 - r1 as f32) * factor) as u8;
    let g = (g1 as f32 + (g2 as f32 - g1 as f32) * factor) as u8;
    let b = (b1 as f32 + (b2 as f32 - b1 as f32) * factor) as u8;

    Color::from_rgb(r, g, b)
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
pub fn set_windows_titlebar_theme(window: &Window, is_dark: bool) {
    use std::mem::size_of;
    use std::ptr::from_ref;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};

    // Guard: raw_handle() panics if called before window.show().
    // During initial apply_settings(), the window isn't shown yet — skip silently.
    if !window.shown() {
        return;
    }

    // SAFETY: We call Windows DWM API to set the title bar dark mode attribute.
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
    }
}
