use syntect::highlighting::HighlightState;
use syntect::parsing::ParseState;

pub const CHECKPOINT_INTERVAL: usize = 128;

/// Stores parse and highlight states at sparse intervals (every CHECKPOINT_INTERVAL lines)
/// instead of per-line, reducing memory usage by ~100x for large files.
pub struct SparseCheckpoints {
    pub parse_states: Vec<ParseState>,
    pub highlight_states: Vec<HighlightState>,
    pub line_count: usize,
}

impl SparseCheckpoints {
    pub fn new() -> Self {
        Self {
            parse_states: Vec::new(),
            highlight_states: Vec::new(),
            line_count: 0,
        }
    }

    pub fn with_capacity(line_count: usize) -> Self {
        let cap = line_count / CHECKPOINT_INTERVAL + 1;
        Self {
            parse_states: Vec::with_capacity(cap),
            highlight_states: Vec::with_capacity(cap),
            line_count,
        }
    }

    pub fn clear(&mut self) {
        self.parse_states.clear();
        self.highlight_states.clear();
        self.line_count = 0;
    }

    /// Number of checkpoints stored.
    pub fn len(&self) -> usize {
        self.parse_states.len()
    }

    /// Push a checkpoint (states before the line at `checkpoint_index * CHECKPOINT_INTERVAL`).
    pub fn push(&mut self, parse_state: ParseState, highlight_state: HighlightState) {
        self.parse_states.push(parse_state);
        self.highlight_states.push(highlight_state);
    }

    /// Convert a line index to the checkpoint index that covers it.
    pub fn checkpoint_index(line: usize) -> usize {
        line / CHECKPOINT_INTERVAL
    }

    /// Convert a checkpoint index back to the line it represents.
    pub fn checkpoint_line(idx: usize) -> usize {
        idx * CHECKPOINT_INTERVAL
    }

}

impl Default for SparseCheckpoints {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SparseCheckpoints {
    fn clone(&self) -> Self {
        Self {
            parse_states: self.parse_states.clone(),
            highlight_states: self.highlight_states.clone(),
            line_count: self.line_count,
        }
    }
}
