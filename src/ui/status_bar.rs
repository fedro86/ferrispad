//! Status bar widget showing file path (left) and cursor position (right).
//!
//! Displays the relative file path on the left and `Ln X, Col Y` with
//! optional selection info on the right, composed into a single label.
//! Updated on every event loop iteration but short-circuits when nothing
//! has changed (0% CPU when idle).

use fltk::{
    enums::{Align, Color, Font, FrameType},
    frame::Frame,
    prelude::*,
    text::TextEditor,
};

use super::dialogs::DialogTheme;

/// Height of the status bar in pixels.
pub const STATUS_BAR_HEIGHT: i32 = 20;

/// Approximate character width for Courier at size 12 (pixels).
const CHAR_WIDTH: i32 = 7;

pub struct StatusBar {
    frame: Frame,
    last_label: String,
    /// Cached cursor position to skip recomputation when nothing changed.
    last_pos: i32,
    /// Cached selection range.
    last_selection: Option<(i32, i32)>,
    /// Cached file path string.
    last_file_path: Option<String>,
    /// Cached frame width to detect resizes.
    last_width: i32,
    /// Cached position text (right side) for composing the label.
    pos_text: String,
    /// Cached display path (left side, already relative + truncated).
    path_text: String,
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
        frame.set_align(Align::Left | Align::Inside);

        Self {
            frame,
            last_label: String::new(),
            last_pos: -1,
            last_selection: None,
            last_file_path: None,
            last_width: 0,
            pos_text: String::new(),
            path_text: String::new(),
        }
    }

    pub fn widget(&self) -> &Frame {
        &self.frame
    }

    /// Recompute cursor position, selection info, and file path.
    /// Short-circuits when nothing has changed.
    pub fn update(
        &mut self,
        editor: &TextEditor,
        file_path: Option<&str>,
        project_root: Option<&str>,
    ) {
        let buf = editor.buffer().unwrap();
        let pos = editor.insert_position();
        let sel = buf.selection_position().filter(|(s, e)| s != e);
        let current_width = self.frame.w();

        let path_changed = file_path != self.last_file_path.as_deref();
        let pos_changed = pos != self.last_pos || sel != self.last_selection;
        let width_changed = current_width != self.last_width;

        if !path_changed && !pos_changed && !width_changed {
            return;
        }

        // --- Ln/Col text (right side) ---
        if pos_changed {
            self.last_pos = pos;
            self.last_selection = sel;

            let line = buf.count_lines(0, pos) + 1;
            let line_start = buf.line_start(pos);
            let col = pos - line_start + 1;

            self.pos_text = if let Some((start, end)) = sel {
                let sel_lines = buf.count_lines(start, end) + 1;
                if sel_lines > 1 {
                    format!("Ln {}, Col {}    {} lines selected", line, col, sel_lines)
                } else {
                    let chars = end - start;
                    format!("Ln {}, Col {}    {} chars selected", line, col, chars)
                }
            } else {
                format!("Ln {}, Col {}", line, col)
            };
        }

        // --- File path text (left side) ---
        if path_changed {
            self.last_file_path = file_path.map(|s| s.to_string());

            self.path_text = match file_path {
                None => String::new(),
                Some(fp) => {
                    if let Some(root) = project_root {
                        let root_with_sep = if root.ends_with('/') {
                            root.to_string()
                        } else {
                            format!("{}/", root)
                        };
                        if fp.starts_with(&root_with_sep) {
                            fp[root_with_sep.len()..].to_string()
                        } else {
                            fp.to_string()
                        }
                    } else {
                        fp.to_string()
                    }
                }
            };
        }

        // --- Compose single label: "  path ...padding... Ln X, Col Y  " ---
        if path_changed || pos_changed || width_changed {
            self.last_width = current_width;

            let total_chars = (current_width / CHAR_WIDTH).max(0) as usize;
            let pos_len = self.pos_text.len();
            let margin = 4; // 2 chars padding on each side
            let available_for_path = total_chars.saturating_sub(pos_len + margin);

            // Truncate path from left if needed
            let display_path = if self.path_text.is_empty() {
                String::new()
            } else if self.path_text.len() <= available_for_path {
                self.path_text.clone()
            } else if available_for_path > 5 {
                // Find a `/` boundary to truncate at
                let skip = self.path_text.len() - (available_for_path - 2); // 2 for "…/"
                if let Some(slash_offset) = self.path_text[skip..].find('/') {
                    format!("\u{2026}{}", &self.path_text[skip + slash_offset..])
                } else {
                    // No slash found — just hard-truncate
                    format!("\u{2026}{}", &self.path_text[skip..])
                }
            } else {
                String::new() // Too narrow to show anything useful
            };

            let label = if display_path.is_empty() {
                // Right-align only
                format!("{:>width$}", self.pos_text, width = total_chars)
            } else {
                // Path on left, Ln/Col on right, fill with spaces
                let gap = total_chars.saturating_sub(display_path.len() + pos_len + margin);
                format!(
                    "  {}{:gap$}{}  ",
                    display_path,
                    "",
                    self.pos_text,
                    gap = gap,
                )
            };

            if label != self.last_label {
                self.frame.set_label(&label);
                self.last_label = label;
            }
        }
    }

    /// Apply theme colors derived from syntax theme background (matches tab bar).
    pub fn apply_theme(&mut self, theme_bg: (u8, u8, u8)) {
        let theme = DialogTheme::from_theme_bg(theme_bg);
        self.frame.set_color(theme.bg);
        self.frame.set_label_color(theme.text_dim);
    }
}
