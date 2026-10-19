pub mod checkpoint;
mod highlighter;
pub mod style_map;

use std::path::Path;

use fltk::enums::Font;
use fltk::text::StyleTableEntryExt;
use syntect::highlighting::{HighlightIterator, HighlightState, Highlighter, ThemeSet};
use syntect::parsing::{
    ParseState, ScopeStack, SyntaxDefinition, SyntaxReference, SyntaxSet, SyntaxSetBuilder,
};

use checkpoint::{CHECKPOINT_INTERVAL, SparseCheckpoints};
use highlighter::LinesWithEndings;
use style_map::StyleMap;

use crate::app::domain::document::DocumentId;
use crate::app::domain::settings::SyntaxTheme;

const CHUNK_SIZE: usize = 2000;

const TOML_SYNTAX: &str = include_str!("../../../../assets/syntaxes/TOML.sublime-syntax");

struct ChunkedState {
    doc_id: DocumentId,
    next_line: usize,
    byte_offset: usize,
    parse_state: ParseState,
    highlight_state: HighlightState,
    checkpoints: SparseCheckpoints,
    syntax_name: String,
    /// Cached copy of the document text, taken once at start_chunked.
    text: String,
}

/// Output from processing one chunk of lines.
pub struct ChunkOutput {
    pub byte_start: usize,
    pub style_chars: String,
    pub done: bool,
    pub final_checkpoints: Option<SparseCheckpoints>,
}

/// Lazily-loaded syntect internals. None until first highlight request.
struct SyntaxHighlighterInner {
    syntax_set: SyntaxSet,
    toml_syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    chunked: Option<ChunkedState>,
}

impl SyntaxHighlighterInner {
    /// Find a syntax by name across both sets (TOML first, then defaults).
    fn find_syntax_by_name(&self, name: &str) -> Option<&SyntaxReference> {
        self.toml_syntax_set
            .find_syntax_by_name(name)
            .or_else(|| self.syntax_set.find_syntax_by_name(name))
    }

    /// Find a syntax by file extension across both sets.
    fn find_syntax_by_extension(&self, ext: &str) -> Option<&SyntaxReference> {
        self.toml_syntax_set
            .find_syntax_by_extension(ext)
            .or_else(|| self.syntax_set.find_syntax_by_extension(ext))
    }

    /// Get the SyntaxSet that owns a given syntax (needed for parse_line).
    fn syntax_set_for(&self, syntax_name: &str) -> &SyntaxSet {
        if self
            .toml_syntax_set
            .find_syntax_by_name(syntax_name)
            .is_some()
        {
            &self.toml_syntax_set
        } else {
            &self.syntax_set
        }
    }
}

fn init_inner() -> SyntaxHighlighterInner {
    // Load pre-compiled defaults AS-IS (no builder conversion = no regex recompilation)
    let syntax_set = SyntaxSet::load_defaults_newlines();

    // Build a tiny separate set with ONLY the TOML syntax
    let mut toml_builder = SyntaxSetBuilder::new();
    if let Ok(toml_def) = SyntaxDefinition::load_from_str(TOML_SYNTAX, true, None) {
        toml_builder.add(toml_def);
    }
    let toml_syntax_set = toml_builder.build();

    let theme_set = ThemeSet::load_defaults();

    SyntaxHighlighterInner {
        syntax_set,
        toml_syntax_set,
        theme_set,
        chunked: None,
    }
}

pub struct SyntaxHighlighter {
    inner: Option<SyntaxHighlighterInner>,
    theme: SyntaxTheme,
    theme_name: String,
    style_map: StyleMap,
    font: Font,
    font_size: i32,
}

/// Result of a full highlight operation.
pub struct FullHighlightResult {
    pub style_string: String,
    pub checkpoints: SparseCheckpoints,
}

/// Result of an incremental highlight operation.
pub struct IncrementalHighlightResult {
    pub byte_start: usize,
    pub style_chars: String,
}

