mod highlighter;
mod style_map;

use std::path::Path;

use fltk::enums::Font;
use fltk::text::StyleTableEntry;
use syntect::highlighting::{HighlightState, Highlighter, HighlightIterator, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

use highlighter::LinesWithEndings;
use style_map::StyleMap;

use super::document::DocumentId;

const DARK_THEME: &str = "base16-ocean.dark";
const LIGHT_THEME: &str = "base16-ocean.light";

const CHUNK_SIZE: usize = 2000;

struct ChunkedState {
    doc_id: DocumentId,
    next_line: usize,
    byte_offset: usize,
    parse_state: ParseState,
    highlight_state: HighlightState,
    parse_states: Vec<ParseState>,
    highlight_states: Vec<HighlightState>,
    syntax_name: String,
}

/// Output from processing one chunk of lines.
pub struct ChunkOutput {
    pub byte_start: usize,
    pub style_chars: String,
    pub done: bool,
    pub final_parse_states: Option<Vec<ParseState>>,
    pub final_highlight_states: Option<Vec<HighlightState>>,
}

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme_name: String,
    style_map: StyleMap,
    chunked: Option<ChunkedState>,
}

/// Result of a full highlight operation.
pub struct FullHighlightResult {
    pub style_string: String,
    pub parse_states: Vec<ParseState>,
    pub highlight_states: Vec<HighlightState>,
}

/// Result of an incremental highlight operation.
pub struct IncrementalHighlightResult {
    pub byte_start: usize,
    pub style_chars: String,
    pub parse_states: Vec<ParseState>,
    pub highlight_states: Vec<HighlightState>,
}

impl SyntaxHighlighter {
    pub fn new(is_dark: bool, font: Font, font_size: i32) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme_name = if is_dark { DARK_THEME } else { LIGHT_THEME }.to_string();
        let style_map = StyleMap::new(font, font_size);

