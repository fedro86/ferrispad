use fltk::{
    app,
    enums::{Color, FrameType},
    frame::Frame,
    menu::MenuBar,
    prelude::*,
    text::TextEditor,
    window::Window,
};

use super::color::{ThemeRgb, wcag_luminance};
use super::dialogs::{SCROLLBAR_SIZE, scrollbar_colors};
use super::scrollbar::style_scrollbars;
use super::tab_bar::theme_colors_from_bg;

/// FLTK's CIELAB contrast threshold as FerrisPad uses it, lowered from
/// FLTK's default 39. The default overrides our custom tree label colors (git
/// status amber/green/red) when selected in light mode. Level 10 preserves
/// them while still protecting against truly unreadable combinations (e.g.
/// white-on-white). Applied through FerrisPad's own contrast function
/// (`ui::contrast`, installed in `main.rs`), which is stricter on the editor
/// selection only.
pub const FLTK_CONTRAST_LEVEL: i32 = 10;

/// Width of all draggable panel dividers (tree, split, terminal).
pub const DIVIDER_WIDTH: i32 = 2;

/// The theme inputs every panel derives its colors from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelTheme {
    /// The app's dark-mode flag.
    pub is_dark: bool,
    /// The syntax theme's editor background.
    pub bg: (u8, u8, u8),
}

/// A panel whose colors follow the app theme. One signature for every panel,
/// so a theme change can iterate them all (see `LayoutWidgets::themeable_panels`).
pub trait Themeable {
    /// Re-derive this panel's colors from `theme`.
    fn apply_theme(&mut self, theme: PanelTheme);

    /// Whether a theme change should reach this panel now. Panels that
    /// re-theme themselves when shown (and are costly to restyle) return
    /// `false` while hidden.
    fn wants_retheme(&self) -> bool {
        true
    }
}

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

/// WCAG luminance below which a background counts as dark: the point where
/// white and black text have the same contrast on it (sqrt(1.05 * 0.05) - 0.05).
const DARK_BG_LUMINANCE: f64 = 0.179;

/// Whether `bg` is a dark background, judged by its WCAG luminance alone
/// (never by the dark-mode flag: any syntax theme can run in either mode).
fn is_dark_background(bg: (u8, u8, u8)) -> bool {
    wcag_luminance(bg) < DARK_BG_LUMINANCE
}

/// Editor selection on a light background: a saturated blue, as in Windows
/// Notepad and web browsers.
const EDITOR_SELECTION_ON_LIGHT: (u8, u8, u8) = (0, 120, 215);
/// Editor selection on a dark background: a lighter blue, so it stands out
/// from the background without being glaring.
const EDITOR_SELECTION_ON_DARK: (u8, u8, u8) = (70, 140, 230);

/// The main editor's text-selection color for the editor background `bg`.
///
/// A clearly blue color whose variant follows the background's luminance.
/// On all seven built-in syntax themes it stays visible focused (>= 3:1) and
/// unfocused (FLTK blends 40 % background in, >= 2:1), and black or white
/// text stays readable on it (>= 4.5:1) — enforced by the tests below.
pub fn editor_selection_color(bg: (u8, u8, u8)) -> (u8, u8, u8) {
    if is_dark_background(bg) {
        EDITOR_SELECTION_ON_DARK
    } else {
        EDITOR_SELECTION_ON_LIGHT
    }
}

/// The main editor's colors, all as RGB: one value per editable field of a
/// custom theme (T0079) plus the background they are drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorColors {
    pub background: (u8, u8, u8),
    pub text: (u8, u8, u8),
    pub caret: (u8, u8, u8),
    pub selection: (u8, u8, u8),
    pub line_number_fg: (u8, u8, u8),
    pub line_number_bg: (u8, u8, u8),
    pub current_line: (u8, u8, u8),
}

