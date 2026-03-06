use ferris_pad::app::plugins::diff::compute_aligned_diff;

#[test]
fn test_diff_produces_aligned_output() {
    let old = "line1\nline2\nline3\nline4\nline5\n";
    let new = "line1\nmodified2\nline3\nnew_line\nline5\n";
    let result = compute_aligned_diff(old, new);

    let left_count = result.left_content.split('\n').count();
    let right_count = result.right_content.split('\n').count();
    assert_eq!(
        left_count, right_count,
        "Left ({}) and right ({}) must have equal line counts",
        left_count, right_count
    );
}

#[test]
fn test_diff_intraline_spans_valid() {
    let old = "hello world foo bar\n";
    let new = "hello rust foo baz\n";
    let result = compute_aligned_diff(old, new);

    // Check left highlights
    for hl in &result.left_highlights {
        let line_idx = (hl.line - 1) as usize;
        let lines: Vec<&str> = result.left_content.split('\n').collect();
        let line_content = lines[line_idx];
        for span in &hl.spans {
            assert!(
                (span.end as usize) <= line_content.len(),
                "Left span end {} exceeds line length {} for line '{}'",
                span.end,
                line_content.len(),
                line_content
            );
            assert!(span.start < span.end, "Span start must be < end");
        }
    }

    // Check right highlights
    for hl in &result.right_highlights {
        let line_idx = (hl.line - 1) as usize;
        let lines: Vec<&str> = result.right_content.split('\n').collect();
        let line_content = lines[line_idx];
        for span in &hl.spans {
            assert!(
                (span.end as usize) <= line_content.len(),
                "Right span end {} exceeds line length {} for line '{}'",
                span.end,
                line_content.len(),
                line_content
            );
        }
    }
}

#[test]
fn test_diff_empty_to_full() {
    let result = compute_aligned_diff("", "line1\nline2\nline3\n");

    // All right lines should be highlighted as added
    assert!(
        !result.right_highlights.is_empty(),
        "Right highlights should be non-empty for pure insertion"
    );
    assert!(
        result.left_highlights.is_empty(),
        "Left highlights should be empty for pure insertion"
    );

    // All right highlights should be "added"
    for hl in &result.right_highlights {
        assert_eq!(hl.color, "added");
    }

    // Lines should be aligned
    let left_count = result.left_content.split('\n').count();
    let right_count = result.right_content.split('\n').count();
    assert_eq!(left_count, right_count);
}

#[test]
fn test_diff_full_to_empty() {
    let result = compute_aligned_diff("line1\nline2\nline3\n", "");

    assert!(
        !result.left_highlights.is_empty(),
        "Left highlights should be non-empty for pure deletion"
    );
    assert!(
        result.right_highlights.is_empty(),
        "Right highlights should be empty for pure deletion"
    );

    for hl in &result.left_highlights {
        assert_eq!(hl.color, "removed");
    }

    let left_count = result.left_content.split('\n').count();
    let right_count = result.right_content.split('\n').count();
    assert_eq!(left_count, right_count);
}

#[test]
fn test_diff_large_texts() {
    // Generate two 5000-line files with scattered changes
    let mut old_lines: Vec<String> = (0..5000).map(|i| format!("original line {}", i)).collect();
    let mut new_lines = old_lines.clone();

    // Scatter changes every 100 lines
    for i in (0..5000).step_by(100) {
        new_lines[i] = format!("MODIFIED line {}", i);
    }
    // Add some insertions
    new_lines.insert(2500, "INSERTED LINE A".to_string());
    new_lines.insert(2501, "INSERTED LINE B".to_string());
    // Delete some lines
    old_lines.push("EXTRA OLD LINE 1".to_string());
    old_lines.push("EXTRA OLD LINE 2".to_string());

    let old_text = old_lines.join("\n") + "\n";
    let new_text = new_lines.join("\n") + "\n";

    let result = compute_aligned_diff(&old_text, &new_text);

    // Must complete (no timeout) and produce aligned output
    let left_count = result.left_content.split('\n').count();
    let right_count = result.right_content.split('\n').count();
    assert_eq!(left_count, right_count, "Large diff must be aligned");

    // Should have highlights (we made changes)
    assert!(
        !result.left_highlights.is_empty() || !result.right_highlights.is_empty(),
        "Should have some highlights for changed text"
    );
}
