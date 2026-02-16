use std::collections::HashMap;

use fltk::enums::{Color, Font};
use fltk::text::StyleTableEntry;
use syntect::highlighting::Color as SyntectColor;

/// Maps syntect RGB colors to FLTK style characters ('A', 'B', 'C', ...).
/// Dynamically builds a StyleTableEntry table as new colors are encountered.
pub struct StyleMap {
    color_to_char: HashMap<(u8, u8, u8), char>,
    entries: Vec<StyleTableEntry>,
    font: Font,
    font_size: i32,
}

impl StyleMap {
    pub fn new(font: Font, font_size: i32) -> Self {
        let mut map = Self {
            color_to_char: HashMap::new(),
            entries: Vec::new(),
            font,
            font_size,
        };
        // Pre-insert 'A' as the default/fallback style (plain text color)
        map.entries.push(StyleTableEntry {
            color: Color::Foreground,
            font,
            size: font_size,
        });
        map.color_to_char.insert((0, 0, 0), 'A');
        map
    }

    /// Get the style character for a syntect color, inserting a new entry if needed.
    pub fn get_or_insert(&mut self, color: SyntectColor) -> char {
        let key = (color.r, color.g, color.b);
        if let Some(&ch) = self.color_to_char.get(&key) {
            return ch;
        }

        let idx = self.entries.len();
        // FLTK style chars go 'A'..'Z' then beyond if needed, but 26 colors is plenty
        if idx >= 26 {
            return (b'A' + 25) as char;
        }
        let ch = (b'A' + idx as u8) as char;
        self.entries.push(StyleTableEntry {
            color: Color::from_rgb(color.r, color.g, color.b),
            font: self.font,
            size: self.font_size,
        });
        self.color_to_char.insert(key, ch);
        ch
    }

    /// Get the style table entries for FLTK's set_highlight_data.
    pub fn entries(&self) -> &[StyleTableEntry] {
        &self.entries
    }

    /// Clear all mappings (used on theme change).
    pub fn clear(&mut self) {
        self.color_to_char.clear();
        self.entries.clear();
        // Re-insert default 'A'
        self.entries.push(StyleTableEntry {
            color: Color::Foreground,
            font: self.font,
            size: self.font_size,
        });
        self.color_to_char.insert((0, 0, 0), 'A');
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
}
