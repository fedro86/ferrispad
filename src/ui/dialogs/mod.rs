pub mod about;
pub mod color_picker;
pub mod community_install;
pub mod find;
pub mod font_picker;
pub mod goto_line;
pub mod large_file;
pub mod plugin_config;
pub mod plugin_manager;
pub mod plugin_permissions;
pub mod plugin_settings;
pub mod readonly_viewer;
pub mod session_picker;
pub mod settings_dialog;
pub mod settings_layout;
pub mod shortcut_dialog;
pub mod theme_manager;
pub mod update;
pub mod widgets;

use crate::ui::color::fltk_rgb;
use fltk::{app, enums::Color, group::Scroll, prelude::*, window::Window};

use super::color::ThemeRgb;
use super::scrollbar::style_scrollbar;

/// Global scrollbar width used across all dialogs.
pub const SCROLLBAR_SIZE: i32 = 12;

/// Dialog background darken factor on dark themes. Lighter than the tab bar's
/// (`0.65`) on purpose: dialogs stay close to the editor background (cdf5750).
pub const DIALOG_BG_DARKEN_DARK: f32 = 0.85;
/// Dialog background darken factor on light themes (tab bar uses `0.85`).
pub const DIALOG_BG_DARKEN_LIGHT: f32 = 0.93;

/// Theme colors for dialogs, derived from the syntax theme background.
///
/// The dialog background is deliberately *closer* to the editor background
/// than the tab bar's (see [`DIALOG_BG_DARKEN_DARK`]); every other dialog
/// shade is derived from that background or from the editor background.
#[derive(Clone, Copy)]
pub struct DialogTheme {
    /// Dialog window background (a slightly muted editor background)
    pub bg: Color,
    /// Primary text color
    pub text: Color,
    /// Dimmed/secondary text color
    pub text_dim: Color,
    /// Input field background (slightly different from dialog bg)
    pub input_bg: Color,
    /// Button/active tab background (contrasts with dialog bg)
    pub button_bg: Color,
    /// Tab active/selected background (slightly lighter than button_bg)
    pub tab_active_bg: Color,
    /// Secondary (Cancel/Close) button background: a subtle step off `bg`
    pub secondary_button_bg: Color,
    /// Row background (for lists/tables)
    pub row_bg: Color,
    /// Accent color — complementary hue from bg, for links/highlights
    pub accent: Color,
    /// Accent hover — slightly adjusted accent for hover state
    pub accent_hover: Color,
    /// Selected row of a dialog list (T0097): >= 3:1 against `input_bg`
    pub list_selection_bg: Color,
    /// 1 px border of text fields and lists (T0105): >= 3:1 against `bg`
    /// (WCAG 1.4.11, so an empty field stays easy to locate)
    pub input_border: Color,
    /// Text of the selected row: >= 4.5:1 on `list_selection_bg`
    pub list_selection_text: Color,
    /// Scrollbar track background
    pub scroll_track: Color,
    /// Scrollbar thumb/slider color
    pub scroll_thumb: Color,
    /// Whether the theme is dark mode
    is_dark: bool,
    /// Original syntax theme background (titlebar theming on Windows, and the
    /// themed popups opened from a dialog that only holds a `DialogTheme`)
    theme_bg: (u8, u8, u8),
}

/// Convert RGB (0-255) to HSL (h: 0-360, s: 0-1, l: 0-1).
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l); // achromatic
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - rf).abs() < 1e-6 {
        ((gf - bf) / d) + if gf < bf { 6.0 } else { 0.0 }
    } else if (max - gf).abs() < 1e-6 {
        ((bf - rf) / d) + 2.0
    } else {
        ((rf - gf) / d) + 4.0
    };
    (h * 60.0, s, l)
}

