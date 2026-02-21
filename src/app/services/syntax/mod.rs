pub mod checkpoint;
mod highlighter;
mod style_map;

use std::path::Path;

use fltk::enums::Font;
use fltk::text::StyleTableEntry;
use syntect::highlighting::{HighlightState, Highlighter, HighlightIterator, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

use checkpoint::{SparseCheckpoints, CHECKPOINT_INTERVAL};
use highlighter::LinesWithEndings;
use style_map::StyleMap;

use crate::app::domain::document::DocumentId;
use crate::app::domain::settings::SyntaxTheme;

const CHUNK_SIZE: usize = 2000;

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
    pub checkpoints: SparseCheckpoints,
}

/// Result of an incremental highlight operation.
pub struct IncrementalHighlightResult {
    pub byte_start: usize,
    pub style_chars: String,
}

impl SyntaxHighlighter {
    pub fn new(theme: SyntaxTheme, font: Font, font_size: i32) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme_name = theme.theme_key().to_string();
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

    /// Perform a full highlight. Returns style string + sparse checkpoints.
    pub fn highlight_full(
        &mut self,
        text: &str,
        syntax_name: &str,
    ) -> FullHighlightResult {
        let syntax = match self.syntax_set.find_syntax_by_name(syntax_name) {
            Some(s) => s.clone(),
            None => return FullHighlightResult {
                style_string: make_default_style(text),
                checkpoints: SparseCheckpoints::new(),
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
        let syntax = match self.syntax_set.find_syntax_by_name(syntax_name) {
            Some(s) => s.clone(),
            None => return IncrementalHighlightResult {
                byte_start: 0,
                style_chars: make_default_style(text),
            },
        };
        let result = highlighter::highlight_incremental(
            text,
            edit_line,
            checkpoints,
            &syntax,
            &self.syntax_set,
            &self.theme_set,
            &self.theme_name,
            &mut self.style_map,
        );
        IncrementalHighlightResult {
            byte_start: result.byte_start,
            style_chars: result.style_chars,
        }
    }

    /// Switch to a specific theme. Clears the style map.
    pub fn set_theme(&mut self, theme: SyntaxTheme) {
        self.theme_name = theme.theme_key().to_string();
        self.style_map.clear();
    }

    /// Get the background color of the current theme as RGB tuple.
    pub fn theme_background(&self) -> (u8, u8, u8) {
        if let Some(theme) = self.theme_set.themes.get(&self.theme_name)
            && let Some(bg) = theme.settings.background
        {
            return (bg.r, bg.g, bg.b);
        }
        // Fallback to white
        (255, 255, 255)
    }

    /// Get the foreground color of the current theme as RGB tuple.
    pub fn theme_foreground(&self) -> (u8, u8, u8) {
        if let Some(theme) = self.theme_set.themes.get(&self.theme_name)
            && let Some(fg) = theme.settings.foreground
        {
            return (fg.r, fg.g, fg.b);
        }
        // Fallback to black
        (0, 0, 0)
    }

    /// Update the font used in style table entries.
    pub fn set_font(&mut self, font: Font, size: i32) {
        self.style_map.update_font(font, size);
    }

    /// Get the style table for FLTK's set_highlight_data.
    pub fn style_table(&self) -> Vec<StyleTableEntry> {
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

    /// Begin chunked highlighting for a large file.
    /// Takes ownership of the text to avoid re-copying it on every chunk.
    pub fn start_chunked(&mut self, doc_id: DocumentId, text: String, syntax_name: &str) {
        let syntax = match self.syntax_set.find_syntax_by_name(syntax_name) {
            Some(s) => s.clone(),
            None => return,
        };
        let theme = &self.theme_set.themes[&self.theme_name];
        let highlighter = Highlighter::new(theme);
        let parse_state = ParseState::new(&syntax);
        let highlight_state = HighlightState::new(&highlighter, ScopeStack::new());

        let line_count = LinesWithEndings::new(&text).count();

        self.chunked = Some(ChunkedState {
            doc_id,
            next_line: 0,
            byte_offset: 0,
            parse_state,
            highlight_state,
            checkpoints: SparseCheckpoints::with_capacity(line_count),
            syntax_name: syntax_name.to_string(),
            text,
        });

        if let Some(ref mut cs) = self.chunked {
            cs.checkpoints.line_count = line_count;
        }
    }

    /// Process the next chunk of lines. Returns None if no chunked operation is active.
    pub fn process_chunk(&mut self) -> Option<ChunkOutput> {
        let mut cs = self.chunked.take()?;

        self.syntax_set.find_syntax_by_name(&cs.syntax_name)?;
        let theme = &self.theme_set.themes[&self.theme_name];
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
                cs.checkpoints.push(cs.parse_state.clone(), cs.highlight_state.clone());
            }

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
                final_checkpoints: Some(cs.checkpoints),
            })
        } else {
            self.chunked = Some(cs);
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
        self.chunked.take().map(|cs| cs.checkpoints)
    }

    /// Get the document ID of the active chunked operation, if any.
    pub fn chunked_doc_id(&self) -> Option<DocumentId> {
        self.chunked.as_ref().map(|cs| cs.doc_id)
    }
}

fn make_default_style(text: &str) -> String {
    std::iter::repeat_n('A', text.len()).collect()
}
