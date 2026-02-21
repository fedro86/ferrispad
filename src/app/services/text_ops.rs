use std::path::Path;

/// Extract filename from a file path
///
/// Returns the filename component of a path, or "Unknown" if it can't be extracted.
pub fn extract_filename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty() && *s != ".")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Find next occurrence of search string in text
///
/// Returns the byte position of the match, or None if not found.
/// Searches from start_pos onwards.
pub fn find_in_text(text: &str, search: &str, start_pos: usize, case_sensitive: bool) -> Option<usize> {
    if search.is_empty() || start_pos >= text.len() {
        return None;
    }

    let haystack = if case_sensitive {
        text[start_pos..].to_string()
    } else {
        text[start_pos..].to_lowercase()
    };

    let needle = if case_sensitive {
        search.to_string()
    } else {
        search.to_lowercase()
    };

    haystack.find(&needle).map(|pos| start_pos + pos)
}

/// Find previous occurrence of search string in text (backward search)
///
/// Returns the byte position of the match, or None if not found.
/// Searches backwards from start_pos (exclusive).
pub fn find_in_text_backward(text: &str, search: &str, start_pos: usize, case_sensitive: bool) -> Option<usize> {
    if search.is_empty() || start_pos == 0 {
        return None;
    }

    let end = start_pos.min(text.len());
    let haystack = &text[..end];

    if case_sensitive {
        haystack.rfind(search)
    } else {
        haystack.to_lowercase().rfind(&search.to_lowercase())
    }
}