/// Convert HSL (h: 0-360, s: 0-1, l: 0-1) to RGB (0-255).
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s.abs() < 1e-6 {
        let v = (l * 255.0) as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let h_norm = h / 360.0;
    let r = hue_to_rgb(p, q, h_norm + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h_norm);
    let b = hue_to_rgb(p, q, h_norm - 1.0 / 3.0);
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

fn hue_to_rgb(p: f32, q: f32, t: f32) -> f32 {
    let t = if t < 0.0 {
        t + 1.0
    } else if t > 1.0 {
        t - 1.0
    } else {
        t
    };
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

/// Derive an accent color from a background using complementary hue.
/// Rotates hue 180°, boosts saturation, adjusts lightness for contrast.
fn accent_from_bg(r: u8, g: u8, b: u8, is_dark: bool) -> (u8, u8, u8) {
    let (h, s, _l) = rgb_to_hsl(r, g, b);
    // Complementary hue
    let accent_h = (h + 180.0) % 360.0;
    // Boost saturation (at least 0.35 for achromatic themes)
    let accent_s = (s.clamp(0.25, 0.55)) + 0.10;
    // Lightness: readable against bg
    let accent_l = if is_dark { 0.55 } else { 0.45 };
    hsl_to_rgb(accent_h, accent_s.min(0.65), accent_l)
}

/// Intensify the dominant channel of a color.
///
/// Warm themes (R > B): amplify R, reduce B → ochre/gold direction.
/// Cool themes (B > R): amplify B, reduce R → deep blue direction.
/// Neutral themes (R ≈ B): minimal effect.
///
/// Derived from: Solarized Light (253,246,227) → target ochre (204,119,34)
/// gives per-channel factors: R×0.81, G×0.48, B×0.15.
/// The dominant channel keeps ~0.81, mid channel ~0.48, suppressed ~0.15.
/// `amount` controls the blend: 0.0 = original, 1.0 = full intensification.
pub(crate) fn intensify_dominant(r: u8, g: u8, b: u8, amount: f32) -> (u8, u8, u8) {
    const KEEP: f32 = 0.81; // factor for dominant channel
    const MID: f32 = 0.48; // factor for middle channel
    const SUPPRESS: f32 = 0.15; // factor for weakest channel

    let rf = r as f32;
    let gf = g as f32;
    let bf = b as f32;

    // Determine warmth: positive = warm (R>B), negative = cool (B>R)
    let warmth = rf - bf;
    // Scale amount by how polarized the theme is (more polarized = stronger effect)
    let polarity = (warmth.abs() / 30.0).clamp(0.0, 1.0);
    let amt = amount * polarity;
    let inv = 1.0 - amt;

    if warmth >= 0.0 {
        // Warm: R=keep, G=mid, B=suppress
        (
            (rf * (inv + amt * KEEP)).clamp(0.0, 255.0) as u8,
            (gf * (inv + amt * MID)).clamp(0.0, 255.0) as u8,
            (bf * (inv + amt * SUPPRESS)).clamp(0.0, 255.0) as u8,
        )
    } else {
        // Cool: B=keep, G=mid, R=suppress
        (
            (rf * (inv + amt * SUPPRESS)).clamp(0.0, 255.0) as u8,
            (gf * (inv + amt * MID)).clamp(0.0, 255.0) as u8,
            (bf * (inv + amt * KEEP)).clamp(0.0, 255.0) as u8,
        )
    }
}

/// Scrollbar `(track, thumb)` colors for a surface with background `base`.
/// Dark: the track is darker than `base` and the thumb lighter. Light: both
/// are darker, the thumb more so. Shared by the dialogs (`DialogTheme`, on the
/// dialog background) and the main editor (on the editor background).
/// Register the dialog list selection of the theme with background
/// `theme_bg` with FerrisPad's contrast rule (T0097). Called on every
/// re-theme, so the registered color never lags behind a theme switch.
pub fn register_dialog_list_selection(theme_bg: (u8, u8, u8)) {
    let selection = DialogTheme::from_theme_bg(theme_bg).list_selection_bg;
    crate::ui::contrast::register_list_selection(selection.to_rgb());
}

/// Minimum WCAG contrast of a dialog list's selected row against the list
/// background (T0097).
const LIST_SELECTION_CONTRAST: f64 = 3.0;
/// Minimum WCAG contrast of a text field's border against the dialog
/// (WCAG 1.4.11, non-text contrast).
const INPUT_BORDER_CONTRAST: f64 = 3.0;
/// Minimum WCAG contrast of the selected row's text.
const LIST_TEXT_CONTRAST: f64 = 4.5;

/// The selected-row background and text of a dialog list (T0097). Starts from
/// `base` (the button color) and steps it away from `list_bg` until it reaches
/// [`LIST_SELECTION_CONTRAST`]: lighter in dark themes, darker in light ones,
/// and the other way when that direction cannot get there (a mid-tone list).
/// The text keeps `text` when it reads at [`LIST_TEXT_CONTRAST`] on the row,
/// otherwise it is black or white, whichever reads better; that is also what
/// FerrisPad's contrast rule makes FLTK draw there.
pub(crate) fn list_selection_colors(
    list_bg: ThemeRgb,
    base: ThemeRgb,
    text: ThemeRgb,
    is_dark: bool,
) -> (ThemeRgb, ThemeRgb) {
    use super::color::{push_to_contrast, wcag_contrast};
    let row = push_to_contrast(base, list_bg, LIST_SELECTION_CONTRAST, is_dark);
    let rgb = |c: ThemeRgb| (c.r, c.g, c.b);
    let text = if wcag_contrast(rgb(text), rgb(row)) >= LIST_TEXT_CONTRAST {
        text
    } else if wcag_contrast((0, 0, 0), rgb(row)) >= wcag_contrast((255, 255, 255), rgb(row)) {
        ThemeRgb::new(0, 0, 0)
    } else {
        ThemeRgb::WHITE
    };
    (row, text)
}

pub(crate) fn scrollbar_colors(base: ThemeRgb, is_dark: bool) -> (Color, Color) {
    let (track, thumb) = if is_dark {
        (base.darken(0.70), base.lighten(0.20))
    } else {
        (base.darken(0.95), base.darken(0.75))
    };
    (track.to_fltk(), thumb.to_fltk())
}

impl DialogTheme {
    /// Create theme colors based on the syntax theme background.
    ///
    /// Color derivation logic:
    /// - Dialog bg: the editor background darkened by
    ///   [`DIALOG_BG_DARKEN_DARK`] / [`DIALOG_BG_DARKEN_LIGHT`] — intentionally
    ///   lighter than the tab bar background, so dialogs stay close to the editor
    /// - Buttons/tabs: contrast with dialog bg (lighter if dark bg, darker if light bg)
    /// - Secondary buttons, inputs, rows, scrollbars: small steps off the dialog bg
    pub fn from_theme_bg(theme_bg: (u8, u8, u8)) -> Self {
        let (r, g, b) = theme_bg;
        let editor = ThemeRgb::from_tuple(theme_bg);
        let is_dark = editor.brightness() < 128;

        // Dialog background: slightly muted version of the editor background.
        // Both modes stay close to the editor bg for a cohesive look.
        let bg_rgb = if is_dark {
            editor.darken(DIALOG_BG_DARKEN_DARK)
        } else {
            editor.darken(DIALOG_BG_DARKEN_LIGHT)
        };
        let bg = bg_rgb.to_fltk();

        // Button background: contrast with dialog bg
        // Light bg → buttons darker (toward black)
        // Dark bg → buttons lighter (toward white)
        let btn_rgb = if is_dark {
            // Dark dialog: lighten buttons for contrast
            bg_rgb.lighten(0.15)
        } else {
            // Light dialog: intensify dominant channel from original theme bg.
            // For neutral themes (R ≈ B), fall back to simple darken.
            let (ir, ig, ib) = intensify_dominant(r, g, b, 0.75);
            let d = editor.darken(0.72);
            // Blend: use intensified if polarized, darkened if neutral
            let warmth = (r as f32 - b as f32).abs();
            let t = (warmth / 30.0).clamp(0.0, 1.0);
            ThemeRgb::new(
                (ir as f32 * t + d.r as f32 * (1.0 - t)) as u8,
                (ig as f32 * t + d.g as f32 * (1.0 - t)) as u8,
                (ib as f32 * t + d.b as f32 * (1.0 - t)) as u8,
            )
        };
        let button_bg = btn_rgb.to_fltk();

        // Tab active background: slightly lighter than button_bg for selected/active state
        let tab_active_bg = if is_dark {
            btn_rgb.lighten(0.10).to_fltk()
        } else {
            btn_rgb.lighten(0.15).to_fltk()
        };

        // Secondary (Cancel/Close) buttons: a subtle step off the dialog bg
        let secondary_button_bg = if is_dark {
            bg_rgb.lighten(0.10).to_fltk()
        } else {
            bg_rgb.darken(0.90).to_fltk()
        };

        // Input background: closer to editor background (lighter than dialog)
        let input_bg = if is_dark {
            bg_rgb.lighten(0.25).to_fltk()
        } else {
            // Light mode: same as editor background
            editor.to_fltk()
        };

        // Row backgrounds for lists/cards
        let row_bg = if is_dark {
            bg_rgb.lighten(0.10).to_fltk()
        } else {
            // Light: subtle intensification for cards, darken fallback for neutral
            let (ir, ig, ib) = intensify_dominant(r, g, b, 0.15);
            let d = editor.darken(0.95);
            let warmth = (r as f32 - b as f32).abs();
            let t = (warmth / 30.0).clamp(0.0, 1.0);
            let rr = (ir as f32 * t + d.r as f32 * (1.0 - t)) as u8;
            let rg = (ig as f32 * t + d.g as f32 * (1.0 - t)) as u8;
            let rb = (ib as f32 * t + d.b as f32 * (1.0 - t)) as u8;
            fltk_rgb(rr, rg, rb)
        };

        // Text colors based on brightness
        let (text, text_dim) = if is_dark {
            (fltk_rgb(230, 230, 230), fltk_rgb(140, 140, 140))
        } else {
            (fltk_rgb(0, 0, 0), fltk_rgb(80, 80, 80))
        };

        let (scroll_track, scroll_thumb) = scrollbar_colors(bg_rgb, is_dark);

        let (list_selection_bg, list_selection_text) = list_selection_colors(
            ThemeRgb::from_fltk(input_bg),
            btn_rgb,
            ThemeRgb::from_fltk(text),
            is_dark,
        );

        // Accent color: complementary hue from theme bg
        let accent_rgb = ThemeRgb::from_tuple(accent_from_bg(r, g, b, is_dark));
        let accent = accent_rgb.to_fltk();
        // Accent hover: slightly lighter/darker
        let accent_hover = if is_dark {
            accent_rgb.lighten(0.15).to_fltk()
        } else {
            accent_rgb.darken(0.85).to_fltk()
        };

        Self {
            bg,
            text,
            text_dim,
            input_bg,
            button_bg,
            tab_active_bg,
            secondary_button_bg,
            row_bg,
            accent,
            accent_hover,
            list_selection_bg: list_selection_bg.to_fltk(),
            list_selection_text: list_selection_text.to_fltk(),
            input_border: super::color::push_to_contrast(
                btn_rgb,
                bg_rgb,
                INPUT_BORDER_CONTRAST,
                is_dark,
            )
            .to_fltk(),
            scroll_track,
            scroll_thumb,
            is_dark,
            theme_bg,
        }
    }

    /// Background and text color of the selected row in a dialog list.
    pub fn list_selection(&self) -> (Color, Color) {
        (self.list_selection_bg, self.list_selection_text)
    }

    /// The syntax theme background this theme was derived from — what the
    /// themed popups ([`show_themed_message`] & co.) take.
    pub fn theme_bg(&self) -> (u8, u8, u8) {
        self.theme_bg
    }

    /// Check if the theme is dark mode
    pub fn is_dark(&self) -> bool {
        self.is_dark
    }

    /// Text color as RGB tuple (for FLTK browser @C format codes).
    pub fn text_rgb(&self) -> (u8, u8, u8) {
        if self.is_dark {
            (230, 230, 230)
        } else {
            (0, 0, 0)
        }
    }

    /// Error/warning text color adapted to the theme brightness.
    /// Light red on dark backgrounds for readability, dark red on light backgrounds.
    pub fn error_color(&self) -> Color {
        if self.is_dark {
            fltk_rgb(255, 120, 120)
        } else {
            fltk_rgb(180, 0, 0)
        }
    }

    /// Apply themed titlebar (icon + colors) to a dialog window.
    /// Must be called AFTER `window.show()`.
    pub fn apply_titlebar(&self, window: &fltk::window::Window) {
        // Set the FerrisPad icon on Windows and Linux (macOS uses the .app bundle icon)
        #[cfg(not(target_os = "macos"))]
        {
            let icon_data = include_bytes!("../../../icons/hicolor/32x32/apps/ferrispad.png");
            if let Ok(icon) = fltk::image::PngImage::from_data(icon_data) {
                window.clone().set_icon(Some(icon));
            }
        }
        // Set themed titlebar colors on Windows
        #[cfg(target_os = "windows")]
        {
            let fg = if self.is_dark {
                (230u8, 230u8, 230u8)
            } else {
                (0u8, 0u8, 0u8)
            };
            crate::ui::theme::set_windows_titlebar_theme(window, self.theme_bg, fg);
        }
        // macOS: the .app bundle provides the icon and the system draws the
        // dialog title bar, so there is nothing to apply.
        #[cfg(target_os = "macos")]
        let _ = window;
    }

    /// Apply flat themed scrollbar styling to a Scroll widget.
    pub fn style_scroll(&self, scroll: &mut Scroll) {
        scroll.set_scrollbar_size(SCROLLBAR_SIZE);
        for mut sb in [scroll.scrollbar(), scroll.hscrollbar()] {
            style_scrollbar(&mut sb, self.scroll_track, self.scroll_thumb);
        }
    }
}

/// Makes widgets built while it lives top-level: it clears FLTK's current
/// group on creation and restores the previous one when dropped.
///
/// FLTK adds every new widget, windows included, to the current group. A
/// group's `end()` makes its *parent* current, not the group that was current
/// before, so at runtime the current group can be some panel inside the main
/// window (the start page and the split view's action bar both leave it so).
/// A dialog window built there becomes a child window, clipped by its parent,
/// and a modal one then freezes the app (T0058). FLTK's own `fl_alert` guards
/// its dialogs the same way.
pub(crate) struct TopLevelScope {
    previous: Option<fltk::group::Group>,
}

impl TopLevelScope {
    pub(crate) fn new() -> Self {
        let previous = fltk::group::Group::try_current();
        fltk::group::Group::set_current(None::<&fltk::group::Group>);
        Self { previous }
    }
}

impl Drop for TopLevelScope {
    fn drop(&mut self) {
        fltk::group::Group::set_current(self.previous.as_ref());
    }
}

/// Run a dialog's event loop, automatically closing the dialog if the app
/// is quitting (e.g. user clicks X on the main window while a dialog is open).
pub fn run_dialog(dialog: &Window) {
    run_dialog_with(dialog, || {});
}

/// [`run_dialog`], calling `after_event` after every event the loop handles.
/// A modal dialog blocks the main loop, so a `Message` it sends waits in the
/// channel until it closes: this hook is how a dialog applies live work (e.g.
/// a theme preview) to the caller's state while it is still open.
pub fn run_dialog_with(dialog: &Window, mut after_event: impl FnMut()) {
    while dialog.shown() {
        app::wait();
        if app::should_program_quit() {
            let mut d = dialog.clone();
            d.hide();
        }
        after_event();
    }
}

/// Button row of a themed popup, following FLTK's `dialog::choice2_default`
/// contract so call sites keep their meaning when they migrate: `b0` is the
/// rightmost button, `b1` the middle one and the Enter default, `b2` the
/// leftmost; an empty label means "no such button".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChoiceLayout<'a> {
    /// Buttons left to right, as `(returned index, label)`.
    pub buttons: Vec<(usize, &'a str)>,
    /// Returned index of the button that Enter triggers.
    pub default: usize,
}

/// Lay out the `b0`/`b1`/`b2` buttons of a themed popup (see [`ChoiceLayout`]).
/// Enter picks `b1` when present (FLTK's return button), else `b0`, else `b2`. With no
/// label at all the popup still gets a single "Close" button at index 0, so
/// it always has a button besides Escape and the window's close button.
pub(crate) fn choice_layout<'a>(b0: &'a str, b1: &'a str, b2: &'a str) -> ChoiceLayout<'a> {
    let b0 = if b0.is_empty() && b1.is_empty() && b2.is_empty() {
        "Close"
    } else {
        b0
    };
    let buttons: Vec<(usize, &str)> = [(2, b2), (1, b1), (0, b0)]
        .into_iter()
        .filter(|(_, label)| !label.is_empty())
        .collect();
    let default = [1, 0, 2]
        .into_iter()
        .find(|i| buttons.iter().any(|(index, _)| index == i))
        .unwrap_or(0);
    ChoiceLayout { buttons, default }
}