/// Average-of-channels brightness test used by the editor-color derivation
/// (kept as it was before T0079, so built-in themes look unchanged).
fn is_dim((r, g, b): (u8, u8, u8)) -> bool {
    (u32::from(r) + u32::from(g) + u32::from(b)) / 3 < 128
}

/// The editor colors FerrisPad derives from a background and foreground.
/// This is the look of every built-in theme, and the fallback for any field
/// a custom theme leaves unset.
pub fn derived_editor_colors(background: (u8, u8, u8), foreground: (u8, u8, u8)) -> EditorColors {
    let bg = ThemeRgb::from_tuple(background);
    let selection = editor_selection_color(background);
    if is_dim(background) {
        EditorColors {
            background,
            // Foreground too dark for a dark background: light fallback.
            text: if is_dim(foreground) {
                (220, 220, 220)
            } else {
                foreground
            },
            caret: (255, 255, 255),
            selection,
            line_number_fg: (150, 150, 150),
            line_number_bg: (
                background.0.saturating_add(15),
                background.1.saturating_add(15),
                background.2.saturating_add(15),
            ),
            current_line: bg.lighten(0.07).to_tuple(),
        }
    } else {
        EditorColors {
            background,
            // Foreground too light for a light background: dark fallback.
            text: if is_dim(foreground) {
                foreground
            } else {
                (30, 30, 30)
            },
            caret: (0, 0, 0),
            selection,
            line_number_fg: (100, 100, 100),
            line_number_bg: (
                background.0.saturating_sub(15),
                background.1.saturating_sub(15),
                background.2.saturating_sub(15),
            ),
            current_line: bg.darken(0.95).to_tuple(),
        }
    }
}

/// A syntect color as RGB, composited over `under` by its alpha (TextMate
/// themes often give `selection` / `lineHighlight` a translucent alpha).
fn over(color: syntect::highlighting::Color, under: (u8, u8, u8)) -> (u8, u8, u8) {
    let a = u32::from(color.a);
    let mix = |c: u8, u: u8| ((u32::from(c) * a + u32::from(u) * (255 - a) + 127) / 255) as u8;
    (
        mix(color.r, under.0),
        mix(color.g, under.1),
        mix(color.b, under.2),
    )
}

/// The editor colors a syntect theme is shown with.
///
/// A built-in theme always gets [`derived_editor_colors`] of its background
/// and foreground, so built-ins keep their established look (and T0078's
/// selection, not syntect's faint one). A custom theme uses each editor field
/// of `theme.settings` it sets and derives the rest. A theme without a
/// background (common in imported themes) gets white, or near-black when its
/// foreground is light, so it never ends up black-on-black.
pub fn effective_editor_colors(
    theme: &syntect::highlighting::Theme,
    is_builtin: bool,
) -> EditorColors {
    let s = &theme.settings;
    let opaque = |c: syntect::highlighting::Color| (c.r, c.g, c.b);
    let fg = s.foreground.map(opaque);
    let background = match s.background.map(opaque) {
        Some(bg) => bg,
        None if fg.is_some_and(|fg| !is_dim(fg)) => (30, 30, 30),
        None => (255, 255, 255),
    };
    let foreground = fg.unwrap_or(if is_dim(background) {
        (220, 220, 220)
    } else {
        (30, 30, 30)
    });
    let derived = derived_editor_colors(background, foreground);
    if is_builtin {
        return derived;
    }
    let pick = |field: Option<syntect::highlighting::Color>, fallback: (u8, u8, u8)| {
        field.map_or(fallback, |c| over(c, background))
    };
    EditorColors {
        background,
        text: fg.unwrap_or(derived.text),
        caret: pick(s.caret, derived.caret),
        selection: pick(s.selection, derived.selection),
        line_number_fg: pick(s.gutter_foreground, derived.line_number_fg),
        line_number_bg: pick(s.gutter, derived.line_number_bg),
        current_line: pick(s.line_highlight, derived.current_line),
    }
}

