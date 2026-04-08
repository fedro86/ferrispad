pub mod about;
pub mod community_install;
pub mod find;
pub mod goto_line;
pub mod large_file;
pub mod plugin_config;
pub mod plugin_manager;
pub mod plugin_permissions;
pub mod plugin_settings;
pub mod readonly_viewer;
pub mod session_picker;
pub mod settings_dialog;
pub mod shortcut_dialog;
pub mod update;

use fltk::{
    app,
    enums::{Color, FrameType},
    group::Scroll,
    prelude::*,
    window::Window,
};

/// Global scrollbar width used across all dialogs.
pub const SCROLLBAR_SIZE: i32 = 12;

/// Theme colors for dialogs, derived from the syntax theme background.
/// Uses the same derivation logic as tab_bar for visual consistency.
#[derive(Clone, Copy)]
pub struct DialogTheme {
    /// Dialog window background (matches tab bar background)
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
    /// Row background (for lists/tables)
    pub row_bg: Color,
    /// Accent color — complementary hue from bg, for links/highlights
    pub accent: Color,
    /// Accent hover — slightly adjusted accent for hover state
    pub accent_hover: Color,
    /// Scrollbar track background
    pub scroll_track: Color,
    /// Scrollbar thumb/slider color
    pub scroll_thumb: Color,
    /// Whether the theme is dark mode
    is_dark: bool,
    /// Original syntax theme background (needed for titlebar theming on Windows)
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
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
    (
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
    )
}