/// Width of a themed popup window.
const POPUP_WIDTH: i32 = 420;
/// Outer padding of a themed popup.
const POPUP_PADDING: i32 = 15;
/// Approximate characters that fit on one line of the message (13px font).
const POPUP_CHARS_PER_LINE: usize = 52;
/// Height of one message line.
const POPUP_LINE_HEIGHT: i32 = 18;
/// Message lines shown before the text is clipped (keeps the popup on screen).
const POPUP_MAX_LINES: usize = 24;
/// Height of the input field and of the button row.
const POPUP_ROW_HEIGHT: i32 = 30;
/// Vertical gap between the message, the input and the buttons.
const POPUP_SPACING: i32 = 10;

/// Number of display lines `message` takes once wrapped at `chars_per_line`
/// (at least one, so an empty message still gets a line).
pub(crate) fn wrapped_line_count(message: &str, chars_per_line: usize) -> usize {
    let per_line = chars_per_line.max(1);
    message
        .split('\n')
        .map(|line| line.chars().count().div_ceil(per_line).max(1))
        .sum::<usize>()
        .max(1)
}

/// Window height of a themed popup showing `message` (plus an input field
/// when `has_input`), sized so the text is not clipped up to
/// [`POPUP_MAX_LINES`] lines.
pub(crate) fn popup_height(message: &str, has_input: bool) -> i32 {
    let lines = wrapped_line_count(message, POPUP_CHARS_PER_LINE).min(POPUP_MAX_LINES);
    let message_h = (lines as i32 * POPUP_LINE_HEIGHT).max(2 * POPUP_LINE_HEIGHT);
    let input_h = if has_input {
        POPUP_ROW_HEIGHT + POPUP_SPACING
    } else {
        0
    };
    2 * POPUP_PADDING + message_h + POPUP_SPACING + input_h + POPUP_ROW_HEIGHT
}

