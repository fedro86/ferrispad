//! Line-aligned diff computation with intraline emphasis.
//!
//! Uses the `similar` crate to produce aligned left/right content
//! with filler lines for insertions/deletions, and character-level
//! spans for replacement regions.

use similar::{ChangeTag, TextDiff};

/// Maximum bytes accepted on either side of a plugin-requested diff.
///
/// The character/line Myers diff is O(n·m); on plugin-supplied strings with no
/// cap, two multi-megabyte inputs hang the UI thread and allocate outside the
/// Lua memory budget. `diff_text` rejects anything larger (T0008, audit M3).
pub const MAX_DIFF_INPUT_BYTES: usize = 1024 * 1024; // 1 MiB per side

/// Longest replacement line pair the O(n·m) character-level intraline diff runs
/// on. Longer lines are still shown as changed, just without intraline
/// emphasis, so a pathological single-line megabyte pair cannot hang the diff
/// (T0008, audit M3).
const MAX_INTRALINE_BYTES: usize = 4096;

/// True if either side exceeds [`MAX_DIFF_INPUT_BYTES`] and the diff should be
/// refused rather than computed.
pub fn diff_input_too_large(old: &str, new: &str) -> bool {
    old.len() > MAX_DIFF_INPUT_BYTES || new.len() > MAX_DIFF_INPUT_BYTES
}

/// A byte range within a line for intraline emphasis highlighting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntralineSpan {
    /// Start byte offset (0-indexed, inclusive)
    pub start: u32,
    /// End byte offset (0-indexed, exclusive)
    pub end: u32,
}

/// Per-line highlight info in the diff result.
#[derive(Debug, Clone)]
pub struct DiffLineHighlight {
    /// 1-indexed line number in the output content
    pub line: u32,
    /// "added" or "removed"
    pub color: &'static str,
    /// Intraline emphasis spans (byte offsets within the line)
    pub spans: Vec<IntralineSpan>,
}

/// Result of computing an aligned diff.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Left pane content (HEAD) with filler lines inserted
    pub left_content: String,
    /// Right pane content (working copy) with filler lines inserted
    pub right_content: String,
    /// Line highlights for the left pane
    pub left_highlights: Vec<DiffLineHighlight>,
    /// Line highlights for the right pane
    pub right_highlights: Vec<DiffLineHighlight>,
}

/// Compute character-level intraline spans for a replacement pair.
/// Returns (left_spans, right_spans) as byte offset ranges within each line.
fn compute_intraline_spans(
    old_line: &str,
    new_line: &str,
) -> (Vec<IntralineSpan>, Vec<IntralineSpan>) {
    // Bound the O(n·m) character diff: a single very long replacement pair (e.g.
    // a minified megabyte line) would otherwise hang the UI thread. Above the
    // cap we skip intraline emphasis; the line still shows as changed (T0008).
    if old_line.len() > MAX_INTRALINE_BYTES || new_line.len() > MAX_INTRALINE_BYTES {
        return (Vec::new(), Vec::new());
    }
    let diff = TextDiff::from_chars(old_line, new_line);
    let mut left_spans = Vec::new();
    let mut right_spans = Vec::new();
    let mut old_pos: u32 = 0;
    let mut new_pos: u32 = 0;

    for change in diff.iter_all_changes() {
        let len = change.value().len() as u32;
        match change.tag() {
            ChangeTag::Equal => {
                old_pos += len;
                new_pos += len;
            }
            ChangeTag::Delete => {
                left_spans.push(IntralineSpan {
                    start: old_pos,
                    end: old_pos + len,
                });
                old_pos += len;
            }
            ChangeTag::Insert => {
                right_spans.push(IntralineSpan {
                    start: new_pos,
                    end: new_pos + len,
                });
                new_pos += len;
            }
        }
    }

    (left_spans, right_spans)
}