        Self {
            syntax_set,
            theme_set,
            theme_name,
            style_map,
            chunked: None,
        }
    }

    /// Detect the syntax for a file path based on extension.
    pub fn detect_syntax(&self, file_path: &str) -> Option<String> {
        let path = Path::new(file_path);
        let ext = path.extension()?.to_str()?;
        let syntax = self.syntax_set.find_syntax_by_extension(ext)?;
        if syntax.name == "Plain Text" {
            return None;
        }
        Some(syntax.name.clone())
    }

    /// Perform a full highlight. Returns style string + cached states.
    pub fn highlight_full(
        &mut self,
        text: &str,
        syntax_name: &str,
    ) -> FullHighlightResult {
        let syntax = match self.syntax_set.find_syntax_by_name(syntax_name) {
            Some(s) => s.clone(),
            None => return FullHighlightResult {
                style_string: make_default_style(text),
                parse_states: Vec::new(),
                highlight_states: Vec::new(),
            },
        };
        let result = highlighter::highlight_full(
            text,
            &syntax,
            &self.syntax_set,
            &self.theme_set,
            &self.theme_name,
            &mut self.style_map,
        );
        FullHighlightResult {
            style_string: result.style_string,
            parse_states: result.parse_states,
            highlight_states: result.highlight_states,
        }
    }

    /// Incremental re-highlight from a given edit line.
    /// Returns only the changed style chars and their byte offset.
    pub fn highlight_incremental(
        &mut self,
        text: &str,
        edit_line: usize,
        old_parse_states: &[ParseState],
        old_highlight_states: &[HighlightState],
        syntax_name: &str,
    ) -> IncrementalHighlightResult {
        let syntax = match self.syntax_set.find_syntax_by_name(syntax_name) {
            Some(s) => s.clone(),
            None => return IncrementalHighlightResult {
                byte_start: 0,
                style_chars: make_default_style(text),
                parse_states: Vec::new(),
                highlight_states: Vec::new(),
            },
        };
        let result = highlighter::highlight_incremental(
            text,
            edit_line,
            old_parse_states,
            old_highlight_states,
            &syntax,
            &self.syntax_set,
            &self.theme_set,
            &self.theme_name,
            &mut self.style_map,
        );
        IncrementalHighlightResult {
            byte_start: result.byte_start,
            style_chars: result.style_chars,
            parse_states: result.parse_states,
            highlight_states: result.highlight_states,
        }
    }

    /// Switch theme for dark/light mode. Clears the style map.
    pub fn set_dark_mode(&mut self, is_dark: bool) {
        self.theme_name = if is_dark { DARK_THEME } else { LIGHT_THEME }.to_string();
        self.style_map.clear();
    }

    /// Update the font used in style table entries.
    pub fn set_font(&mut self, font: Font, size: i32) {
        self.style_map.update_font(font, size);
    }

    /// Get the style table for FLTK's set_highlight_data.
    pub fn style_table(&self) -> Vec<StyleTableEntry> {
        self.style_map.entries().to_vec()
    }

    /// Begin chunked highlighting for a large file.
    pub fn start_chunked(&mut self, doc_id: DocumentId, text: &str, syntax_name: &str) {
        let syntax = match self.syntax_set.find_syntax_by_name(syntax_name) {
            Some(s) => s.clone(),
            None => return,
        };
        let theme = &self.theme_set.themes[&self.theme_name];
        let highlighter = Highlighter::new(theme);
        let parse_state = ParseState::new(&syntax);
        let highlight_state = HighlightState::new(&highlighter, ScopeStack::new());

        self.chunked = Some(ChunkedState {
            doc_id,
            next_line: 0,
            byte_offset: 0,
            parse_state,
            highlight_state,
            parse_states: Vec::new(),
            highlight_states: Vec::new(),
            syntax_name: syntax_name.to_string(),
        });

        // Count lines to pre-allocate
        let line_count = LinesWithEndings::new(text).count();
        if let Some(ref mut cs) = self.chunked {
            cs.parse_states.reserve(line_count);
            cs.highlight_states.reserve(line_count);
        }
    }

    /// Process the next chunk of lines. Returns None if no chunked operation is active.
    pub fn process_chunk(&mut self, text: &str) -> Option<ChunkOutput> {
        let mut cs = self.chunked.take()?;

        if self.syntax_set.find_syntax_by_name(&cs.syntax_name).is_none() {
            return None;
        }
        let theme = &self.theme_set.themes[&self.theme_name];
        let highlighter = Highlighter::new(theme);

        let byte_start = cs.byte_offset;
        let mut style_chars = String::new();
        let mut lines_processed = 0;

        // Skip to the line we need to start from
        let mut lines_iter = LinesWithEndings::new(text);
        let mut skip_count = 0;
        while skip_count < cs.next_line {
            if lines_iter.next().is_none() {
                break;
            }
            skip_count += 1;
        }

        for line in lines_iter {
            if lines_processed >= CHUNK_SIZE {
                break;
            }

            cs.parse_states.push(cs.parse_state.clone());
            cs.highlight_states.push(cs.highlight_state.clone());

            let ops = cs.parse_state.parse_line(line, &self.syntax_set).unwrap_or_default();
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
                final_parse_states: Some(cs.parse_states),
                final_highlight_states: Some(cs.highlight_states),
            })
        } else {
            self.chunked = Some(cs);
            Some(ChunkOutput {
                byte_start,
                style_chars,
                done: false,
                final_parse_states: None,
                final_highlight_states: None,
            })
        }
    }

    /// Cancel an in-progress chunked highlight. Returns partial states if active.
    pub fn cancel_chunked(&mut self) -> Option<(Vec<ParseState>, Vec<HighlightState>)> {
        self.chunked.take().map(|cs| (cs.parse_states, cs.highlight_states))
    }

    /// Get the document ID of the active chunked operation, if any.
    pub fn chunked_doc_id(&self) -> Option<DocumentId> {
        self.chunked.as_ref().map(|cs| cs.doc_id)
    }
}

fn make_default_style(text: &str) -> String {
    std::iter::repeat('A').take(text.len()).collect()
}
