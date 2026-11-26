pub mod checkpoint;
mod highlighter;
pub mod style_map;

use std::path::Path;

use fltk::enums::Font;
use fltk::text::StyleTableEntryExt;
use syntect::highlighting::{HighlightState, Highlighter, Theme, ThemeSet};
use syntect::parsing::{
    ParseState, ScopeStack, SyntaxDefinition, SyntaxReference, SyntaxSet, SyntaxSetBuilder,
};

use checkpoint::{CHECKPOINT_INTERVAL, SparseCheckpoints};
use highlighter::LinesWithEndings;
use style_map::StyleMap;

use crate::app::domain::document::DocumentId;
use crate::app::domain::settings::SyntaxTheme;

pub use highlighter::{LONG_LINE_BYTES, is_long_line};

/// A chunk of a chunked highlight ends after this many lines…
const CHUNK_SIZE: usize = 2000;
/// …or once it has handed syntect this many bytes, whichever comes first
/// (T0087), so that one chunk's work is bounded in bytes too. Lines skipped
/// by [`is_long_line`] do not count.
pub const CHUNK_BYTES: usize = 256 * 1024;

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

fn init_inner(custom_themes: &[(String, Theme)]) -> SyntaxHighlighterInner {
    // Load pre-compiled defaults AS-IS (no builder conversion = no regex recompilation)
    let syntax_set = SyntaxSet::load_defaults_newlines();

    // Build a tiny separate set with ONLY the TOML syntax
    let mut toml_builder = SyntaxSetBuilder::new();
    if let Ok(toml_def) = SyntaxDefinition::load_from_str(TOML_SYNTAX, true, None) {
        toml_builder.add(toml_def);
    }
    let toml_syntax_set = toml_builder.build();

    let mut theme_set = ThemeSet::load_defaults();
    for (key, theme) in custom_themes {
        theme_set.themes.insert(key.clone(), theme.clone());
    }

    SyntaxHighlighterInner {
        syntax_set,
        toml_syntax_set,
        theme_set,
        chunked: None,
    }
}