impl SyntaxHighlighter {
    pub fn new(theme: SyntaxTheme, font: Font, font_size: i32) -> Self {
        let theme_name = theme.theme_key().to_string();
        let style_map = StyleMap::new(font, font_size);

        Self {
            inner: None,
            theme,
            theme_name,
            style_map,
            font,
            font_size,
        }
    }

    /// Ensure syntect is loaded, returning a mutable reference.
    fn inner_mut(&mut self) -> &mut SyntaxHighlighterInner {
        self.inner.get_or_insert_with(init_inner)
    }

    /// Ensure syntect is loaded (called before methods that need split borrows).
    fn ensure_loaded(&mut self) {
        if self.inner.is_none() {
            self.inner = Some(init_inner());
        }
    }

    /// Detect the syntax for a file path based on extension.
    pub fn detect_syntax(&mut self, file_path: &str) -> Option<String> {
        let path = Path::new(file_path);
        let ext = path.extension()?.to_str()?;
        let inner = self.inner_mut();
        let syntax = inner.find_syntax_by_extension(ext)?;
        if syntax.name == "Plain Text" {
            return None;
        }
        Some(syntax.name.clone())
    }

    /// Perform a full highlight. Returns style string + sparse checkpoints.
    pub fn highlight_full(&mut self, text: &str, syntax_name: &str) -> FullHighlightResult {
        self.ensure_loaded();
        let inner = self.inner.as_ref().unwrap();
        let syntax = match inner.find_syntax_by_name(syntax_name) {
            Some(s) => s.clone(),
            None => {
                return FullHighlightResult {
                    style_string: make_default_style(text),
                    checkpoints: SparseCheckpoints::new(),
                };
            }
        };
        let syntax_set = inner.syntax_set_for(syntax_name);
        let result = highlighter::highlight_full(
            text,
            &syntax,
            syntax_set,
            &inner.theme_set,
            &self.theme_name,
            &mut self.style_map,
        );
        FullHighlightResult {
            style_string: result.style_string,
            checkpoints: result.checkpoints,
        }
    }

    /// Incremental re-highlight from a given edit line.
    /// Modifies checkpoints in place and returns only the changed style chars.
    pub fn highlight_incremental(
        &mut self,
        text: &str,
        edit_line: usize,
        checkpoints: &mut SparseCheckpoints,
        syntax_name: &str,
    ) -> IncrementalHighlightResult {
        self.ensure_loaded();
        let inner = self.inner.as_ref().unwrap();
        let syntax = match inner.find_syntax_by_name(syntax_name) {
            Some(s) => s.clone(),
            None => {
                return IncrementalHighlightResult {
                    byte_start: 0,
                    style_chars: make_default_style(text),
                };
            }
        };
        let syntax_set = inner.syntax_set_for(syntax_name);
        let result = highlighter::highlight_incremental(
            text,
            edit_line,
            checkpoints,
            &syntax,
            syntax_set,
            &inner.theme_set,
            &self.theme_name,
            &mut self.style_map,
        );
        IncrementalHighlightResult {
            byte_start: result.byte_start,
            style_chars: result.style_chars,
        }
    }

    /// Switch to a specific theme. Clears the style map and updates theme colors.
    pub fn set_theme(&mut self, theme: SyntaxTheme) {
        self.theme = theme;
        self.theme_name = theme.theme_key().to_string();

        let bg = self.theme_background();
        let fg = self.theme_foreground();
        let is_dark = (bg.0 as u32 + bg.1 as u32 + bg.2 as u32) / 3 < 128;

        self.style_map.set_theme_colors(
            fltk::enums::Color::from_rgb(fg.0, fg.1, fg.2),
            fltk::enums::Color::from_rgb(bg.0, bg.1, bg.2),
            is_dark,
        );
        self.style_map.clear();
    }

    /// Get the background color of the current theme as RGB tuple.
    /// Uses hardcoded values — no syntect needed.
    pub fn theme_background(&self) -> (u8, u8, u8) {
        self.theme.background()
    }