/// Points added to a label's font size when measuring it for layout, so a
/// desktop font a little wider than the one measured still fits (the UI
/// tests check labels at +2 pt too).
pub(crate) const LABEL_SLACK_PT: i32 = 2;

/// Room a label gets on each side inside its widget.
pub(crate) const LABEL_PAD: i32 = 4;

/// The width a widget needs to show `label` inside it at label size `size`
/// (FLTK's default label font), measured with the fonts actually installed,
/// plus [`LABEL_PAD`] on each side and [`LABEL_SLACK_PT`]. Fixed pixel
/// widths tuned on one machine overflow on desktops with wider fonts.
pub(crate) fn label_width(label: &str, size: i32) -> i32 {
    // Measuring needs the display open; this opens it if it is not yet.
    let _ = app::screen_size();
    fltk::draw::set_font(fltk::enums::Font::Helvetica, size + LABEL_SLACK_PT);
    let (width, _) = fltk::draw::measure(label, false);
    width + 2 * LABEL_PAD
}

/// The widest [`label_width`] of `labels`.
pub(crate) fn max_label_width(labels: &[&str], size: i32) -> i32 {
    labels
        .iter()
        .map(|label| label_width(label, size))
        .max()
        .unwrap_or(0)
}

pub(crate) fn popup_button_width(label: &str) -> i32 {
    (label.chars().count() as i32 * 8 + 24).max(80)
}

