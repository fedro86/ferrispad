pub mod about;
pub mod find;
pub mod goto_line;
pub mod large_file;
pub mod plugin_config;
pub mod plugin_manager;
pub mod plugin_permissions;
pub mod plugin_settings;
pub mod readonly_viewer;
pub mod settings_dialog;
pub mod update;

use fltk::{app, enums::Color, prelude::*, window::Window};

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
    /// Alternate row background (for zebra striping)
    pub row_bg_alt: Color,
    /// Whether the theme is dark mode
    is_dark: bool,
}

/// Helper to darken a color (shift toward black)
fn darken(r: u8, g: u8, b: u8, factor: f32) -> (u8, u8, u8) {
    (
        (r as f32 * factor) as u8,
        (g as f32 * factor) as u8,
        (b as f32 * factor) as u8,
    )
}

/// Helper to lighten a color (shift toward white)
fn lighten(r: u8, g: u8, b: u8, factor: f32) -> (u8, u8, u8) {
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
            // Light dialog: darken buttons for contrast
            darken(bg_r, bg_g, bg_b, 0.85)
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

        // Row backgrounds for lists (same logic as buttons)
        let row_bg = if is_dark {
            let (rr, rg, rb) = lighten(bg_r, bg_g, bg_b, 0.10);
            Color::from_rgb(rr, rg, rb)
        } else {
            Color::from_rgb(r, g, b)
        };

        let row_bg_alt = if is_dark {
            let (rr, rg, rb) = lighten(bg_r, bg_g, bg_b, 0.15);
            Color::from_rgb(rr, rg, rb)
        } else {
            let (rr, rg, rb) = darken(r, g, b, 0.97);
            Color::from_rgb(rr, rg, rb)
        };

        // Text colors based on brightness
        let (text, text_dim) = if is_dark {
            (
                Color::from_rgb(230, 230, 230),
                Color::from_rgb(140, 140, 140),
            )
        } else {
            (
                Color::from_rgb(0, 0, 0),
                Color::from_rgb(80, 80, 80),
            )
        };

        Self {
            bg,
            text,
            text_dim,
            input_bg,
            button_bg,
            tab_active_bg,
            row_bg,
            row_bg_alt,
            is_dark,
        }
    }

    /// Check if the theme is dark mode
    pub fn is_dark(&self) -> bool {
        self.is_dark
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
