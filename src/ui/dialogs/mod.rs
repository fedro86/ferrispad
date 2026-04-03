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

/// Helper to darken a color (shift toward black)
pub(crate) fn darken(r: u8, g: u8, b: u8, factor: f32) -> (u8, u8, u8) {
    (
        (r as f32 * factor) as u8,
        (g as f32 * factor) as u8,
        (b as f32 * factor) as u8,
    )
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

        Self {
            bg,
            text,
            text_dim,
            input_bg,
            button_bg,
            tab_active_bg,
            row_bg,
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