/// The widgets of a built, not yet shown themed popup (see
/// [`build_themed_popup`]).
struct ThemedPopup {
    dialog: Window,
    input: Option<fltk::input::Input>,
    default_btn: Option<fltk::button::Button>,
    /// Index of the pressed button, set by the button callbacks.
    result: std::rc::Rc<std::cell::Cell<Option<usize>>>,
}

/// Build the window of the single themed popup behind [`show_themed_message`],
/// [`show_themed_choice`] and [`show_themed_input`]: a modal window with the
/// message, an optional prefilled input and a button row. It is not shown or
/// positioned yet (that needs a display), see [`run_themed_popup`].
fn build_themed_popup(
    theme: &DialogTheme,
    title: &str,
    message: &str,
    input_default: Option<&str>,
    layout: &ChoiceLayout<'_>,
) -> ThemedPopup {
    use fltk::{
        enums::{Align, Event, Key},
        frame::Frame,
        group::{Flex, FlexType},
    };
    use std::cell::Cell;
    use std::rc::Rc;

    let height = popup_height(message, input_default.is_some());

    // Restored when this returns, after `dialog.end()`.
    let _top_level = TopLevelScope::new();
    let mut dialog = Window::default()
        .with_size(POPUP_WIDTH, height)
        .with_label(title);
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    let mut column = Flex::new(
        POPUP_PADDING,
        POPUP_PADDING,
        POPUP_WIDTH - 2 * POPUP_PADDING,
        height - 2 * POPUP_PADDING,
        None,
    );
    column.set_type(FlexType::Column);
    column.set_spacing(POPUP_SPACING);

    let mut msg_frame = Frame::default();
    msg_frame.set_label(message);
    msg_frame.set_label_size(13);
    msg_frame.set_label_color(theme.text);
    msg_frame.set_align(Align::Left | Align::Inside | Align::Wrap | Align::Clip);

    let input = input_default.map(|text| {
        let mut input = theme.input();
        input.set_value(text);
        column.fixed(&input, POPUP_ROW_HEIGHT);
        input
    });

    let result: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));

    let mut row = Flex::default();
    row.set_type(FlexType::Row);
    row.set_spacing(POPUP_SPACING);
    Frame::default(); // spacer: the buttons sit on the right, as in FLTK
    let mut default_btn = None;
    for &(index, label) in &layout.buttons {
        // The Enter default stands out; the others are secondary.
        let mut btn = if index == layout.default {
            theme.button()
        } else {
            theme.secondary_button()
        }
        .with_label(label);
        row.fixed(&btn, popup_button_width(label));
        let result_btn = result.clone();
        let mut dialog_btn = dialog.clone();
        btn.set_callback(move |_| {
            result_btn.set(Some(index));
            dialog_btn.hide();
        });
        if index == layout.default {
            default_btn = Some(btn);
        }
    }
    row.end();
    column.fixed(&row, POPUP_ROW_HEIGHT);
    column.end();
    dialog.end();

    // Escape and the window's close button dismiss with no choice.
    dialog.set_callback(|w| w.hide());
    // Enter (main or keypad) presses the default button, even from the input.
    if let Some(btn) = default_btn.clone() {
        dialog.handle(move |_, ev| {
            if ev == Event::Shortcut && matches!(app::event_key(), Key::Enter | Key::KPEnter) {
                btn.clone().do_callback();
                return true;
            }
            false
        });
    }

    ThemedPopup {
        dialog,
        input,
        default_btn,
        result,
    }
}

