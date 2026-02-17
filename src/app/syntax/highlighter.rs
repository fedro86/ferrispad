use syntect::highlighting::{HighlightState, Highlighter, HighlightIterator, Style, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

use super::checkpoint::{SparseCheckpoints, CHECKPOINT_INTERVAL};
use super::style_map::StyleMap;

pub struct FullResult {
    pub style_string: String,
    pub checkpoints: SparseCheckpoints,
}

pub struct IncrementalResult {
    /// Byte offset in the style buffer where changes begin.
    pub byte_start: usize,
    /// New style characters for [byte_start .. byte_start + style_chars.len()).
    pub style_chars: String,
}

/// Full highlight of the document text.
pub fn highlight_full(
    text: &str,
    syntax: &SyntaxReference,
    syntax_set: &SyntaxSet,
    theme_set: &ThemeSet,
    theme_name: &str,
    style_map: &mut StyleMap,
) -> FullResult {
    let theme = &theme_set.themes[theme_name];
    let highlighter = Highlighter::new(theme);
    let mut parse_state = ParseState::new(syntax);
    let mut highlight_state = HighlightState::new(&highlighter, ScopeStack::new());

    let line_count = LinesWithEndings::new(text).count();
    let mut checkpoints = SparseCheckpoints::with_capacity(line_count);
    checkpoints.line_count = line_count;
    let mut style_string = String::with_capacity(text.len());

    for (line_idx, line) in LinesWithEndings::new(text).enumerate() {
        if line_idx % CHECKPOINT_INTERVAL == 0 {
            checkpoints.push(parse_state.clone(), highlight_state.clone());
        }
        let ops = parse_state.parse_line(line, syntax_set).unwrap_or_default();
        let iter = HighlightIterator::new(&mut highlight_state, &ops, line, &highlighter);
        for (style, piece) in iter {
            let ch = style_to_char(style, style_map);
            for _ in 0..piece.len() {
                style_string.push(ch);
            }
        }
    }

    FullResult { style_string, checkpoints }
}

/// Incremental re-highlight starting from `edit_line` (0-indexed).
/// Modifies `checkpoints` in place and returns only the changed style chars region.
/// Uses an iterator over lines (no Vec collection) to minimize allocations.
pub fn highlight_incremental(
    text: &str,
    edit_line: usize,
    checkpoints: &mut SparseCheckpoints,
    syntax: &SyntaxReference,
    syntax_set: &SyntaxSet,
    theme_set: &ThemeSet,
    theme_name: &str,
    style_map: &mut StyleMap,
) -> IncrementalResult {
    let theme = &theme_set.themes[theme_name];
    let highlighter = Highlighter::new(theme);

    // Count total lines efficiently
    let total_lines = LinesWithEndings::new(text).count();

    // Find the nearest checkpoint at or before the edit line
    let start_cp_idx = SparseCheckpoints::checkpoint_index(
        edit_line.min(total_lines.saturating_sub(1)),
    );
    let start_line = SparseCheckpoints::checkpoint_line(start_cp_idx);

    // Resume from cached states at the checkpoint
    let (mut parse_state, mut highlight_state) = if start_cp_idx < checkpoints.len() {
        (
            checkpoints.parse_states[start_cp_idx].clone(),
            checkpoints.highlight_states[start_cp_idx].clone(),
        )
    } else {
        (ParseState::new(syntax), HighlightState::new(&highlighter, ScopeStack::new()))
    };

    // Compute byte_start and iterate lines without collecting into Vec
    let mut byte_start = 0;
    let mut style_chars = String::new();
    let mut _converged_at_cp: Option<usize> = None;
    let mut line_idx = 0;

    for line in LinesWithEndings::new(text) {
        if line_idx < start_line {
            byte_start += line.len();
            line_idx += 1;
            continue;
        }

        // Save/check checkpoint at boundary
        if line_idx % CHECKPOINT_INTERVAL == 0 {
            let cp_idx = line_idx / CHECKPOINT_INTERVAL;

            // Check convergence at checkpoint boundaries (skip the starting checkpoint)
            if cp_idx > start_cp_idx && cp_idx < checkpoints.len() {
                if parse_state == checkpoints.parse_states[cp_idx]
                    && highlight_state == checkpoints.highlight_states[cp_idx]
                {
                    // Converged — no more changes needed
                    _converged_at_cp = Some(cp_idx);
                    break;
                }
            }

            // Update checkpoint in place (or push if we're past the old length)
            if cp_idx < checkpoints.len() {
                checkpoints.parse_states[cp_idx] = parse_state.clone();
                checkpoints.highlight_states[cp_idx] = highlight_state.clone();
            } else {
                checkpoints.push(parse_state.clone(), highlight_state.clone());
            }
        }

        let ops = parse_state.parse_line(line, syntax_set).unwrap_or_default();
        let iter = HighlightIterator::new(&mut highlight_state, &ops, line, &highlighter);
        for (style, piece) in iter {
            let ch = style_to_char(style, style_map);
            for _ in 0..piece.len() {
                style_chars.push(ch);
            }
        }

        line_idx += 1;
    }

    // Update total line count and trim excess checkpoints if file got shorter
    checkpoints.line_count = total_lines;
    let expected_cp_count = total_lines / CHECKPOINT_INTERVAL + 1;
    checkpoints.parse_states.truncate(expected_cp_count);
    checkpoints.highlight_states.truncate(expected_cp_count);

    // If we didn't converge and went past the old checkpoint count,
    // the new checkpoints were already pushed in the loop above.

    IncrementalResult {
        byte_start,
        style_chars,
    }
}

fn style_to_char(style: Style, style_map: &mut StyleMap) -> char {
    style_map.get_or_insert(style.foreground)
}

/// Iterator that yields lines including their line endings.
pub(super) struct LinesWithEndings<'a> {
    text: &'a str,
}

impl<'a> LinesWithEndings<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        Self { text }
    }
}

impl<'a> Iterator for LinesWithEndings<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.text.is_empty() {
            return None;
        }
        let end = self.text.find('\n').map(|i| i + 1).unwrap_or(self.text.len());
        let line = &self.text[..end];
        self.text = &self.text[end..];
        Some(line)
    }
}
