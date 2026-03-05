use std::collections::HashMap;

use fltk::enums::{Color, Font};
use fltk::text::{StyleTableEntryExt, TextAttr};
use syntect::highlighting::Color as SyntectColor;

use crate::app::plugins::AnnotationColor;

/// Reserved style characters for line markers (with bgcolor)
/// These use indices 1-6 in the style table ('B'-'G') right after the default 'A'
/// This ensures they map to valid entries in the StyleTableEntryExt array
pub const MARKER_STYLE_ADDED: char = 'B';
pub const MARKER_STYLE_DELETED: char = 'C';
pub const MARKER_STYLE_MODIFIED: char = 'D';
pub const MARKER_STYLE_ERROR: char = 'E';
pub const MARKER_STYLE_WARNING: char = 'F';
pub const MARKER_STYLE_INFO: char = 'G';

/// Maps syntect RGB colors to FLTK style characters ('A', 'B', 'C', ...).
/// Also supports line markers with background colors ('a', 'b', 'c', ...).
/// Dynamically builds a StyleTableEntryExt table as new colors are encountered.
pub struct StyleMap {
    color_to_char: HashMap<(u8, u8, u8), char>,
    entries: Vec<StyleTableEntryExt>,
    font: Font,
    font_size: i32,
    /// Number of entries at last `reset_changed()` call.
    last_entry_count: usize,
    /// Background color for the editor (from theme)
    theme_bgcolor: Color,
    /// Foreground color for the editor (from theme)
    theme_fgcolor: Color,
    /// Whether dark mode is active
    is_dark: bool,
}

impl StyleMap {
    pub fn new(font: Font, font_size: i32) -> Self {
        let mut map = Self {
            color_to_char: HashMap::new(),
            entries: Vec::new(),
            font,
            font_size,
            last_entry_count: 0,
            theme_bgcolor: Color::Background,
            theme_fgcolor: Color::Foreground,
            is_dark: false,
        };
        // Pre-insert 'A' as the default/fallback style (plain text color)
        map.entries.push(StyleTableEntryExt {
            color: Color::Foreground,
            font,
            size: font_size,
            attr: TextAttr::None,
            bgcolor: Color::Background,
        });
        map.color_to_char.insert((0, 0, 0), 'A');

        // Pre-insert marker styles with bgcolor
        map.add_marker_styles();

        map
    }

    /// Add marker styles with background colors
    fn add_marker_styles(&mut self) {
        let fg = self.theme_fgcolor;
        let font = self.font;
        let size = self.font_size;

        // Style 'B' - Added (green background)
        // Use TextAttr::BgColor to enable the bgcolor field
        let added_bg = if self.is_dark {
            Color::from_rgb(20, 60, 20)
        } else {
            Color::from_rgb(200, 255, 200)
        };
        self.entries.push(StyleTableEntryExt {
            color: fg,
            font,
            size,
            attr: TextAttr::BgColor,
            bgcolor: added_bg,
        });

        // Style 'C' - Deleted (red background)
        let deleted_bg = if self.is_dark {
            Color::from_rgb(60, 20, 20)
        } else {
            Color::from_rgb(255, 200, 200)
        };
        self.entries.push(StyleTableEntryExt {
            color: fg,
            font,
            size,
            attr: TextAttr::BgColor,
            bgcolor: deleted_bg,
        });

        // Style 'D' - Modified (yellow background)
        let modified_bg = if self.is_dark {
            Color::from_rgb(60, 50, 10)
        } else {
            Color::from_rgb(255, 255, 200)
        };
        self.entries.push(StyleTableEntryExt {
            color: fg,
            font,
            size,
            attr: TextAttr::BgColor,
            bgcolor: modified_bg,
        });

        // Style 'E' - Error (red background, more intense)
        let error_bg = if self.is_dark {
            Color::from_rgb(85, 0, 0)
        } else {
            Color::from_rgb(255, 180, 180)
        };
        self.entries.push(StyleTableEntryExt {
            color: fg,
            font,
            size,
            attr: TextAttr::BgColor,
            bgcolor: error_bg,
        });

        // Style 'F' - Warning (orange background)
        let warning_bg = if self.is_dark {
            Color::from_rgb(70, 40, 0)
        } else {
            Color::from_rgb(255, 230, 180)
        };
        self.entries.push(StyleTableEntryExt {
            color: fg,
            font,
            size,
            attr: TextAttr::BgColor,
            bgcolor: warning_bg,
        });

        // Style 'G' - Info (blue background)
        let info_bg = if self.is_dark {
            Color::from_rgb(0, 30, 60)
        } else {
            Color::from_rgb(200, 220, 255)
        };
        self.entries.push(StyleTableEntryExt {
            color: fg,
            font,
            size,
            attr: TextAttr::BgColor,
            bgcolor: info_bg,
        });
    }