/// Build, without showing, the themed text prompt [`show_themed_input`]
/// shows (Duplicate/Rename name prompt). For the UI construction tests
/// (`tests/ui_dialogs.rs`).
pub fn build_themed_input(
    theme_bg: (u8, u8, u8),
    title: &str,
    prompt: &str,
    default: &str,
) -> Window {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    build_themed_popup(
        &theme,
        title,
        prompt,
        Some(default),
        &choice_layout("Cancel", "OK", ""),
    )
    .dialog
}

/// Build, without showing, the themed notice [`show_themed_message`] shows.
/// For the UI construction tests (`tests/ui_dialogs.rs`).
pub fn build_themed_message(theme_bg: (u8, u8, u8), title: &str, message: &str) -> Window {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    build_themed_popup(
        &theme,
        title,
        message,
        None,
        &choice_layout("Close", "", ""),
    )
    .dialog
}

/// Build, without showing, the themed question [`show_themed_choice`] shows
/// (e.g. the large-file warning). For the UI construction tests.
pub fn build_themed_choice(
    theme_bg: (u8, u8, u8),
    title: &str,
    message: &str,
    b0: &str,
    b1: &str,
    b2: &str,
) -> Window {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    build_themed_popup(&theme, title, message, None, &choice_layout(b0, b1, b2)).dialog
}

/// Show the themed popup (see [`build_themed_popup`]) and run it with
/// [`run_dialog`]. Returns the pressed button's index (`None` for Escape, the
/// window's close button, or an app quit) and the input's final text.
fn run_themed_popup(
    theme_bg: (u8, u8, u8),
    title: &str,
    message: &str,
    input_default: Option<&str>,
    layout: &ChoiceLayout<'_>,
) -> (Option<usize>, String) {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let ThemedPopup {
        dialog,
        input,
        default_btn,
        result,
    } = build_themed_popup(&theme, title, message, input_default, layout);

    let mut dialog = dialog.center_screen();
    dialog.show();
    theme.apply_titlebar(&dialog);
    if let Some(input) = &input {
        // Focus the input with its text selected, so typing replaces it.
        let mut input = input.clone();
        let _ = input.take_focus();
        let end = input.value().len() as i32;
        let _ = input.set_position(end);
        let _ = input.set_mark(0);
    } else if let Some(mut btn) = default_btn {
        let _ = btn.take_focus();
    }
    run_dialog(&dialog);

    let text = input.map(|i| i.value()).unwrap_or_default();
    (result.get(), text)
}

/// Themed info/error notice replacing FLTK's unthemed `dialog::message_default`
/// and `dialog::alert_default`: the message and a "Close" button (Enter,
/// Escape and the window's close button all dismiss it). Pass the current
/// theme background so the popup matches the app.
pub fn show_themed_message(theme_bg: (u8, u8, u8), title: &str, message: &str) {
    run_themed_popup(
        theme_bg,
        title,
        message,
        None,
        &choice_layout("Close", "", ""),
    );
}

/// Themed question replacing FLTK's unthemed `dialog::choice2_default`, with
/// the same button contract (see [`ChoiceLayout`]): returns the index of the
/// pressed button (`0` for `b0`, `1` for `b1`, `2` for `b2`), or `None` when
/// the popup is dismissed with Escape or its close button.
pub fn show_themed_choice(
    theme_bg: (u8, u8, u8),
    title: &str,
    message: &str,
    b0: &str,
    b1: &str,
    b2: &str,
) -> Option<usize> {
    run_themed_popup(theme_bg, title, message, None, &choice_layout(b0, b1, b2)).0
}

/// Themed text prompt replacing FLTK's unthemed `dialog::input_default`:
/// the prompt, an input prefilled with `default` (selected, so typing
/// replaces it), and OK / Cancel. Returns the text on OK or Enter, `None` on
/// Cancel, Escape or the close button.
pub fn show_themed_input(
    theme_bg: (u8, u8, u8),
    title: &str,
    prompt: &str,
    default: &str,
) -> Option<String> {
    // FLTK's own input dialog layout: b0 = Cancel (rightmost), b1 = OK (Enter).
    let layout = choice_layout("Cancel", "OK", "");
    match run_themed_popup(theme_bg, title, prompt, Some(default), &layout) {
        (Some(1), text) => Some(text),
        _ => None,
    }
}

/// FLTK widget checks that must run on the process main thread: on macOS,
/// AppKit aborts when FLTK opens the display from another thread, which is
/// where libtest runs unit tests. Called by the `harness = false` test binary
/// `tests/fltk_main_thread.rs` (T0089); not part of the public API.
#[doc(hidden)]
pub mod main_thread_checks {
    use super::*;
    use fltk::group::Group;

