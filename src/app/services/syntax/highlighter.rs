use std::sync::{LazyLock, Once};

use syntect::highlighting::{
    HighlightIterator, HighlightState, Highlighter, Style, Theme, ThemeSet,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

use crate::app::domain::settings::SyntaxTheme;

use super::checkpoint::{CHECKPOINT_INTERVAL, SparseCheckpoints};
use super::style_map::StyleMap;

/// Lines longer than this (in bytes, line ending included) are not handed to
/// syntect (T0087). syntect's cost on one long line is about 2 µs per byte,
/// and the line would be re-parsed on every keystroke: such a line is styled
/// as one plain run instead, and the parse state carries over unchanged to
/// the next line. VS Code applies the same rule (20 000 characters).
pub const LONG_LINE_BYTES: usize = 16 * 1024;

/// True for a line (line ending included) that is not handed to syntect.
/// Highlight work is budgeted in the bytes of the other lines: a skipped line
/// costs syntect nothing.
pub fn is_long_line(line: &str) -> bool {
    line.len() > LONG_LINE_BYTES
}

pub struct FullResult {
    pub style_string: String,
    pub checkpoints: SparseCheckpoints,
}

pub struct IncrementalResult {
    /// Byte offset in the style buffer where changes begin.
    pub byte_start: usize,
    /// New style characters for [byte_start .. byte_start + style_chars.len()).
    pub style_chars: String,
    /// Set when the byte budget ran out before the parse state converged or
    /// the text ended (T0096): where to continue.
    pub resume: Option<ResumePoint>,
}

/// Where a budgeted re-highlight paused (T0096): the next line to
/// highlight, its byte offset, and the parser state before it.
#[derive(Clone)]
pub struct ResumePoint {
    /// Index of the next line to highlight.
    pub line: usize,
    /// Byte offset of that line in the text.
    pub byte: usize,
    /// Line count of the text, so a continuation step need not rescan it.
    total_lines: usize,
    parse_state: ParseState,
    highlight_state: HighlightState,
}

/// Look up `theme_name`, failing soft instead of panicking like
/// `theme_set.themes[name]`: an unknown name falls back to the default
/// `SyntaxTheme`, then to any theme in the set, then to syntect's
/// built-in default. The fallback is logged once per process: this runs on
/// every (incremental) highlight, i.e. on every keystroke.
pub(super) fn resolve_theme<'a>(theme_set: &'a ThemeSet, theme_name: &str) -> &'a Theme {
    static EMPTY_SET_THEME: LazyLock<Theme> = LazyLock::new(Theme::default);
    static WARN_ONCE: Once = Once::new();

    if let Some(theme) = theme_set.themes.get(theme_name) {
        return theme;
    }
    let fallback = SyntaxTheme::default().theme_key();
    WARN_ONCE.call_once(|| {
        eprintln!("[syntax] theme '{theme_name}' not found, falling back to '{fallback}'");
    });
    theme_set
        .themes
        .get(fallback)
        .or_else(|| theme_set.themes.values().next())
        .unwrap_or(&EMPTY_SET_THEME)
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
    let theme = resolve_theme(theme_set, theme_name);
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
        highlight_line(
            line,
            &mut parse_state,
            &mut highlight_state,
            &highlighter,
            syntax_set,
            style_map,
            &mut style_string,
        );
    }

    FullResult {
        style_string,
        checkpoints,
    }
}

/// Incremental re-highlight starting from `edit_line` (0-indexed): from the
/// checkpoint at or before it, until the parse state converges with a
/// stored checkpoint, the text ends, or `budget` tokenized bytes are done
/// (then `resume` says where to continue, see [`highlight_resume`]).
/// Modifies `checkpoints` in place and returns only the changed style chars.
#[allow(clippy::too_many_arguments)]
pub fn highlight_incremental(
    text: &str,
    edit_line: usize,
    checkpoints: &mut SparseCheckpoints,
    syntax: &SyntaxReference,
    syntax_set: &SyntaxSet,
    theme_set: &ThemeSet,
    theme_name: &str,
    style_map: &mut StyleMap,
    budget: usize,
) -> IncrementalResult {
    let theme = resolve_theme(theme_set, theme_name);
    let highlighter = Highlighter::new(theme);

    let total_lines = LinesWithEndings::new(text).count();

    // Find the nearest checkpoint at or before the edit line
    let start_cp_idx =
        SparseCheckpoints::checkpoint_index(edit_line.min(total_lines.saturating_sub(1)));
    let start_line = SparseCheckpoints::checkpoint_line(start_cp_idx);

    // Resume from cached states at the checkpoint
    let (parse_state, highlight_state) = if start_cp_idx < checkpoints.len() {
        (
            checkpoints.parse_states[start_cp_idx].clone(),
            checkpoints.highlight_states[start_cp_idx].clone(),
        )
    } else {
        (
            ParseState::new(syntax),
            HighlightState::new(&highlighter, ScopeStack::new()),
        )
    };
    let start_byte: usize = LinesWithEndings::new(text)
        .take(start_line)
        .map(str::len)
        .sum();

    highlight_lines_from(
        text,
        ResumePoint {
            line: start_line,
            byte: start_byte,
            total_lines,
            parse_state,
            highlight_state,
        },
        // The starting checkpoint is where the state came from: converging
        // means matching a later one.
        start_cp_idx + 1,
        total_lines,
        checkpoints,
        &highlighter,
        syntax_set,
        style_map,
        budget,
    )
}

