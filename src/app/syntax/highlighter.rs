use syntect::highlighting::{HighlightState, Highlighter, HighlightIterator, Style, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

use super::style_map::StyleMap;

pub struct FullResult {
    pub style_string: String,
    pub parse_states: Vec<ParseState>,
    pub highlight_states: Vec<HighlightState>,
}

pub struct IncrementalResult {
    /// Byte offset in the style buffer where changes begin.
    pub byte_start: usize,
    /// New style characters for [byte_start .. byte_start + style_chars.len()).
    pub style_chars: String,
    /// Full updated parse states for all lines.
    pub parse_states: Vec<ParseState>,
    /// Full updated highlight states for all lines.
    pub highlight_states: Vec<HighlightState>,
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
    let mut parse_states = Vec::new();
    let mut highlight_states = Vec::new();
    let mut style_string = String::with_capacity(text.len());

    for line in LinesWithEndings::new(text) {
        parse_states.push(parse_state.clone());
        highlight_states.push(highlight_state.clone());
        let ops = parse_state.parse_line(line, syntax_set).unwrap_or_default();
        let iter = HighlightIterator::new(&mut highlight_state, &ops, line, &highlighter);
        for (style, piece) in iter {
            let ch = style_to_char(style, style_map);
            // One style char per byte (not per char) for UTF-8 correctness
            for _ in 0..piece.len() {
                style_string.push(ch);
            }
        }
    }

    FullResult { style_string, parse_states, highlight_states }
}

/// Incremental re-highlight starting from `edit_line` (0-indexed).
/// Resumes from cached states, parses forward until convergence.
/// Only returns style chars for the changed region.
pub fn highlight_incremental(
    text: &str,
    edit_line: usize,
    old_parse_states: &[ParseState],
    old_highlight_states: &[HighlightState],
    syntax: &SyntaxReference,
    syntax_set: &SyntaxSet,
    theme_set: &ThemeSet,
    theme_name: &str,
    style_map: &mut StyleMap,
) -> IncrementalResult {
    let theme = &theme_set.themes[theme_name];
    let highlighter = Highlighter::new(theme);
    let lines: Vec<&str> = LinesWithEndings::new(text).collect();

    let start_line = edit_line.min(lines.len().saturating_sub(1))
        .min(old_parse_states.len().saturating_sub(1));

    // Byte offset where the changed region starts
    let byte_start: usize = lines[..start_line].iter().map(|l| l.len()).sum();

    // Resume from cached states at start_line
    let (mut parse_state, mut highlight_state) = if start_line < old_parse_states.len() {
        (old_parse_states[start_line].clone(), old_highlight_states[start_line].clone())
    } else {
        (ParseState::new(syntax), HighlightState::new(&highlighter, ScopeStack::new()))
    };

    // Reuse states for lines before the edit
    let mut new_parse_states: Vec<ParseState> = old_parse_states[..start_line].to_vec();
    let mut new_highlight_states: Vec<HighlightState> = old_highlight_states[..start_line].to_vec();

    let mut style_chars = String::new();
    let mut converged_at: Option<usize> = None;

    for (i, &line) in lines[start_line..].iter().enumerate() {
        let line_idx = start_line + i;
        new_parse_states.push(parse_state.clone());
        new_highlight_states.push(highlight_state.clone());

        let ops = parse_state.parse_line(line, syntax_set).unwrap_or_default();
        let iter = HighlightIterator::new(&mut highlight_state, &ops, line, &highlighter);
        for (style, piece) in iter {
            let ch = style_to_char(style, style_map);
            for _ in 0..piece.len() {
                style_chars.push(ch);
            }
        }

        // Check convergence: if the next line's states match the old cache, stop
        let next_idx = line_idx + 1;
        if next_idx < old_parse_states.len() && next_idx < lines.len() {
            if parse_state == old_parse_states[next_idx]
                && highlight_state == old_highlight_states[next_idx]
            {
                converged_at = Some(next_idx);
                break;
            }
        }
    }

    // If converged, copy remaining old states
    if let Some(conv) = converged_at {
        if conv < old_parse_states.len() {
            new_parse_states.extend_from_slice(&old_parse_states[conv..]);
            new_highlight_states.extend_from_slice(&old_highlight_states[conv..]);
        }
    }

    IncrementalResult {
        byte_start,
        style_chars,
        parse_states: new_parse_states,
        highlight_states: new_highlight_states,
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