    /// Get the foreground color of the current theme as RGB tuple.
    /// Uses hardcoded values — no syntect needed.
    pub fn theme_foreground(&self) -> (u8, u8, u8) {
        self.theme.foreground()
    }

    pub fn font(&self) -> Font {
        self.font
    }

    pub fn font_size(&self) -> i32 {
        self.font_size
    }

    /// Update the font used in style table entries.
    pub fn set_font(&mut self, font: Font, size: i32) {
        self.font = font;
        self.font_size = size;
        self.style_map.update_font(font, size);
    }

    /// Get the style table for FLTK's set_highlight_data_ext.
    pub fn style_table(&self) -> Vec<StyleTableEntryExt> {
        self.style_map.entries().to_vec()
    }

    /// Returns true if new style entries were added since the last reset.
    pub fn style_table_changed(&self) -> bool {
        self.style_map.has_new_entries()
    }

    /// Mark the style table as up-to-date (call after set_highlight_data).
    pub fn reset_style_table_changed(&mut self) {
        self.style_map.reset_changed();
    }

    /// Get or insert a marker style for an RGB color.
    /// Returns the style character for the bgcolor marker.
    pub fn get_or_insert_marker_rgb(&mut self, r: u8, g: u8, b: u8) -> char {
        self.style_map.get_or_insert_marker_rgb(r, g, b)
    }

    /// Begin chunked highlighting for a large file.
    /// Takes ownership of the text to avoid re-copying it on every chunk.
    pub fn start_chunked(&mut self, doc_id: DocumentId, text: String, syntax_name: &str) {
        self.ensure_loaded();
        let inner = self.inner.as_mut().unwrap();
        let syntax = match inner.find_syntax_by_name(syntax_name) {
            Some(s) => s.clone(),
            None => return,
        };
        let theme = highlighter::resolve_theme(&inner.theme_set, &self.theme_name);
        let highlighter = Highlighter::new(theme);
        let parse_state = ParseState::new(&syntax);
        let highlight_state = HighlightState::new(&highlighter, ScopeStack::new());

        let line_count = LinesWithEndings::new(&text).count();

        let mut chunked = ChunkedState {
            doc_id,
            next_line: 0,
            byte_offset: 0,
            parse_state,
            highlight_state,
            checkpoints: SparseCheckpoints::with_capacity(line_count),
            syntax_name: syntax_name.to_string(),
            text,
        };
        chunked.checkpoints.line_count = line_count;
        inner.chunked = Some(chunked);
    }

    /// Process the next chunk of lines. Returns None if no chunked operation is active.
    pub fn process_chunk(&mut self) -> Option<ChunkOutput> {
        let inner = self.inner.as_mut()?;
        let mut cs = inner.chunked.take()?;

        let syntax_set = inner.syntax_set_for(&cs.syntax_name);
        syntax_set.find_syntax_by_name(&cs.syntax_name)?;
        let theme = highlighter::resolve_theme(&inner.theme_set, &self.theme_name);
        let highlighter = Highlighter::new(theme);

        let byte_start = cs.byte_offset;
        let mut style_chars = String::new();
        let mut lines_processed = 0;

        // Start directly from byte_offset — no need to skip lines from the beginning
        for line in LinesWithEndings::new(&cs.text[cs.byte_offset..]) {
            if lines_processed >= CHUNK_SIZE {
                break;
            }

            // Only save checkpoint at interval boundaries
            if cs.next_line % CHECKPOINT_INTERVAL == 0 {
                cs.checkpoints
                    .push(cs.parse_state.clone(), cs.highlight_state.clone());
            }

            let ops = cs
                .parse_state
                .parse_line(line, syntax_set)
                .unwrap_or_default();
            let iter = HighlightIterator::new(&mut cs.highlight_state, &ops, line, &highlighter);
            for (style, piece) in iter {
                let ch = self.style_map.get_or_insert(style.foreground);
                for _ in 0..piece.len() {
                    style_chars.push(ch);
                }
            }

            cs.byte_offset += line.len();
            cs.next_line += 1;
            lines_processed += 1;
        }

        // Check if we're done (no more lines)
        let done = lines_processed < CHUNK_SIZE;

        if done {
            Some(ChunkOutput {
                byte_start,
                style_chars,
                done: true,
                final_checkpoints: Some(cs.checkpoints),
            })
        } else {
            if let Some(inner) = self.inner.as_mut() {
                inner.chunked = Some(cs);
            }
            Some(ChunkOutput {
                byte_start,
                style_chars,
                done: false,
                final_checkpoints: None,
            })
        }
    }

