//! Status bar widget showing file path (left) and cursor position (right).
//!
//! Displays the relative file path on the left and `Ln X, Col Y` with
//! optional selection info on the right, composed into a single label.
//! Updated on every event loop iteration but short-circuits when nothing
//! has changed (0% CPU when idle).

use crate::ui::color::fltk_rgb;
use fltk::{
    enums::{Align, Font, FrameType},
    frame::Frame,
    prelude::*,
    text::TextEditor,
};

use super::dialogs::DialogTheme;
use super::theme::{PanelTheme, Themeable};

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
        frame.set_color(fltk_rgb(45, 45, 45));
        frame.set_label_color(fltk_rgb(180, 180, 180));
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
            let label = compose_label(&self.path_text, &self.pos_text, total_chars);

            if label != self.last_label {
                self.frame.set_label(&label);
                self.last_label = label;
            }
        }
    }
}

/// Columns a character takes in the status bar: 2 for East Asian wide and
/// fullwidth characters (drawn about twice as wide as an ASCII one), else 1.
fn char_columns(c: char) -> usize {
    match c as u32 {
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// Columns `text` takes in the status bar.
fn columns(text: &str) -> usize {
    text.chars().map(char_columns).sum()
}

/// The status bar's label for a bar `total` columns wide: the path on the
/// left, "Ln X, Col Y" (`pos`) on the right, spaces between. Measured in
/// columns ([`columns`]), so a CJK path cannot push `pos` off the bar.
fn compose_label(path: &str, pos: &str, total: usize) -> String {
    let pos_len = columns(pos);
    let margin = 6; // 2 chars left + 4 chars right padding
    let available_for_path = total.saturating_sub(pos_len + margin);

    let display_path = fit_path_left(path, available_for_path);

    if display_path.is_empty() {
        // Right-align with right padding
        let inner = total.saturating_sub(4);
        format!("{:>width$}    ", pos, width = inner)
    } else {
        // Path on left, Ln/Col on right, fill with spaces
        let gap = total.saturating_sub(columns(&display_path) + pos_len + margin);
        format!("  {}{:gap$}{}    ", display_path, "", pos, gap = gap)
    }
}

/// `path` shortened from the left to fit `available` columns: "…/rest" cut
/// at a `/` when there is one, or empty when too narrow to be useful.
/// Measured in columns ([`columns`]) and cut between characters, never at a
/// byte offset, which can fall inside a multi-byte character and panic
/// (T0124).
fn fit_path_left(path: &str, available: usize) -> String {
    if path.is_empty() {
        String::new()
    } else if columns(path) <= available {
        path.to_string()
    } else if available > 5 {
        // Keep the last characters that fit, leaving 2 columns for "…/".
        let keep = available - 2;
        let mut used = 0;
        let mut skip = path.len();
        for (i, c) in path.char_indices().rev() {
            used += char_columns(c);
            if used > keep {
                break;
            }
            skip = i;
        }
        let tail = &path[skip..];
        // Start at a `/` when there is one; else hard-truncate.
        let from = tail.find('/').unwrap_or(0);
        format!("\u{2026}{}", &tail[from..])
    } else {
        String::new() // Too narrow to show anything useful
    }
}

impl Themeable for StatusBar {
    /// Colors derived from the syntax theme background (matches tab bar).
    fn apply_theme(&mut self, theme: PanelTheme) {
        let dialog = DialogTheme::from_theme_bg(theme.bg);
        self.frame.set_color(dialog.bg);
        self.frame.set_label_color(dialog.text_dim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path with multi-byte characters everywhere, so some width cuts
    /// inside one of them.
    const PATH: &str =
        "/home/user/Präsentationen-ÄÖÜ-äöü-ß/Übersicht-Größenänderung/日本語のメモ/notes.txt";

    #[test]
    fn long_non_ascii_path_never_panics() {
        for available in 0..=PATH.chars().count() + 2 {
            let shown = fit_path_left(PATH, available);
            assert!(
                columns(&shown) <= available,
                "{shown:?} is wider than {available} characters"
            );
        }
    }

    #[test]
    fn shortened_path_starts_at_a_slash() {
        let shown = fit_path_left(PATH, 30);
        assert!(shown.starts_with("\u{2026}/"), "{shown:?}");
        assert!(PATH.ends_with(shown.trim_start_matches('\u{2026}')));
        assert_eq!(fit_path_left("/a/b.txt", 30), "/a/b.txt");
        assert_eq!(fit_path_left(PATH, 3), "");
    }

    /// Columns as drawn: a CJK or fullwidth glyph is about two ASCII ones.
    fn drawn_columns(s: &str) -> usize {
        s.chars()
            .map(|c| match c as u32 {
                0x3000..=0x9FFF | 0xFF00..=0xFF60 => 2,
                _ => 1,
            })
            .sum()
    }

    /// Found in the real app (compare.sh, status-bar-path.sh): with a
    /// Japanese file name, "Ln X, Col Y" was pushed past the bar's right
    /// edge, because a CJK glyph is drawn about two characters wide.
    #[test]
    fn ln_col_stays_inside_the_bar_with_a_cjk_path() {
        let pos = "Ln 2, Col 1";
        for total in 20..=140 {
            let label = compose_label(PATH, pos, total);
            assert!(label.ends_with(&format!("{pos}    ")), "{label:?}");
            assert!(
                drawn_columns(&label) <= total,
                "{} columns in a {total}-column bar: {label:?}",
                drawn_columns(&label)
            );
        }
    }
}
