use fltk::{
    prelude::*,
    text::{TextBuffer, TextEditor, WrapMode},
};

use super::tabs::TabManager;

/// Manages view-related state and toggles (line numbers, word wrap, dark mode, fonts).
///
/// Holds cheap FLTK widget clones (pointer copies) so methods can operate
/// without borrowing AppState fields.
pub struct ViewController {
    pub dark_mode: bool,
    pub show_linenumbers: bool,
    pub word_wrap: bool,
    editor: TextEditor,
}

impl ViewController {
    pub fn new(
        editor: TextEditor,
        dark_mode: bool,
        show_linenumbers: bool,
        word_wrap: bool,
    ) -> Self {
        Self {
            dark_mode,
            show_linenumbers,
            word_wrap,
            editor,
        }
    }

    /// Update line number gutter width based on document line count.
    pub fn update_linenumber_width(&mut self, tab_manager: &TabManager) {
        if !self.show_linenumbers {
            self.editor.set_linenumber_width(0);
            return;
        }
        let line_count = tab_manager
            .active_doc()
            .map(|d| d.cached_line_count)
            .unwrap_or(0);
        let digits = ((line_count as i32 + 1) as f64).log10().floor() as i32 + 1;
        let width = (digits * 8 + 16).max(40);
        self.editor.set_linenumber_width(width);
    }

    pub fn toggle_line_numbers(&mut self, tab_manager: &TabManager) {
        self.show_linenumbers = !self.show_linenumbers;
        self.update_linenumber_width(tab_manager);
        self.editor.redraw();
    }

    pub fn toggle_word_wrap(&mut self) {
        self.word_wrap = !self.word_wrap;
        if self.word_wrap {
            self.editor.wrap_mode(WrapMode::AtBounds, 0);
        } else {
            self.editor.wrap_mode(WrapMode::None, 0);
        }
        self.editor.redraw();
    }

    /// Navigate to a specific line number (1-indexed).
    pub fn goto_line(&mut self, buf: &TextBuffer, line: u32) {
        let line_count = buf.count_lines(0, buf.length());

        // Clamp line to valid range
        let target_line = (line as i32).min(line_count).max(1);

        // Find position of the line
        let mut pos = 0;
        for _ in 1..target_line {
            if let Some(next_pos) = buf.find_char_forward(pos, '\n') {
                pos = next_pos + 1;
            } else {
                break;
            }
        }

        self.editor.set_insert_position(pos);
        self.editor.show_insert_position();
        self.editor.take_focus().ok();
    }
}