fn hue_to_rgb(p: f32, q: f32, t: f32) -> f32 {
    let t = if t < 0.0 { t + 1.0 } else if t > 1.0 { t - 1.0 } else { t };
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

/// Helper to darken a color (shift toward black)
pub(crate) fn darken(r: u8, g: u8, b: u8, factor: f32) -> (u8, u8, u8) {
    (
        (r as f32 * factor) as u8,
        (g as f32 * factor) as u8,
        (b as f32 * factor) as u8,
    )
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
    const KEEP: f32 = 0.81;   // factor for dominant channel
    const MID: f32 = 0.48;    // factor for middle channel
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

/// Helper to lighten a color (shift toward white)
pub(crate) fn lighten(r: u8, g: u8, b: u8, factor: f32) -> (u8, u8, u8) {
    (
        r + ((255 - r) as f32 * factor) as u8,
        g + ((255 - g) as f32 * factor) as u8,
        b + ((255 - b) as f32 * factor) as u8,
    )
}

impl DialogTheme {
    /// Create theme colors based on the syntax theme background.
    /// This ensures dialogs match the tab bar and main window appearance.
    ///
    /// Color derivation logic (same as tab_bar):
    /// - Dialog bg: darker than editor (matches tab bar background)
    /// - Buttons/tabs: contrast with dialog bg (lighter if dark bg, darker if light bg)
    pub fn from_theme_bg(theme_bg: (u8, u8, u8)) -> Self {
        let (r, g, b) = theme_bg;
        let brightness = (r as u32 + g as u32 + b as u32) / 3;
        let is_dark = brightness < 128;

        // Dialog background: match tab bar background (darker than editor)
        // Same logic as tab_bar: darken(0.65) for dark, darken(0.85) for light
        let (bg_r, bg_g, bg_b) = if is_dark {
            darken(r, g, b, 0.65)
        } else {
            darken(r, g, b, 0.85)
        };
        let bg = Color::from_rgb(bg_r, bg_g, bg_b);

        // Button background: contrast with dialog bg
        // Light bg → buttons darker (toward black)
        // Dark bg → buttons lighter (toward white)
        let (btn_r, btn_g, btn_b) = if is_dark {
            // Dark dialog: lighten buttons for contrast
            lighten(bg_r, bg_g, bg_b, 0.15)
        } else {
            // Light dialog: intensify dominant channel from original theme bg.
            // For neutral themes (R ≈ B), fall back to simple darken.
            let (ir, ig, ib) = intensify_dominant(r, g, b, 0.75);
            let (dr, dg, db) = darken(r, g, b, 0.72);
            // Blend: use intensified if polarized, darkened if neutral
            let warmth = (r as f32 - b as f32).abs();
            let t = (warmth / 30.0).clamp(0.0, 1.0);
            (
                (ir as f32 * t + dr as f32 * (1.0 - t)) as u8,
                (ig as f32 * t + dg as f32 * (1.0 - t)) as u8,
                (ib as f32 * t + db as f32 * (1.0 - t)) as u8,
            )
        };
        let button_bg = Color::from_rgb(btn_r, btn_g, btn_b);

        // Tab active background: slightly lighter than button_bg for selected/active state
        let tab_active_bg = if is_dark {
            let (tr, tg, tb) = lighten(btn_r, btn_g, btn_b, 0.10);
            Color::from_rgb(tr, tg, tb)
        } else {
            let (tr, tg, tb) = lighten(btn_r, btn_g, btn_b, 0.15);
            Color::from_rgb(tr, tg, tb)
        };

        // Input background: closer to editor background (lighter than dialog)
        let input_bg = if is_dark {
            let (ir, ig, ib) = lighten(bg_r, bg_g, bg_b, 0.25);
            Color::from_rgb(ir, ig, ib)
        } else {
            // Light mode: same as editor background
            Color::from_rgb(r, g, b)
        };

        // Row backgrounds for lists/cards
        let row_bg = if is_dark {
            let (rr, rg, rb) = lighten(bg_r, bg_g, bg_b, 0.10);
            Color::from_rgb(rr, rg, rb)
        } else {
            // Light: subtle intensification for cards, darken fallback for neutral
            let (ir, ig, ib) = intensify_dominant(r, g, b, 0.15);
            let (dr, dg, db) = darken(r, g, b, 0.95);
            let warmth = (r as f32 - b as f32).abs();
            let t = (warmth / 30.0).clamp(0.0, 1.0);
            let rr = (ir as f32 * t + dr as f32 * (1.0 - t)) as u8;
            let rg = (ig as f32 * t + dg as f32 * (1.0 - t)) as u8;
            let rb = (ib as f32 * t + db as f32 * (1.0 - t)) as u8;
            Color::from_rgb(rr, rg, rb)
        };

        // Text colors based on brightness
        let (text, text_dim) = if is_dark {
            (
                Color::from_rgb(230, 230, 230),
                Color::from_rgb(140, 140, 140),
            )
        } else {
            (Color::from_rgb(0, 0, 0), Color::from_rgb(80, 80, 80))
        };

        // Scrollbar colors
        let (scroll_track, scroll_thumb) = if is_dark {
            let (tr, tg, tb) = darken(bg_r, bg_g, bg_b, 0.70);
            let (thr, thg, thb) = lighten(bg_r, bg_g, bg_b, 0.20);
            (Color::from_rgb(tr, tg, tb), Color::from_rgb(thr, thg, thb))
        } else {
            let (tr, tg, tb) = darken(bg_r, bg_g, bg_b, 0.95);
            let (thr, thg, thb) = darken(bg_r, bg_g, bg_b, 0.75);
            (Color::from_rgb(tr, tg, tb), Color::from_rgb(thr, thg, thb))
        };

        // Accent color: complementary hue from theme bg
        let (ar, ag, ab) = accent_from_bg(r, g, b, is_dark);
        let accent = Color::from_rgb(ar, ag, ab);
        // Accent hover: slightly lighter/darker
        let (ahr, ahg, ahb) = if is_dark {
            lighten(ar, ag, ab, 0.15)
        } else {
            darken(ar, ag, ab, 0.85)
        };
        let accent_hover = Color::from_rgb(ahr, ahg, ahb);

        Self {
            bg,
            text,
            text_dim,
            input_bg,
            button_bg,
            tab_active_bg,
            row_bg,
            accent,
            accent_hover,
            scroll_track,
            scroll_thumb,
            is_dark,
            theme_bg,
        }
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
            Color::from_rgb(255, 120, 120)
        } else {
            Color::from_rgb(180, 0, 0)
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
    }

    /// Apply flat themed scrollbar styling to a Scroll widget.
    pub fn style_scroll(&self, scroll: &mut Scroll) {
        scroll.set_scrollbar_size(SCROLLBAR_SIZE);
        let mut vsb = scroll.scrollbar();
        vsb.set_frame(FrameType::FlatBox);
        vsb.set_color(self.scroll_track);
        vsb.set_slider_frame(FrameType::FlatBox);
        vsb.set_selection_color(self.scroll_thumb);
        let mut hsb = scroll.hscrollbar();
        hsb.set_frame(FrameType::FlatBox);
        hsb.set_color(self.scroll_track);
        hsb.set_slider_frame(FrameType::FlatBox);
        hsb.set_selection_color(self.scroll_thumb);
    }
}

/// Run a dialog's event loop, automatically closing the dialog if the app
/// is quitting (e.g. user clicks X on the main window while a dialog is open).
pub fn run_dialog(dialog: &Window) {
    while dialog.shown() {
        app::wait();
        if app::should_program_quit() {
            let mut d = dialog.clone();
            d.hide();
        }
    }
}