    /// Cancel an in-progress chunked highlight. Returns partial checkpoints if active.
    pub fn cancel_chunked(&mut self) -> Option<SparseCheckpoints> {
        self.inner.as_mut()?.chunked.take().map(|cs| cs.checkpoints)
    }

    /// Get the document ID of the active chunked operation, if any.
    pub fn chunked_doc_id(&self) -> Option<DocumentId> {
        self.inner.as_ref()?.chunked.as_ref().map(|cs| cs.doc_id)
    }
}

fn make_default_style(text: &str) -> String {
    std::iter::repeat_n('A', text.len()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- T0033: a theme name missing from the ThemeSet must not panic ---

    // Every highlight entry point indexed `theme_set.themes[&self.theme_name]`,
    // which panics on an unknown key. Force a name that no ThemeSet contains
    // (as a new SyntaxTheme variant or a user-supplied theme would) and drive
    // each path: all must fall back to a default theme and still produce styles.
    fn highlighter_with_missing_theme() -> SyntaxHighlighter {
        let mut h = SyntaxHighlighter::new(SyntaxTheme::Base16OceanDark, Font::Courier, 14);
        h.theme_name = "no-such-theme".to_string();
        h
    }

    const SRC: &str = "fn main() {\n    let x = 1;\n}\n";

    #[test]
    fn highlight_full_with_missing_theme_falls_back() {
        let mut h = highlighter_with_missing_theme();
        let result = h.highlight_full(SRC, "Rust");
        assert_eq!(result.style_string.len(), SRC.len());
    }

    #[test]
    fn highlight_incremental_with_missing_theme_falls_back() {
        let mut h = highlighter_with_missing_theme();
        let mut checkpoints = SparseCheckpoints::new();
        let result = h.highlight_incremental(SRC, 0, &mut checkpoints, "Rust");
        assert_eq!(result.byte_start + result.style_chars.len(), SRC.len());
    }

    #[test]
    fn chunked_highlight_with_missing_theme_falls_back() {
        let mut h = highlighter_with_missing_theme();
        h.start_chunked(DocumentId(1), SRC.to_string(), "Rust");
        let chunk = h.process_chunk().expect("chunked highlight is active");
        assert!(chunk.done);
        assert_eq!(chunk.style_chars.len(), SRC.len());
    }

    // Last rungs of the fallback chain: an empty ThemeSet (no requested theme,
    // no default key, nothing to pick) still yields a usable theme.
    #[test]
    fn resolve_theme_on_empty_set_uses_builtin_default() {
        let empty = ThemeSet::default();
        let theme = highlighter::resolve_theme(&empty, "base16-ocean.dark");
        assert!(theme.scopes.is_empty());
    }

    #[test]
    fn resolve_theme_prefers_the_requested_theme() {
        let set = ThemeSet::load_defaults();
        let theme = highlighter::resolve_theme(&set, "Solarized (light)");
        assert_eq!(theme.name.as_deref(), Some("Solarized (light)"));
    }

    // The fallback must stay a safety net, not hide a typo: every theme the UI
    // offers has to exist in the ThemeSet we actually load.
    #[test]
    fn every_syntax_theme_key_is_in_the_default_theme_set() {
        let set = ThemeSet::load_defaults();
        for theme in SyntaxTheme::all() {
            assert!(
                set.themes.contains_key(theme.theme_key()),
                "{theme:?} maps to missing key {:?}",
                theme.theme_key()
            );
        }
    }
}