pub struct SyntaxHighlighter {
    inner: Option<SyntaxHighlighterInner>,
    /// Custom themes (T0079), keyed `custom:<id>` like their settings
    /// reference; inserted into the `ThemeSet` beside the built-ins.
    custom_themes: Vec<(String, Theme)>,
    /// The active theme's `ThemeSet` key.
    theme_name: String,
    /// The active theme's editor background and default text color.
    background: (u8, u8, u8),
    foreground: (u8, u8, u8),
    style_map: StyleMap,
    font: Font,
    font_size: i32,
    /// Background of the current-line twins (T0081), from the controller's
    /// `editor_colors().current_line`.
    current_line_bg: (u8, u8, u8),
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
            custom_themes: Vec::new(),
            theme_name,
            background: theme.background(),
            foreground: theme.foreground(),
            style_map,
            font,
            font_size,
            current_line_bg: theme.background(),
        }
    }

    /// Ensure syntect is loaded, returning a mutable reference.
    fn inner_mut(&mut self) -> &mut SyntaxHighlighterInner {
        let custom_themes = &self.custom_themes;
        self.inner.get_or_insert_with(|| init_inner(custom_themes))
    }

    /// Ensure syntect is loaded (called before methods that need split borrows).
    fn ensure_loaded(&mut self) {
        if self.inner.is_none() {
            self.inner = Some(init_inner(&self.custom_themes));
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

    /// Switch to a built-in theme. Clears the style map and updates theme colors.
    pub fn set_theme(&mut self, theme: SyntaxTheme) {
        self.set_theme_key(theme.theme_key(), theme.background(), theme.foreground());
    }

    /// Switch to the theme stored under `key` in the `ThemeSet` (a built-in
    /// key or `custom:<id>`), shown on `background` with default text color
    /// `foreground`. An unknown key falls back softly when highlighting
    /// (`resolve_theme`). Clears the style map.
    pub fn set_theme_key(&mut self, key: &str, background: (u8, u8, u8), foreground: (u8, u8, u8)) {
        self.theme_name = key.to_string();
        self.background = background;
        self.foreground = foreground;
        let is_dark =
            (u32::from(background.0) + u32::from(background.1) + u32::from(background.2)) / 3 < 128;

        self.style_map.set_theme_colors(
            fltk::enums::Color::from_rgb(foreground.0, foreground.1, foreground.2),
            fltk::enums::Color::from_rgb(background.0, background.1, background.2),
            is_dark,
        );
        self.style_map.clear();
    }

    /// Replace the custom themes (`(custom:<id>, theme)` pairs). Takes effect
    /// in the `ThemeSet` at once if syntect is already loaded, else on load.
    pub fn set_custom_themes(&mut self, themes: Vec<(String, Theme)>) {
        if let Some(inner) = self.inner.as_mut() {
            for (key, _) in &self.custom_themes {
                inner.theme_set.themes.remove(key);
            }
            for (key, theme) in &themes {
                inner.theme_set.themes.insert(key.clone(), theme.clone());
            }
        }
        self.custom_themes = themes;
    }

    /// The active theme's `ThemeSet` key.
    pub fn theme_key(&self) -> &str {
        &self.theme_name
    }

    /// Get the background color of the current theme as RGB tuple.
    pub fn theme_background(&self) -> (u8, u8, u8) {
        self.background
    }

    /// Get the foreground color of the current theme as RGB tuple.
    pub fn theme_foreground(&self) -> (u8, u8, u8) {
        self.foreground
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

    /// Get the style table for FLTK's set_highlight_data_ext: the primaries
    /// plus their current-line twins (T0081, `StyleMap::table_with_twins`).
    pub fn style_table(&self) -> Vec<StyleTableEntryExt> {
        let (r, g, b) = self.current_line_bg;
        self.style_map
            .table_with_twins(fltk::enums::Color::from_rgb(r, g, b))
    }

    /// Set the current-line band color used by the twins in `style_table`.
    pub fn set_current_line_bg(&mut self, color: (u8, u8, u8)) {
        self.current_line_bg = color;
    }

    /// Which primary style chars may be twinned on the cursor line.
    pub fn twinnable_styles(&self) -> [bool; crate::app::services::current_line::PRIMARY_STYLES] {
        self.style_map.twinnable()
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
        let mut tokenized = 0;
        // Start directly from byte_offset — no need to skip lines from the beginning
        for (lines_processed, line) in LinesWithEndings::new(&cs.text[cs.byte_offset..]).enumerate()
        {
            if lines_processed >= CHUNK_SIZE || tokenized >= CHUNK_BYTES {
                break;
            }
            if !is_long_line(line) {
                tokenized += line.len();
            }

            // Only save checkpoint at interval boundaries
            if cs.next_line % CHECKPOINT_INTERVAL == 0 {
                cs.checkpoints
                    .push(cs.parse_state.clone(), cs.highlight_state.clone());
            }

            highlighter::highlight_line(
                line,
                &mut cs.parse_state,
                &mut cs.highlight_state,
                &highlighter,
                syntax_set,
                &mut self.style_map,
                &mut style_chars,
            );

            cs.byte_offset += line.len();
            cs.next_line += 1;
        }

        let done = cs.byte_offset >= cs.text.len();

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
    // --- T0087: lines longer than LONG_LINE_BYTES are not tokenized ---

    /// One line of minified-looking JavaScript, longer than
    /// `LONG_LINE_BYTES`, ending with `end` (e.g. "\n").
    fn long_js_line(end: &str) -> String {
        let mut line = String::new();
        while line.len() <= LONG_LINE_BYTES {
            line.push_str("var a=\"x\";function f(b){return b+1}");
        }
        line.push_str(end);
        line
    }

    /// The style chars of line `idx` (line ending included).
    fn line_styles<'a>(text: &str, styles: &'a str, idx: usize) -> &'a str {
        let mut start = 0;
        for (i, line) in LinesWithEndings::new(text).enumerate() {
            if i == idx {
                return &styles[start..start + line.len()];
            }
            start += line.len();
        }
        panic!("no line {idx}");
    }

    fn distinct_styles(styles: &str) -> usize {
        styles
            .chars()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }

    fn js() -> SyntaxHighlighter {
        SyntaxHighlighter::new(SyntaxTheme::Base16OceanDark, Font::Courier, 14)
    }

    /// The chunked path, run to the end: the concatenated style chars and
    /// the number of chunks it took.
    fn chunked_styles(h: &mut SyntaxHighlighter, text: &str, syntax: &str) -> (String, usize) {
        h.start_chunked(DocumentId(1), text.to_string(), syntax);
        let (mut styles, mut chunks) = (String::new(), 0);
        loop {
            let chunk = h.process_chunk().expect("chunked highlight is active");
            assert_eq!(chunk.byte_start, styles.len(), "chunks are contiguous");
            styles.push_str(&chunk.style_chars);
            chunks += 1;
            if chunk.done {
                return (styles, chunks);
            }
        }
    }

    /// A long line in the middle, at the start and at the end (without a
    /// final newline) is one plain run in all three paths, which agree; the
    /// short lines around it are still highlighted.
    #[test]
    fn long_line_is_one_plain_run_in_every_path() {
        let long = long_js_line("\n");
        let cases = [
            (
                format!("var x = 1;\n{long}function g(a) {{ return a; }}\n"),
                1,
            ),
            (format!("{long}function g(a) {{ return a; }}\n"), 0),
            (
                format!("function g(a) {{ return a; }}\n{}", long_js_line("")),
                1,
            ),
        ];
        for (text, long_idx) in cases {
            let mut h = js();
            let full = h.highlight_full(&text, "JavaScript").style_string;
            assert_eq!(full.len(), text.len());
            let plain = line_styles(&text, &full, long_idx);
            assert!(
                plain.bytes().all(|b| b == b'A'),
                "line {long_idx} has {} styles",
                distinct_styles(plain)
            );
            let line_count = LinesWithEndings::new(&text).count();
            for idx in (0..line_count).filter(|&i| i != long_idx) {
                assert!(
                    distinct_styles(line_styles(&text, &full, idx)) > 1,
                    "line {idx} is highlighted"
                );
            }
            let mut checkpoints = SparseCheckpoints::new();
            let incremental = h.highlight_incremental(&text, 0, &mut checkpoints, "JavaScript");
            assert_eq!(incremental.byte_start, 0);
            assert_eq!(incremental.style_chars, full, "incremental == full");
            let (chunked, _) = chunked_styles(&mut h, &text, "JavaScript");
            assert_eq!(chunked, full, "chunked == full");
        }
    }

    /// The skipped line would have closed the block comment opened on the
    /// line before. The parse state carries over unchanged, so the code after
    /// it is still colored as comment: a known, accepted effect.
    #[test]
    fn long_line_skipped_carries_parse_state_over() {
        let text = format!("/* open\n{}var z = 3;\n", long_js_line(" */\n"));
        let mut h = js();
        let full = h.highlight_full(&text, "JavaScript").style_string;
        let comment = line_styles(&text, &full, 0);
        assert_eq!(distinct_styles(comment), 1, "line 0 is all comment");
        let after = line_styles(&text, &full, 2);
        assert!(
            after.chars().all(|c| comment.starts_with(c)),
            "line 2 keeps the comment style: {after:?}"
        );
    }

    /// Apply an incremental result to the old style string.
    fn apply(old: &str, new_len: usize, result: &IncrementalHighlightResult) -> String {
        let mut styles = String::with_capacity(new_len);
        styles.push_str(&old[..result.byte_start]);
        styles.push_str(&result.style_chars);
        if styles.len() < new_len {
            // Converged: the tail is unchanged, shifted by the edit.
            styles.push_str(&old[old.len() - (new_len - styles.len())..]);
        }
        styles
    }

    /// A keystroke on the long line keeps it plain (not re-tokenized); an
    /// edit on a normal line of the same file re-highlights exactly as a
    /// fresh full highlight would.
    #[test]
    fn long_line_edits_keep_it_plain_and_normal_lines_rehighlight() {
        let long = long_js_line("\n");
        let before = format!("var x = 1;\n{long}var y = 2;\n");
        let mut h = js();
        let full = h.highlight_full(&before, "JavaScript");
        let mut checkpoints = full.checkpoints;

        // Type one character into the long line.
        let typed = format!("var x = 1;\nq{long}var y = 2;\n");
        let result = h.highlight_incremental(&typed, 1, &mut checkpoints, "JavaScript");
        let styles = apply(&full.style_string, typed.len(), &result);
        assert!(line_styles(&typed, &styles, 1).bytes().all(|b| b == b'A'));

        // Edit the normal line after it.
        let edited = format!("var x = 1;\nq{long}if (y) {{ z(); }}\n");
        let result = h.highlight_incremental(&edited, 2, &mut checkpoints, "JavaScript");
        let styles = apply(&styles, edited.len(), &result);
        assert_eq!(styles, h.highlight_full(&edited, "JavaScript").style_string);
        assert!(distinct_styles(line_styles(&edited, &styles, 2)) > 1);
    }

    /// A line pasted past the limit turns plain; shrunk back under it, it is
    /// tokenized again.
    #[test]
    fn long_line_growing_past_limit_turns_plain_and_back() {
        let mut short = String::new();
        while short.len() + 40 < LONG_LINE_BYTES {
            short.push_str("var a=\"x\";function f(b){return b+1}");
        }
        let small = format!("{short}\nvar y = 2;\n");
        let big = format!("{short}{}\nvar y = 2;\n", "var b=2;".repeat(10));
        assert!(small.find('\n').unwrap() < LONG_LINE_BYTES);
        assert!(big.find('\n').unwrap() > LONG_LINE_BYTES);

        let mut h = js();
        let full = h.highlight_full(&small, "JavaScript");
        assert!(distinct_styles(line_styles(&small, &full.style_string, 0)) > 1);
        let mut checkpoints = full.checkpoints;

        let grown = h.highlight_incremental(&big, 0, &mut checkpoints, "JavaScript");
        let styles = apply(&full.style_string, big.len(), &grown);
        assert!(line_styles(&big, &styles, 0).bytes().all(|b| b == b'A'));

        let shrunk = h.highlight_incremental(&small, 0, &mut checkpoints, "JavaScript");
        let styles = apply(&styles, small.len(), &shrunk);
        assert_eq!(styles, full.style_string);
    }

    /// A chunk stops after CHUNK_BYTES even when it holds far fewer than
    /// CHUNK_SIZE lines, so one chunk's work is bounded in bytes.
    #[test]
    fn long_line_chunks_are_bounded_in_bytes() {
        let line = format!("{}\n", "w".repeat(10 * 1024));
        let text = line.repeat(60); // 600 KiB in 60 lines
        let mut h = js();
        h.start_chunked(DocumentId(1), text.clone(), "Plain Text");
        let first = h.process_chunk().expect("chunked highlight is active");
        assert!(!first.done, "600 KiB do not fit one chunk");
        assert!(
            first.style_chars.len() <= CHUNK_BYTES + line.len(),
            "first chunk took {} bytes",
            first.style_chars.len()
        );
        let (styles, chunks) = chunked_styles(&mut h, &text, "Plain Text");
        assert_eq!(styles.len(), text.len());
        assert!(chunks >= 3, "{chunks} chunks");

        // A skipped long line costs syntect nothing: it does not use up the
        // budget, so it and the short lines after it fit one chunk.
        let skipped = format!("{}\n{}", "w".repeat(600 * 1024), line.repeat(3));
        let (styles, chunks) = chunked_styles(&mut h, &skipped, "Plain Text");
        assert_eq!(styles.len(), skipped.len());
        assert_eq!(chunks, 1);
    }
}
