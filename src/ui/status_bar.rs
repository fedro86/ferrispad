//! Status bar widget showing cursor position and selection info.
//!
//! Displays `Ln X, Col Y` and optionally `N lines selected` at the bottom
//! of the window. Updated on every event loop iteration but short-circuits
//! when cursor position and selection haven't changed (0% CPU when idle).

use fltk::{
    enums::{Align, Color, Font, FrameType},
    frame::Frame,
    prelude::*,
    text::TextEditor,
};

use super::dialogs::DialogTheme;

/// Height of the status bar in pixels.
pub const STATUS_BAR_HEIGHT: i32 = 20;

pub struct StatusBar {
    frame: Frame,
    last_label: String,
    /// Cached cursor position to skip recomputation when nothing changed.
    last_pos: i32,
    /// Cached selection range.
    last_selection: Option<(i32, i32)>,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    pub fn new() -> Self {
        let mut frame = Frame::default().with_size(0, STATUS_BAR_HEIGHT);
        frame.set_frame(FrameType::FlatBox);
        frame.set_color(Color::from_rgb(45, 45, 45));
        frame.set_label_color(Color::from_rgb(180, 180, 180));
        frame.set_label_font(Font::Courier);
        frame.set_label_size(12);
        frame.set_align(Align::Right | Align::Inside);

        Self {
            frame,
            last_label: String::new(),
            last_pos: -1,
            last_selection: None,
        }
    }

    pub fn widget(&self) -> &Frame {
        &self.frame
    }

    /// Recompute cursor position and selection info from the editor.
    /// Short-circuits when position and selection haven't changed.
    pub fn update(&mut self, editor: &TextEditor) {
        let buf = editor.buffer().unwrap();
        let pos = editor.insert_position();
        let sel = buf.selection_position().filter(|(s, e)| s != e);

        // Skip all work if nothing changed
        if pos == self.last_pos && sel == self.last_selection {
            return;
        }
        self.last_pos = pos;
        self.last_selection = sel;

        // Line number (1-indexed)
        let line = buf.count_lines(0, pos) + 1;

        // Column (1-indexed): distance from line start
        let line_start = buf.line_start(pos);
        let col = pos - line_start + 1;

        let label = if let Some((start, end)) = sel {
            let sel_lines = buf.count_lines(start, end) + 1;
            if sel_lines > 1 {
                format!("Ln {}, Col {}    {} lines selected  ", line, col, sel_lines)
            } else {
                let chars = end - start;
                format!("Ln {}, Col {}    {} chars selected  ", line, col, chars)
            }
        } else {
            format!("Ln {}, Col {}  ", line, col)
        };

        if label != self.last_label {
            self.frame.set_label(&label);
            self.last_label = label;
        }
    }

    /// Apply theme colors derived from syntax theme background (matches tab bar).
    pub fn apply_theme(&mut self, theme_bg: (u8, u8, u8)) {
        let theme = DialogTheme::from_theme_bg(theme_bg);
        self.frame.set_color(theme.bg);
        self.frame.set_label_color(theme.text_dim);
    }
}