    /// Get the style character for a syntect color, inserting a new entry if needed.
    /// Note: Indices 0-6 are reserved ('A' default, 'B'-'G' markers), syntax starts at 'H' (index 7)
    pub fn get_or_insert(&mut self, color: SyntectColor) -> char {
        let key = (color.r, color.g, color.b);
        if let Some(&ch) = self.color_to_char.get(&key) {
            return ch;
        }

        let idx = self.entries.len();
        // Style table layout:
        // 0 = 'A' (default/plain text)
        // 1-6 = 'B'-'G' (marker styles with bgcolor)
        // 7+ = 'H'-'Z' (syntax highlighting colors)
        if idx >= 26 {
            // Fallback to last available style
            return 'Z';
        }
        let ch = (b'A' + idx as u8) as char;
        self.entries.push(StyleTableEntryExt {
            color: Color::from_rgb(color.r, color.g, color.b),
            font: self.font,
            size: self.font_size,
            attr: TextAttr::None,
            bgcolor: self.theme_bgcolor,
        });
        self.color_to_char.insert(key, ch);
        ch
    }

    /// Get the style character for a marker type (static, for semantic colors only)
    pub fn marker_style_char(color: &AnnotationColor) -> char {
        match color {
            AnnotationColor::Added => MARKER_STYLE_ADDED,
            AnnotationColor::Deleted => MARKER_STYLE_DELETED,
            AnnotationColor::Modified => MARKER_STYLE_MODIFIED,
            AnnotationColor::Error => MARKER_STYLE_ERROR,
            AnnotationColor::Warning => MARKER_STYLE_WARNING,
            AnnotationColor::Info | AnnotationColor::Hint => MARKER_STYLE_INFO,
            AnnotationColor::Rgb(_, _, _) => MARKER_STYLE_INFO, // Use get_or_insert_marker_rgb for actual RGB
        }
    }

    /// Get or insert a marker style for an RGB color.
    /// Returns the style character for the bgcolor marker.
    /// Limited to ~10 custom colors to avoid exhausting the style table.
    pub fn get_or_insert_marker_rgb(&mut self, r: u8, g: u8, b: u8) -> char {
        // Check if we already have this RGB as a bgcolor marker
        // We search through existing entries for a matching bgcolor

        // Check existing entries for this bgcolor (after the semantic markers)
        // We start checking from entry 7 (after A and B-G markers)
        for (idx, entry) in self.entries.iter().enumerate().skip(7) {
            if entry.attr == TextAttr::BgColor {
                // Extract RGB from the Color
                let (cr, cg, cb) = self.color_to_rgb(entry.bgcolor);
                if cr == r && cg == g && cb == b {
                    return (b'A' + idx as u8) as char;
                }
            }
        }

        // Need to insert a new entry
        let idx = self.entries.len();
        if idx >= 26 {
            // Out of style slots, fallback to Info
            return MARKER_STYLE_INFO;
        }

        let ch = (b'A' + idx as u8) as char;
        self.entries.push(StyleTableEntryExt {
            color: self.theme_fgcolor,
            font: self.font,
            size: self.font_size,
            attr: TextAttr::BgColor,
            bgcolor: Color::from_rgb(r, g, b),
        });

        // We don't add to color_to_char since that's for syntax colors
        ch
    }

    /// Extract RGB values from an FLTK Color
    fn color_to_rgb(&self, color: Color) -> (u8, u8, u8) {
        let (r, g, b) = color.to_rgb();
        (r, g, b)
    }

    /// Get the style table entries for FLTK's set_highlight_data_ext.
    pub fn entries(&self) -> &[StyleTableEntryExt] {
        &self.entries
    }

    /// Clear all mappings (used on theme change).
    pub fn clear(&mut self) {
        self.color_to_char.clear();
        self.entries.clear();
        // Re-insert default 'A'
        self.entries.push(StyleTableEntryExt {
            color: self.theme_fgcolor,
            font: self.font,
            size: self.font_size,
            attr: TextAttr::None,
            bgcolor: self.theme_bgcolor,
        });
        self.color_to_char.insert((0, 0, 0), 'A');
        // Re-add marker styles
        self.add_marker_styles();
    }

    /// Returns true if new style entries were added since the last `reset_changed()`.
    pub fn has_new_entries(&self) -> bool {
        self.entries.len() > self.last_entry_count
    }

    /// Mark current entry count as the baseline for `has_new_entries()`.
    pub fn reset_changed(&mut self) {
        self.last_entry_count = self.entries.len();
    }

    /// Update font info for all entries.
    pub fn update_font(&mut self, font: Font, size: i32) {
        self.font = font;
        self.font_size = size;
        for entry in &mut self.entries {
            entry.font = font;
            entry.size = size;
        }
    }

    /// Update theme colors (foreground and background) for the style map.
    /// Call this before `clear()` when switching themes.
    pub fn set_theme_colors(&mut self, fg: Color, bg: Color, is_dark: bool) {
        self.theme_fgcolor = fg;
        self.theme_bgcolor = bg;
        self.is_dark = is_dark;
    }
}