/// Compute an aligned diff between old and new text.
///
/// Lines are aligned so that:
/// - Equal lines appear on both sides at the same row
/// - Deleted lines appear on the left with a blank filler on the right
/// - Inserted lines appear on the right with a blank filler on the left
/// - Consecutive delete+insert sequences are paired as replacements
///   with intraline character-level emphasis
pub fn compute_aligned_diff(old: &str, new: &str) -> DiffResult {
    let diff = TextDiff::from_lines(old, new);
    let changes: Vec<_> = diff.iter_all_changes().collect();

    let mut left_lines: Vec<String> = Vec::new();
    let mut right_lines: Vec<String> = Vec::new();
    let mut left_highlights: Vec<DiffLineHighlight> = Vec::new();
    let mut right_highlights: Vec<DiffLineHighlight> = Vec::new();

    // Collect consecutive deletes and inserts to pair as replacements
    let mut i = 0;
    while i < changes.len() {
        match changes[i].tag() {
            ChangeTag::Equal => {
                let line = changes[i].value();
                let trimmed = line.strip_suffix('\n').unwrap_or(line);
                left_lines.push(trimmed.to_string());
                right_lines.push(trimmed.to_string());
                i += 1;
            }
            ChangeTag::Delete => {
                // Collect consecutive deletes
                let mut deletes: Vec<&str> = Vec::new();
                while i < changes.len() && changes[i].tag() == ChangeTag::Delete {
                    deletes.push(changes[i].value());
                    i += 1;
                }
                // Collect consecutive inserts that follow
                let mut inserts: Vec<&str> = Vec::new();
                while i < changes.len() && changes[i].tag() == ChangeTag::Insert {
                    inserts.push(changes[i].value());
                    i += 1;
                }

                // Pair deletes with inserts as replacements
                let max_pairs = deletes.len().max(inserts.len());
                for j in 0..max_pairs {
                    let del = deletes.get(j).map(|s| s.strip_suffix('\n').unwrap_or(s));
                    let ins = inserts.get(j).map(|s| s.strip_suffix('\n').unwrap_or(s));

                    match (del, ins) {
                        (Some(d), Some(n)) => {
                            // Replacement: both sides have content, compute intraline
                            let (left_spans, right_spans) = compute_intraline_spans(d, n);
                            let left_line_num = (left_lines.len() + 1) as u32;
                            let right_line_num = (right_lines.len() + 1) as u32;
                            left_lines.push(d.to_string());
                            right_lines.push(n.to_string());
                            left_highlights.push(DiffLineHighlight {
                                line: left_line_num,
                                color: "removed",
                                spans: left_spans,
                            });
                            right_highlights.push(DiffLineHighlight {
                                line: right_line_num,
                                color: "added",
                                spans: right_spans,
                            });
                        }
                        (Some(d), None) => {
                            // Pure deletion: left has line, right gets filler
                            let left_line_num = (left_lines.len() + 1) as u32;
                            left_lines.push(d.to_string());
                            right_lines.push(String::new());
                            left_highlights.push(DiffLineHighlight {
                                line: left_line_num,
                                color: "removed",
                                spans: Vec::new(),
                            });
                        }
                        (None, Some(n)) => {
                            // Pure insertion: right has line, left gets filler
                            let right_line_num = (right_lines.len() + 1) as u32;
                            left_lines.push(String::new());
                            right_lines.push(n.to_string());
                            right_highlights.push(DiffLineHighlight {
                                line: right_line_num,
                                color: "added",
                                spans: Vec::new(),
                            });
                        }
                        (None, None) => unreachable!(),
                    }
                }
            }
            ChangeTag::Insert => {
                // Standalone insert (no preceding delete)
                let line = changes[i].value();
                let trimmed = line.strip_suffix('\n').unwrap_or(line);
                let right_line_num = (right_lines.len() + 1) as u32;
                left_lines.push(String::new());
                right_lines.push(trimmed.to_string());
                right_highlights.push(DiffLineHighlight {
                    line: right_line_num,
                    color: "added",
                    spans: Vec::new(),
                });
                i += 1;
            }
        }
    }

    DiffResult {
        left_content: left_lines.join("\n"),
        right_content: right_lines.join("\n"),
        left_highlights,
        right_highlights,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical() {
        let text = "hello\nworld\n";
        let result = compute_aligned_diff(text, text);
        assert_eq!(result.left_content, result.right_content);
        assert!(result.left_highlights.is_empty());
        assert!(result.right_highlights.is_empty());
    }

    #[test]
    fn test_empty_inputs() {
        let result = compute_aligned_diff("", "");
        assert!(result.left_content.is_empty());
        assert!(result.right_content.is_empty());
    }

    #[test]
    fn test_pure_insertion() {
        let old = "aaa\nccc\n";
        let new = "aaa\nbbb\nccc\n";
        let result = compute_aligned_diff(old, new);

        // Left should have a filler line, right should have the inserted line
        let left_lines: Vec<&str> = result.left_content.lines().collect();
        let right_lines: Vec<&str> = result.right_content.lines().collect();
        assert_eq!(left_lines.len(), right_lines.len(), "Lines must be aligned");

        // The inserted "bbb" should appear in right highlights
        assert!(!result.right_highlights.is_empty());
        assert!(result.left_highlights.is_empty());
    }

    #[test]
    fn test_pure_deletion() {
        let old = "aaa\nbbb\nccc\n";
        let new = "aaa\nccc\n";
        let result = compute_aligned_diff(old, new);

        let left_lines: Vec<&str> = result.left_content.lines().collect();
        let right_lines: Vec<&str> = result.right_content.lines().collect();
        assert_eq!(left_lines.len(), right_lines.len(), "Lines must be aligned");

        // The deleted "bbb" should appear in left highlights
        assert!(!result.left_highlights.is_empty());
        assert!(result.right_highlights.is_empty());
    }

    #[test]
    fn test_replacement_with_intraline() {
        let old = "hello world\n";
        let new = "hello rust\n";
        let result = compute_aligned_diff(old, new);

        let left_lines: Vec<&str> = result.left_content.lines().collect();
        let right_lines: Vec<&str> = result.right_content.lines().collect();
        assert_eq!(left_lines.len(), right_lines.len());

        // Both sides should be highlighted
        assert_eq!(result.left_highlights.len(), 1);
        assert_eq!(result.right_highlights.len(), 1);

        // Intraline spans should exist (the differing part)
        assert!(
            !result.left_highlights[0].spans.is_empty(),
            "Left should have intraline spans"
        );
        assert!(
            !result.right_highlights[0].spans.is_empty(),
            "Right should have intraline spans"
        );
    }

    #[test]
    fn test_utf8_content() {
        let old = "hello 世界\n";
        let new = "hello 地球\n";
        let result = compute_aligned_diff(old, new);

        assert_eq!(result.left_highlights.len(), 1);
        assert_eq!(result.right_highlights.len(), 1);

        // Spans should be valid byte offsets
        for span in &result.left_highlights[0].spans {
            assert!(span.end as usize <= "hello 世界".len());
        }
        for span in &result.right_highlights[0].spans {
            assert!(span.end as usize <= "hello 地球".len());
        }
    }

    #[test]
    fn test_mixed_changes() {
        let old = "line1\nline2\nline3\nline4\n";
        let new = "line1\nmodified2\nline4\nnew5\n";
        let result = compute_aligned_diff(old, new);

        // Use split('\n') instead of lines() because lines() skips trailing empty strings,
        // which causes miscounts when filler lines (empty strings) are at the end.
        let left_count = result.left_content.split('\n').count();
        let right_count = result.right_content.split('\n').count();
        assert_eq!(left_count, right_count, "Lines must be aligned");
    }

    #[test]
    fn test_old_empty_new_has_content() {
        let result = compute_aligned_diff("", "hello\nworld\n");
        assert!(!result.right_highlights.is_empty());
        let left_count = result.left_content.split('\n').count();
        let right_count = result.right_content.split('\n').count();
        assert_eq!(left_count, right_count);
    }

    #[test]
    fn test_old_has_content_new_empty() {
        let result = compute_aligned_diff("hello\nworld\n", "");
        assert!(!result.left_highlights.is_empty());
        let left_count = result.left_content.split('\n').count();
        let right_count = result.right_content.split('\n').count();
        assert_eq!(left_count, right_count);
    }

    // Regression (T0008, audit M3): a replacement pair of very long lines must
    // NOT run the O(n·m) character diff — it would hang the UI thread. The lines
    // still show as changed, but with no intraline spans. Before the guard,
    // `from_chars` on these 5000-byte lines produced non-empty spans (kept small
    // enough here to run quickly and prove the change cleanly).
    #[test]
    fn intraline_spans_are_skipped_for_long_lines() {
        let old = "a".repeat(5000); // > MAX_INTRALINE_BYTES (4096)
        let mut new = "a".repeat(4998);
        new.push_str("bb"); // differs at the tail -> a replacement pair
        assert_eq!(old.len(), 5000);
        assert_eq!(new.len(), 5000);

        let result = compute_aligned_diff(&old, &new);

        assert_eq!(result.left_highlights.len(), 1);
        assert_eq!(result.right_highlights.len(), 1);
        assert!(
            result.left_highlights[0].spans.is_empty(),
            "long-line intraline diff must be skipped (left)"
        );
        assert!(
            result.right_highlights[0].spans.is_empty(),
            "long-line intraline diff must be skipped (right)"
        );
    }

    // Short replacement lines still get intraline emphasis (guard is length-based
    // only, it must not disable the normal case).
    #[test]
    fn intraline_spans_still_computed_for_short_lines() {
        let result = compute_aligned_diff("hello world\n", "hello rust\n");
        assert!(!result.left_highlights[0].spans.is_empty());
        assert!(!result.right_highlights[0].spans.is_empty());
    }

    // A pathological single-line multi-megabyte pair must terminate promptly
    // (the intraline guard skips the O(n·m) diff). Before the fix this hung.
    #[test]
    fn huge_single_line_diff_terminates() {
        let old = "x".repeat(2 * 1024 * 1024);
        let new = "y".repeat(2 * 1024 * 1024);
        let result = compute_aligned_diff(&old, &new);
        // It completed; the megabyte lines carry no intraline spans.
        assert!(result.left_highlights.iter().all(|h| h.spans.is_empty()));
        assert!(result.right_highlights.iter().all(|h| h.spans.is_empty()));
    }

    #[test]
    fn diff_input_too_large_flags_oversized_inputs() {
        let big = "a".repeat(MAX_DIFF_INPUT_BYTES + 1);
        assert!(diff_input_too_large(&big, ""));
        assert!(diff_input_too_large("", &big));
        assert!(!diff_input_too_large("small", "also small"));
        // Exactly at the cap is allowed.
        let at_cap = "a".repeat(MAX_DIFF_INPUT_BYTES);
        assert!(!diff_input_too_large(&at_cap, &at_cap));
    }
}