/// Convert a 1-based line number to a byte position in the text
///
/// Returns None if the line number is 0 or beyond the end of the text.
pub fn line_number_to_byte_position(text: &str, line: usize) -> Option<usize> {
    if line == 0 {
        return None;
    }
    if line == 1 {
        return Some(0);
    }

    let mut current_line = 1;
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            current_line += 1;
            if current_line == line {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Replace all occurrences of search string with replacement
///
/// Returns (new_text, count_of_replacements)
pub fn replace_all_in_text(text: &str, search: &str, replace: &str, case_sensitive: bool) -> (String, usize) {
    if search.is_empty() {
        return (text.to_string(), 0);
    }

    let mut result = text.to_string();
    let mut count = 0;
    let mut pos = 0;

    while let Some(found_pos) = find_in_text(&result, search, pos, case_sensitive) {
        // Get the actual matched text (preserves original case)
        let matched_text = &result[found_pos..found_pos + search.len()];

        // Replace this occurrence
        result.replace_range(found_pos..found_pos + matched_text.len(), replace);

        // Move position forward by replacement length
        pos = found_pos + replace.len();
        count += 1;

        // Prevent infinite loop if replace contains search
        if replace.contains(search) && pos >= result.len() {
            break;
        }
    }

    (result, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_filename_from_path() {
        assert_eq!(extract_filename("/home/user/test.txt"), "test.txt");
        assert_eq!(extract_filename("/home/user/document.md"), "document.md");
        assert_eq!(extract_filename("test.txt"), "test.txt");
        assert_eq!(extract_filename("/path/with/many/levels/file.rs"), "file.rs");
    }

    #[test]
    fn test_extract_filename_edge_cases() {
        assert_eq!(extract_filename("/home/user/"), "user");
        assert_eq!(extract_filename(""), "Unknown");
        assert_eq!(extract_filename("."), "Unknown");
        assert_eq!(extract_filename("/"), "Unknown");
    }

    #[test]
    fn test_find_next_simple() {
        let text = "Hello world, hello Rust, hello FerrisPad";
        let search = "hello";
        let result = find_in_text(text, search, 0, false);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_find_case_sensitive() {
        let text = "Hello world, hello Rust, hello FerrisPad";
        let search = "Hello";
        let result = find_in_text(text, search, 0, true);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_find_no_match() {
        let text = "Hello world";
        let search = "rust";
        let result = find_in_text(text, search, 0, false);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_from_position() {
        let text = "cat dog cat mouse cat";
        let search = "cat";
        let result = find_in_text(text, search, 10, false);
        assert_eq!(result, Some(18));
    }

    #[test]
    fn test_replace_all_simple() {
        let text = "cat cat cat";
        let result = replace_all_in_text(text, "cat", "dog", false);
        assert_eq!(result.0, "dog dog dog");
        assert_eq!(result.1, 3);
    }

    #[test]
    fn test_replace_all_case_sensitive() {
        let text = "Cat cat CAT";
        let result = replace_all_in_text(text, "cat", "dog", true);
        assert_eq!(result.0, "Cat dog CAT");
        assert_eq!(result.1, 1);
    }

    #[test]
    fn test_replace_all_case_insensitive() {
        let text = "Cat cat CAT";
        let result = replace_all_in_text(text, "cat", "dog", false);
        assert_eq!(result.0, "dog dog dog");
        assert_eq!(result.1, 3);
    }

    #[test]
    fn test_replace_all_no_matches() {
        let text = "hello world";
        let result = replace_all_in_text(text, "rust", "ferris", false);
        assert_eq!(result.0, "hello world");
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_replace_all_empty_replacement() {
        let text = "hello world hello";
        let result = replace_all_in_text(text, "hello", "", false);
        assert_eq!(result.0, " world ");
        assert_eq!(result.1, 2);
    }

    // Find backward tests

    #[test]
    fn test_find_backward_simple() {
        let text = "cat dog cat mouse cat";
        let result = find_in_text_backward(text, "cat", text.len(), false);
        assert_eq!(result, Some(18));
    }

    #[test]
    fn test_find_backward_from_middle() {
        let text = "cat dog cat mouse cat";
        // Search backward from position 18 (last "cat"), should find middle "cat"
        let result = find_in_text_backward(text, "cat", 18, false);
        assert_eq!(result, Some(8));
    }

    #[test]
    fn test_find_backward_no_match() {
        let text = "hello world";
        let result = find_in_text_backward(text, "rust", text.len(), false);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_backward_case_insensitive() {
        let text = "Hello world HELLO";
        let result = find_in_text_backward(text, "hello", text.len(), false);
        assert_eq!(result, Some(12));
    }

    #[test]
    fn test_find_backward_start_zero() {
        let text = "cat dog cat";
        let result = find_in_text_backward(text, "cat", 0, false);
        assert_eq!(result, None);
    }

    // Line number tests

    #[test]
    fn test_line_to_pos_first_line() {
        let text = "first line\nsecond line\nthird line";
        assert_eq!(line_number_to_byte_position(text, 1), Some(0));
    }

    #[test]
    fn test_line_to_pos_middle() {
        let text = "first\nsecond\nthird";
        assert_eq!(line_number_to_byte_position(text, 2), Some(6));
        assert_eq!(line_number_to_byte_position(text, 3), Some(13));
    }

    #[test]
    fn test_line_to_pos_out_of_range() {
        let text = "first\nsecond\nthird";
        assert_eq!(line_number_to_byte_position(text, 4), None);
        assert_eq!(line_number_to_byte_position(text, 100), None);
    }

    #[test]
    fn test_line_to_pos_zero() {
        let text = "hello";
        assert_eq!(line_number_to_byte_position(text, 0), None);
    }

    // Additional edge case tests

    #[test]
    fn test_find_empty_search() {
        let text = "hello world";
        assert_eq!(find_in_text(text, "", 0, false), None);
        assert_eq!(find_in_text_backward(text, "", text.len(), false), None);
    }

    #[test]
    fn test_find_start_beyond_text() {
        let text = "hello";
        assert_eq!(find_in_text(text, "hello", 100, false), None);
    }

    #[test]
    fn test_replace_empty_search() {
        let text = "hello world";
        let (result, count) = replace_all_in_text(text, "", "X", false);
        assert_eq!(result, "hello world");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_replace_with_longer_string() {
        let text = "a b c";
        let (result, count) = replace_all_in_text(text, " ", "---", false);
        assert_eq!(result, "a---b---c");
        assert_eq!(count, 2);
    }

    #[test]
    fn test_line_to_pos_empty_text() {
        let text = "";
        assert_eq!(line_number_to_byte_position(text, 1), Some(0));
        assert_eq!(line_number_to_byte_position(text, 2), None);
    }

    #[test]
    fn test_line_to_pos_single_newline() {
        let text = "\n";
        assert_eq!(line_number_to_byte_position(text, 1), Some(0));
        assert_eq!(line_number_to_byte_position(text, 2), Some(1));
        assert_eq!(line_number_to_byte_position(text, 3), None);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_extract_filename_windows_path() {
        // Windows Path parses Windows paths correctly
        assert_eq!(extract_filename("C:\\Users\\test\\file.txt"), "file.txt");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_extract_filename_unix_absolute() {
        // Unix systems parse forward-slash paths
        assert_eq!(extract_filename("/usr/local/bin/program"), "program");
    }

    #[test]
    fn test_find_unicode() {
        let text = "Hello 世界 world";
        assert_eq!(find_in_text(text, "世界", 0, false), Some(6));
    }

    #[test]
    fn test_replace_unicode() {
        let text = "Hello 世界";
        let (result, count) = replace_all_in_text(text, "世界", "World", false);
        assert_eq!(result, "Hello World");
        assert_eq!(count, 1);
    }
}