    /// Regression for T0058: a popup built while another window's group is
    /// current (begun, not ended) must still be a top-level window, must not
    /// add itself to that group, and must leave that group current.
    pub fn popup_is_top_level() {
        let theme = DialogTheme::from_theme_bg((40, 44, 52));
        let previous = Group::try_current();

        let outer = Window::new(0, 0, 600, 400, None); // begun: now current
        let outer_children = outer.children();
        let message = build_themed_popup(
            &theme,
            "Error",
            "Error opening file: No such file or directory",
            None,
            &choice_layout("Close", "", ""),
        );
        let input = build_themed_popup(
            &theme,
            "Go To Line",
            "Line number:",
            Some("1"),
            &choice_layout("Cancel", "OK", ""),
        );
        let current_after = Group::try_current().map(|g| g.as_widget_ptr());
        let children_after = outer.children();

        // Restore FLTK's process-global current group before asserting, so a
        // failure leaves no group begun for later tests.
        outer.end();
        Group::set_current(previous.as_ref());

        for popup in [&message, &input] {
            assert!(
                popup.dialog.parent().is_none(),
                "the popup must be a top-level window, not a child of the current group"
            );
        }
        assert_eq!(
            children_after, outer_children,
            "outer group gained children"
        );
        assert_eq!(
            current_after,
            Some(outer.as_widget_ptr()),
            "the caller's current group must be restored"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T0105: the user-chosen field border reads at >= 3:1 against the
    /// dialog background (WCAG 1.4.11) on every theme.
    #[test]
    fn input_border_stands_out_on_every_theme() {
        use crate::ui::color::wcag_contrast;
        let mut failures = Vec::new();
        for (name, bg) in dialog_list_test_backgrounds() {
            let theme = DialogTheme::from_theme_bg(bg);
            let ratio = wcag_contrast(theme.input_border.to_rgb(), theme.bg.to_rgb());
            if ratio < 3.0 {
                failures.push(format!(
                    "{name}: border {:?} on dialog {:?} = {ratio:.2}:1",
                    theme.input_border.to_rgb(),
                    theme.bg.to_rgb()
                ));
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    /// a3's T0097 nit: the registered list selection must follow every
    /// re-theme, not wait for the next dialog list to be styled.
    #[test]
    fn dialog_list_selection_follows_every_retheme() {
        let theme_rs = include_str!("../theme.rs");
        let body = theme_rs
            .split("pub fn apply_syntax_theme_colors")
            .nth(1)
            .and_then(|rest| rest.split("\n}\n").next())
            .expect("apply_syntax_theme_colors");
        assert!(
            body.contains("register_dialog_list_selection(background)"),
            "every re-theme must refresh the dialog list selection"
        );
        for bg in [(43, 48, 59), (239, 241, 245), (59, 31, 92)] {
            register_dialog_list_selection(bg);
            assert_eq!(
                crate::ui::contrast::registered_list_selection(),
                Some(DialogTheme::from_theme_bg(bg).list_selection_bg.to_rgb()),
                "{bg:?}"
            );
        }
    }

    /// Theme backgrounds to check: every built-in syntax theme plus a sweep
    /// of grays and tinted colors, standing in for custom themes.
    fn dialog_list_test_backgrounds() -> Vec<(String, (u8, u8, u8))> {
        use crate::app::domain::settings::SyntaxTheme;
        let mut bgs: Vec<(String, (u8, u8, u8))> = SyntaxTheme::all()
            .iter()
            .map(|t| (format!("{t:?}"), t.background()))
            .collect();
        for v in (0..=255u8).step_by(15) {
            bgs.push((format!("gray {v}"), (v, v, v)));
            bgs.push((format!("blue-ish {v}"), (v / 2, v / 2, v)));
            bgs.push((format!("warm {v}"), (v, v.saturating_sub(20), v / 2)));
        }
        bgs
    }

    /// T0097: the selected row of a dialog list must stand out from the list
    /// background (>= 3:1) and the text FLTK draws on it must be readable
    /// (>= 4.5:1), in light and dark themes. Measured 1.39:1 in the theme
    /// manager before.
    #[test]
    fn dialog_list_selection_stands_out_in_every_theme() {
        use crate::ui::color::wcag_contrast;
        use crate::ui::contrast::{EditorSelectionBgs, ferrispad_contrast};
        let mut failures = Vec::new();
        for (name, bg) in dialog_list_test_backgrounds() {
            let theme = DialogTheme::from_theme_bg(bg);
            let (sel, text) = theme.list_selection();
            let list_bg = theme.input_bg.to_rgb();
            let row = wcag_contrast(sel.to_rgb(), list_bg);
            // What FLTK draws on the selected row: fl_contrast(text, sel),
            // with the dialog list selection known to the contrast rule.
            let bgs = EditorSelectionBgs::default().with_list_selection(sel.to_rgb());
            let drawn = ferrispad_contrast(theme.text, sel, bgs).to_rgb();
            let readable = wcag_contrast(drawn, sel.to_rgb());
            if row < 3.0 || readable < 4.5 || drawn != text.to_rgb() {
                failures.push(format!(
                    "{name}: row {:?} on list {list_bg:?} = {row:.2}:1, text {drawn:?} = {readable:.2}:1 (expected text {:?})",
                    sel.to_rgb(),
                    text.to_rgb()
                ));
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn scrollbar_colors_dark_darkens_track_and_lightens_thumb() {
        let (track, thumb) = scrollbar_colors(ThemeRgb::new(40, 44, 52), true);
        // track = base * 0.70; thumb = base + (255 - base) * 0.20
        assert_eq!(track, fltk_rgb(28, 30, 36));
        assert_eq!(thumb, fltk_rgb(83, 86, 92));
    }

    #[test]
    fn scrollbar_colors_light_darkens_both() {
        let (track, thumb) = scrollbar_colors(ThemeRgb::new(250, 250, 250), false);
        // track = base * 0.95; thumb = base * 0.75
        assert_eq!(track, fltk_rgb(237, 237, 237));
        assert_eq!(thumb, fltk_rgb(187, 187, 187));
    }

    #[test]
    fn dialog_theme_scrollbar_colors_derive_from_dialog_bg() {
        let theme = DialogTheme::from_theme_bg((40, 44, 52));
        assert_eq!(
            (theme.scroll_track, theme.scroll_thumb),
            scrollbar_colors(ThemeRgb::from_fltk(theme.bg), true)
        );
    }

    /// The Cancel/Close buttons of the large-file and read-only-viewer dialogs
    /// sit on the dialog background, so they must be derived from it — not from
    /// a stale copy of the pre-cdf5750 dialog factors (0.65 / 0.85).
    #[test]
    fn secondary_button_bg_derives_from_dialog_bg() {
        for theme_bg in [(40, 44, 52), (30, 30, 30), (253, 246, 227), (255, 255, 255)] {
            let theme = DialogTheme::from_theme_bg(theme_bg);
            let bg = ThemeRgb::from_fltk(theme.bg);
            let expected = if theme.is_dark() {
                bg.lighten(0.10)
            } else {
                bg.darken(0.90)
            };
            assert_eq!(
                ThemeRgb::from_fltk(theme.secondary_button_bg),
                expected,
                "theme_bg {theme_bg:?}"
            );
        }
    }

    #[test]
    fn dialog_theme_remembers_its_theme_bg() {
        assert_eq!(
            DialogTheme::from_theme_bg((40, 44, 52)).theme_bg(),
            (40, 44, 52)
        );
    }

    // --- Themed popups (T0024): FLTK `choice2_default` button contract ---

    #[test]
    fn choice_layout_puts_b0_rightmost_and_b1_as_default() {
        let layout = choice_layout("Save", "Discard", "Cancel");
        assert_eq!(
            layout.buttons,
            vec![(2, "Cancel"), (1, "Discard"), (0, "Save")]
        );
        assert_eq!(layout.default, 1);
    }

    #[test]
    fn choice_layout_skips_empty_labels() {
        let layout = choice_layout("Reload", "Keep Mine", "");
        assert_eq!(layout.buttons, vec![(1, "Keep Mine"), (0, "Reload")]);
        assert_eq!(layout.default, 1);
    }

    #[test]
    fn choice_layout_single_button_is_the_default() {
        let layout = choice_layout("Close", "", "");
        assert_eq!(layout.buttons, vec![(0, "Close")]);
        assert_eq!(layout.default, 0);
    }

    #[test]
    fn choice_layout_default_falls_back_to_b2() {
        let layout = choice_layout("", "", "Only");
        assert_eq!(layout.buttons, vec![(2, "Only")]);
        assert_eq!(layout.default, 2);
    }

    #[test]
    fn choice_layout_never_has_no_button() {
        let layout = choice_layout("", "", "");
        assert_eq!(layout.buttons, vec![(0, "Close")]);
        assert_eq!(layout.default, 0);
    }

    #[test]
    fn wrapped_line_count_counts_newlines_and_wraps() {
        assert_eq!(wrapped_line_count("", 10), 1);
        assert_eq!(wrapped_line_count("short", 10), 1);
        assert_eq!(wrapped_line_count("a\n\nb", 10), 3);
        assert_eq!(wrapped_line_count(&"x".repeat(25), 10), 3);
        // Counted in chars, not bytes.
        assert_eq!(wrapped_line_count(&"é".repeat(10), 10), 1);
        // A zero width cannot divide by zero.
        assert_eq!(wrapped_line_count("abc", 0), 3);
    }

    #[test]
    fn popup_height_grows_with_the_message_and_is_capped() {
        let one = popup_height("one line", false);
        let five = popup_height("1\n2\n3\n4\n5", false);
        assert!(five > one);
        assert_eq!(five - one, 3 * POPUP_LINE_HEIGHT); // short messages get 2 lines
        assert_eq!(
            popup_height("x", true) - popup_height("x", false),
            POPUP_ROW_HEIGHT + POPUP_SPACING
        );
        let huge = "line\n".repeat(500);
        assert_eq!(
            popup_height(&huge, false),
            popup_height(&"line\n".repeat(POPUP_MAX_LINES - 1), false)
        );
    }

    #[test]
    fn popup_button_width_fits_long_labels() {
        assert_eq!(popup_button_width("OK"), 80);
        assert!(popup_button_width("Quit Without Saving") > 80);
        assert!(
            popup_button_width("Save All")
                + popup_button_width("Quit Without Saving")
                + popup_button_width("Cancel")
                + 3 * POPUP_SPACING
                <= POPUP_WIDTH - 2 * POPUP_PADDING
        );
    }
}