/// Apply the editor colors of the active syntax theme to the main editor.
/// Every re-theme (startup, dark-mode toggle, Settings, live preview and its
/// revert on Cancel) ends here, after `apply_theme`.
pub fn apply_syntax_theme_colors(editor: &mut TextEditor, colors: &EditorColors) {
    let fltk = |c: (u8, u8, u8)| Color::from_rgb(c.0, c.1, c.2);
    let background = colors.background;
    editor.set_color(fltk(background));
    // Editor only — NOT FLTK's global Selection color, which is also used for
    // checkbox/radio marks.
    editor.set_selection_color(fltk(colors.selection));
    // Selected text must stay readable on it (see `ui::contrast`).
    super::contrast::register_editor_selection(background, colors.selection, colors.current_line);
    // ...and so must a selected row of the dialog lists of this theme (T0097).
    super::dialogs::register_dialog_list_selection(background);
    editor.set_text_color(fltk(colors.text));
    editor.set_cursor_color(fltk(colors.caret));
    editor.set_linenumber_bgcolor(fltk(colors.line_number_bg));
    editor.set_linenumber_fgcolor(fltk(colors.line_number_fg));
    let brightness = ThemeRgb::from_tuple(background).brightness();

    // Style both editor scrollbars to match the editor theme, with the same
    // track/thumb derivation the dialogs apply to their own background.
    let (track, thumb) = scrollbar_colors(ThemeRgb::from_tuple(background), brightness < 128);
    editor.set_scrollbar_size(SCROLLBAR_SIZE);
    style_scrollbars(&*editor, track, thumb);

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

/// Re-theme the main window, menu bar and update banner. The editor's own
/// colors are applied by [`apply_syntax_theme_colors`].
pub fn apply_theme(
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
        window.set_color(Color::from_rgb(25, 25, 25));
        window.set_label_color(Color::from_rgb(220, 220, 220));
        if let Some(b) = banner {
            b.set_color(Color::from_rgb(139, 128, 0)); // Darker yellow/olive
            b.set_label_color(Color::White);
        }
    } else {
        // Light mode colors
        window.set_color(Color::from_rgb(240, 240, 240));
        window.set_label_color(Color::Black);
        if let Some(b) = banner {
            b.set_color(Color::from_rgb(255, 250, 205)); // Lemon chiffon
            b.set_label_color(Color::Black);
        }
    }

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

/// FLTK foreground (`app::foreground`) for menu text: light on a dark menu
/// background, black on a light one. The single rule for startup and every
/// theme change (via `dispatch::retheme_panels`).
///
/// It deliberately ignores `theme.is_dark`: the text is drawn on the menu
/// background, and that is derived from the syntax theme background alone
/// (`menu_colors_from_bg`). Following the dark-mode flag instead would put
/// light text on a light menu whenever a light syntax theme is used in dark
/// mode (and black on a dark menu in the reverse case).
pub fn menu_foreground(theme: PanelTheme) -> (u8, u8, u8) {
    let menu_bg = ThemeRgb::from_fltk(menu_colors_from_bg(theme.bg).color);
    if menu_bg.brightness() < 128 {
        (230, 230, 230)
    } else {
        (0, 0, 0)
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

    /// Startup and every theme change (dark-mode toggle, Settings, live
    /// preview) pick the menu text colour through `menu_foreground`. For every
    /// (syntax theme, dark flag) pair the colour must not depend on the flag —
    /// so toggling dark mode twice cannot change it — and must be readable on
    /// the menu background it is drawn on.
    #[test]
    fn menu_foreground_is_the_same_at_startup_and_on_toggle() {
        use crate::app::domain::settings::SyntaxTheme;
        let mut failures = Vec::new();
        for &syntax in SyntaxTheme::all() {
            let bg = syntax.background();
            let menu_bg = ThemeRgb::from_fltk(menu_colors_from_bg(bg).color);
            let light_text = menu_bg.brightness() < 128;
            for is_dark in [false, true] {
                let fg = menu_foreground(PanelTheme { is_dark, bg });
                let toggled = menu_foreground(PanelTheme {
                    is_dark: !is_dark,
                    bg,
                });
                if fg != toggled {
                    failures.push(format!(
                        "{syntax:?} (dark mode {is_dark}): menu text {fg:?} becomes {toggled:?} when the flag flips"
                    ));
                }
                if (ThemeRgb::from_tuple(fg).brightness() >= 128) != light_text {
                    failures.push(format!(
                        "{syntax:?} (dark mode {is_dark}): menu text {fg:?} unreadable on menu background {:?}",
                        menu_bg.to_tuple()
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    // --- Editor selection (T0078) -------------------------------------------

    use crate::app::domain::settings::SyntaxTheme as Syntax;
    use crate::ui::color::wcag_contrast;

    /// Run `check` on every syntax theme background; collect the failures so
    /// one run names every theme that misses the bar, with its measured value.
    fn for_every_theme(check: impl Fn(Syntax, (u8, u8, u8), (u8, u8, u8)) -> Option<String>) {
        let failures: Vec<String> = Syntax::all()
            .iter()
            .filter_map(|&t| {
                let bg = t.background();
                check(t, bg, editor_selection_color(bg))
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// FLTK draws an unfocused `Fl_Text_Display` selection as
    /// `fl_color_average(color(), selection_color(), 0.4)`.
    fn fltk_unfocused_selection(bg: (u8, u8, u8), sel: (u8, u8, u8)) -> (u8, u8, u8) {
        use crate::ui::contrast::{UNFOCUSED_SELECTION_WEIGHT, fltk_color_average};
        fltk_color_average(bg, sel, UNFOCUSED_SELECTION_WEIGHT)
    }

    #[test]
    fn editor_selection_contrast_focused() {
        for_every_theme(|t, bg, sel| {
            let ratio = wcag_contrast(sel, bg);
            (ratio < 3.0).then(|| {
                format!("{t:?}: selection {sel:?} on background {bg:?} has contrast {ratio:.2}:1 (< 3.0)")
            })
        });
    }

    #[test]
    fn editor_selection_contrast_unfocused() {
        for_every_theme(|t, bg, sel| {
            let blended = fltk_unfocused_selection(bg, sel);
            let ratio = wcag_contrast(blended, bg);
            (ratio < 2.0).then(|| {
                format!("{t:?}: unfocused selection {blended:?} on background {bg:?} has contrast {ratio:.2}:1 (< 2.0)")
            })
        });
    }

    /// Every text color the editor can draw for `theme`: its foreground and
    /// each syntax-scope foreground of the syntect theme.
    fn editor_text_colors(theme: Syntax) -> Vec<(u8, u8, u8)> {
        let set = syntect::highlighting::ThemeSet::load_defaults();
        let mut colors = vec![theme.foreground()];
        if let Some(t) = set.themes.get(theme.theme_key()) {
            colors.extend(
                t.scopes
                    .iter()
                    .filter_map(|item| item.style.foreground)
                    .map(|c| (c.r, c.g, c.b)),
            );
        }
        colors.sort_unstable();
        colors.dedup();
        colors
    }

    /// The text color FLTK draws for selected text `fg` on selection `bg`:
    /// `Fl_Text_Display` passes both through `fl_contrast`, which FerrisPad
    /// routes to `ferrispad_contrast` with the editor's selection registered.
    fn drawn_selected_text(
        fg: (u8, u8, u8),
        bg: (u8, u8, u8),
        editor_bg: (u8, u8, u8),
    ) -> (u8, u8, u8) {
        use crate::ui::contrast::{EditorSelectionBgs, ferrispad_contrast};
        let c = |(r, g, b): (u8, u8, u8)| Color::from_rgb(r, g, b);
        let registered = EditorSelectionBgs::new(editor_bg, editor_selection_color(editor_bg));
        ferrispad_contrast(c(fg), c(bg), registered).to_rgb()
    }

    /// Selected text, as FLTK really draws it, is readable (>= 4.5:1) on the
    /// focused selection and on the unfocused blend, for the theme foreground
    /// and every syntax color of all seven themes.
    #[test]
    fn editor_selection_text_readable() {
        for_every_theme(|t, bg, sel| {
            let unfocused = fltk_unfocused_selection(bg, sel);
            let mut failures = Vec::new();
            for fg in editor_text_colors(t) {
                for (what, on) in [("focused", sel), ("unfocused", unfocused)] {
                    let drawn = drawn_selected_text(fg, on, bg);
                    let ratio = wcag_contrast(drawn, on);
                    if ratio < 4.5 {
                        failures.push(format!(
                            "{t:?}: text {fg:?} drawn as {drawn:?} on {what} selection {on:?}: {ratio:.2}:1 (< 4.5)"
                        ));
                    }
                }
            }
            (!failures.is_empty()).then(|| failures.join("\n"))
        });
    }

    #[test]
    fn editor_selection_is_blue() {
        for_every_theme(|t, _bg, (r, g, b)| {
            let blue = b > r && b > g && i32::from(b) - i32::from(r) >= 60;
            (!blue).then(|| format!("{t:?}: selection {:?} is not clearly blue", (r, g, b)))
        });
    }

    #[test]
    fn editor_selection_follows_luminance_not_mode() {
        let dark_variant = editor_selection_color(Syntax::Base16OceanDark.background());
        let light_variant = editor_selection_color(Syntax::Base16OceanLight.background());
        assert_ne!(dark_variant, light_variant, "there must be two variants");
        // A dark syntax theme (usable in light mode) gets the dark variant...
        assert_eq!(
            editor_selection_color(Syntax::SolarizedDark.background()),
            dark_variant
        );
        // ...and a light one (usable in dark mode) gets the light variant.
        assert_eq!(
            editor_selection_color(Syntax::InspiredGitHub.background()),
            light_variant
        );
    }

    /// T0081 edge case 11, on T0079's `editor_colors().current_line` (the
    /// derived tint for built-ins): the band is visible on the background
    /// (>= 1.05:1) and weaker than the selection, and it costs the default
    /// text at most 20% of its contrast on the plain background, never having
    /// to beat 4.5:1 (`text/band >= min(4.5, 0.8 * text/bg)`).
    #[test]
    fn current_line_color_sane_on_all_themes() {
        use crate::app::controllers::highlight::HighlightController;
        use crate::app::domain::settings::{SyntaxTheme, ThemeRef};
        use crate::ui::color::wcag_contrast;

        assert_eq!(SyntaxTheme::all().len(), 7);
        let mut failures = Vec::new();
        for theme in SyntaxTheme::all() {
            let mut controller =
                HighlightController::new(*theme, fltk::enums::Font::Courier, 14, true);
            let is_dark = ThemeRgb::from_tuple(theme.background()).brightness() < 128;
            controller.set_theme(&ThemeRef::Builtin(*theme), is_dark);
            let c = controller.editor_colors();
            let line = wcag_contrast(c.current_line, c.background);
            let selection = wcag_contrast(c.selection, c.background);
            let text = wcag_contrast(c.text, c.current_line);
            let text_on_bg = wcag_contrast(c.text, c.background);
            let summary = format!(
                "{theme:?}: bg {:?} band {:?} text {:?} | band/bg {line:.3} sel/bg {selection:.2} text/band {text:.2} text/bg {text_on_bg:.2}",
                c.background, c.current_line, c.text
            );
            if line < 1.05 || line >= selection || text < (0.8 * text_on_bg).min(4.5) {
                failures.push(summary);
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
