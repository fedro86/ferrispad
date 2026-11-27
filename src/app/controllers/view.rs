use fltk::{
    prelude::*,
    text::{TextBuffer, TextEditor, WrapMode},
};

use super::tabs::TabManager;
use crate::ui::line_numbers;

/// Manages view-related state and toggles (line numbers, word wrap, dark mode, fonts).
///
/// Holds cheap FLTK widget clones (pointer copies) so methods can operate
/// without borrowing AppState fields.
pub struct ViewController {
    pub dark_mode: bool,
    pub show_linenumbers: bool,
    pub word_wrap: bool,
    editor: TextEditor,
    /// Digits of the largest line number the gutter is sized for (T0107).
    gutter_digits: u32,
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
            gutter_digits: 0,
        }
    }

    /// Size the line-number gutter for the active document's largest line
    /// number, in the line-number font (T0107). Sets the width only when it
    /// changes: setting it makes FLTK recalculate the whole display.
    pub fn update_linenumber_width(&mut self, tab_manager: &TabManager) {
        let width = if self.show_linenumbers {
            let line_count = tab_manager
                .active_doc()
                .map(|d| d.cached_line_count)
                .unwrap_or(0);
            self.gutter_digits = line_numbers::digits(line_count);
            line_numbers::gutter_width(
                line_count,
                self.editor.linenumber_font(),
                self.editor.linenumber_size(),
            )
        } else {
            0
        };
        if self.editor.linenumber_width() != width {
            self.editor.set_linenumber_width(width);
        }
    }

    /// The active document's line count changed: resize the gutter only if
    /// its largest line number gained or lost a digit (T0107), so typing
    /// costs nothing here.
    pub fn line_count_changed(&mut self, tab_manager: &TabManager) {
        let digits = tab_manager
            .active_doc()
            .map_or(1, |d| line_numbers::digits(d.cached_line_count));
        if self.show_linenumbers && digits != self.gutter_digits {
            self.update_linenumber_width(tab_manager);
        }
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