/// Continue a re-highlight paused at `resume` (T0096), with the same rules
/// as [`highlight_incremental`]: until convergence, the end of the text, or
/// `budget` tokenized bytes. `text` must be the text the pause was computed
/// on; a resume point that does not fit it yields an empty result.
#[allow(clippy::too_many_arguments)]
pub fn highlight_resume(
    text: &str,
    resume: ResumePoint,
    checkpoints: &mut SparseCheckpoints,
    syntax_set: &SyntaxSet,
    theme_set: &ThemeSet,
    theme_name: &str,
    style_map: &mut StyleMap,
    budget: usize,
) -> IncrementalResult {
    if resume.byte > text.len() || !text.is_char_boundary(resume.byte) {
        return IncrementalResult {
            byte_start: resume.byte.min(text.len()),
            style_chars: String::new(),
            resume: None,
        };
    }
    let theme = resolve_theme(theme_set, theme_name);
    let highlighter = Highlighter::new(theme);
    // Counted at the pause: a continuation step costs its budget, not a
    // scan of the whole text.
    let total_lines = resume.total_lines;
    // The checkpoint at the resume line (if it falls on one) was not touched
    // before the pause: it may already show convergence.
    let first_comparable = resume.line.div_ceil(CHECKPOINT_INTERVAL);
    highlight_lines_from(
        text,
        resume,
        first_comparable,
        total_lines,
        checkpoints,
        &highlighter,
        syntax_set,
        style_map,
        budget,
    )
}

/// The one re-highlight loop (incremental and its continuation): highlight
/// lines from `start` until the state matches a stored checkpoint whose index
/// is at least `first_comparable` (converged), the text ends, or `budget`
/// tokenized bytes are done (paused, before the next line and its
/// checkpoint).
#[allow(clippy::too_many_arguments)]
fn highlight_lines_from(
    text: &str,
    start: ResumePoint,
    first_comparable: usize,
    total_lines: usize,
    checkpoints: &mut SparseCheckpoints,
    highlighter: &Highlighter,
    syntax_set: &SyntaxSet,
    style_map: &mut StyleMap,
    budget: usize,
) -> IncrementalResult {
    let ResumePoint {
        line: mut line_idx,
        byte: byte_start,
        mut parse_state,
        mut highlight_state,
        ..
    } = start;
    let mut byte = byte_start;
    let mut style_chars = String::new();
    let mut tokenized = 0;
    let mut resume = None;

    for line in LinesWithEndings::new(&text[byte_start..]) {
        if tokenized >= budget {
            resume = Some(ResumePoint {
                line: line_idx,
                byte,
                total_lines,
                parse_state,
                highlight_state,
            });
            break;
        }

        // Save/check checkpoint at boundary
        if line_idx % CHECKPOINT_INTERVAL == 0 {
            let cp_idx = line_idx / CHECKPOINT_INTERVAL;

            if cp_idx >= first_comparable
                && cp_idx < checkpoints.len()
                && parse_state == checkpoints.parse_states[cp_idx]
                && highlight_state == checkpoints.highlight_states[cp_idx]
            {
                // Converged — no more changes needed
                break;
            }

            // Update checkpoint in place (or push if we're past the old length)
            if cp_idx < checkpoints.len() {
                checkpoints.parse_states[cp_idx] = parse_state.clone();
                checkpoints.highlight_states[cp_idx] = highlight_state.clone();
            } else {
                checkpoints.push(parse_state.clone(), highlight_state.clone());
            }
        }

        highlight_line(
            line,
            &mut parse_state,
            &mut highlight_state,
            highlighter,
            syntax_set,
            style_map,
            &mut style_chars,
        );
        if !is_long_line(line) {
            tokenized += line.len();
        }
        byte += line.len();
        line_idx += 1;
    }

    // Update total line count and trim excess checkpoints if file got shorter
    checkpoints.line_count = total_lines;
    let expected_cp_count = total_lines / CHECKPOINT_INTERVAL + 1;
    checkpoints.parse_states.truncate(expected_cp_count);
    checkpoints.highlight_states.truncate(expected_cp_count);

    IncrementalResult {
        byte_start,
        style_chars,
        resume,
    }
}

/// Highlight one line (with its line ending), appending one style char per
/// byte of `line` to `out`. The one per-line step of every highlight path
/// (full, incremental, chunked). A line longer than [`LONG_LINE_BYTES`] is
/// not tokenized: it gets the plain style ('A') and leaves both states as
/// they are.
pub(super) fn highlight_line(
    line: &str,
    parse_state: &mut ParseState,
    highlight_state: &mut HighlightState,
    highlighter: &Highlighter,
    syntax_set: &SyntaxSet,
    style_map: &mut StyleMap,
    out: &mut String,
) {
    if is_long_line(line) {
        out.extend(std::iter::repeat_n('A', line.len()));
        return;
    }
    let ops = parse_state.parse_line(line, syntax_set).unwrap_or_default();
    let iter = HighlightIterator::new(highlight_state, &ops, line, highlighter);
    for (style, piece) in iter {
        let ch = style_to_char(style, style_map);
        out.extend(std::iter::repeat_n(ch, piece.len()));
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
        let end = self
            .text
            .find('\n')
            .map(|i| i + 1)
            .unwrap_or(self.text.len());
        let line = &self.text[..end];
        self.text = &self.text[end..];
        Some(line)
    }
}
