use regex_lite::RegexBuilder;
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
pub fn find_in_text(
    text: &str,
    search: &str,
    start_pos: usize,
    case_sensitive: bool,
) -> Option<usize> {
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
pub fn find_in_text_backward(
    text: &str,
    search: &str,
    start_pos: usize,
    case_sensitive: bool,
) -> Option<usize> {
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
pub fn replace_all_in_text(
    text: &str,
    search: &str,
    replace: &str,
    case_sensitive: bool,
) -> (String, usize) {
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

/// Find next regex match in text, returns (match_start, match_end) byte positions
pub fn find_in_text_regex(
    text: &str,
    pattern: &str,
    start_pos: usize,
    case_sensitive: bool,
) -> Result<Option<(usize, usize)>, String> {
    if pattern.is_empty() {
        return Ok(None);
    }
    let re = RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| e.to_string())?;
    let slice_start = start_pos.min(text.len());
    Ok(re
        .find(&text[slice_start..])
        .map(|m| (slice_start + m.start(), slice_start + m.end())))
}

/// Find last regex match before end_pos (backward search), returns (match_start, match_end)
pub fn find_in_text_regex_backward(
    text: &str,
    pattern: &str,
    end_pos: usize,
    case_sensitive: bool,
) -> Result<Option<(usize, usize)>, String> {
    if pattern.is_empty() {
        return Ok(None);
    }
    let re = RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| e.to_string())?;
    let slice = &text[..end_pos.min(text.len())];
    Ok(re.find_iter(slice).last().map(|m| (m.start(), m.end())))
}

/// Replace all regex matches; replacement may reference capture groups as $1, $2, etc.
pub fn replace_all_in_text_regex(
    text: &str,
    pattern: &str,
    replacement: &str,
    case_sensitive: bool,
) -> Result<(String, usize), String> {
    if pattern.is_empty() {
        return Ok((text.to_string(), 0));
    }
    let re = RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| e.to_string())?;
    let count = re.find_iter(text).count();
    let result = re.replace_all(text, replacement).into_owned();
    Ok((result, count))
}

/// Replace the first regex match in text; replacement may reference capture groups as $1, $2, etc.
pub fn replace_first_regex(
    text: &str,
    pattern: &str,
    replacement: &str,
    case_sensitive: bool,
) -> Result<Option<String>, String> {
    if pattern.is_empty() {
        return Ok(None);
    }
    let re = RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| e.to_string())?;
    if re.is_match(text) {
        Ok(Some(re.replace(text, replacement).into_owned()))
    } else {
        Ok(None)
    }
}

/// Detect indentation style from text content (first 20 lines)
/// Returns (is_spaces, indent_size, has_indentation)
pub fn detect_indent_style(text: &str) -> (bool, u8) {
    let mut space_count = 0;
    let mut tab_count = 0;
    let mut leading_spaces: Vec<usize> = Vec::new();

    let lines: Vec<&str> = text.lines().take(20).collect();

    for line in lines {
        if line.is_empty() || !line.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }

        if line.starts_with('\t') {
            tab_count += 1;
        } else if line.starts_with(' ') {
            space_count += 1;
            let spaces = line.chars().take_while(|c| *c == ' ').count();
            if spaces > 0 {
                leading_spaces.push(spaces);
            }
        }
    }

    let is_spaces = space_count >= tab_count;
    let indent_size = if is_spaces {
        find_most_common_indent(&leading_spaces).unwrap_or(4) as u8
    } else {
        4
    };

    (is_spaces, indent_size)
}

/// Find the most common indentation size from a list of indent lengths
fn find_most_common_indent(indents: &[usize]) -> Option<usize> {
    if indents.is_empty() {
        return None;
    }

    let mut counts = std::collections::HashMap::new();
    for &indent in indents {
        if indent > 0 && indent <= 16 {
            *counts.entry(indent).or_insert(0) += 1;
        }
    }

    counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(indent, _)| indent)
}

/// Calculate nesting level for a line based on opening/closing keywords and braces
pub fn calculate_nesting_level(line: &str, language: &str, previous_level: i32) -> i32 {
    let mut level = previous_level;

    let opening_keywords = match language {
        "py" | "python" => vec!["if", "else", "elif", "for", "while", "def", "class", "with", "try", "except"],
        "js" | "ts" | "jsx" | "tsx" => vec!["if", "else", "for", "while", "function", "class"],
        "rs" | "rust" => vec!["if", "else", "for", "while", "fn", "impl", "mod", "match"],
        _ => vec![],
    };

    let closing_keywords = match language {
        "py" | "python" => vec!["else", "elif", "except", "finally"],
        _ => vec![],
    };

    let trimmed = line.trim();

    // Count closing keywords first
    for kw in &closing_keywords {
        if trimmed.starts_with(kw) {
            level = (level - 1).max(0);
        }
    }

    // Count opening keywords
    for kw in &opening_keywords {
        if trimmed.contains(kw) {
            let trimmed_start = trimmed.split_whitespace().next().unwrap_or("");
            if trimmed_start == *kw || trimmed.starts_with(&format!("{}(", kw)) {
                level += 1;
            }
        }
    }

    // Count braces (general language-agnostic)
    for ch in trimmed.chars() {
        if ch == '{' || ch == '[' || ch == '(' {
            level += 1;
        } else if ch == '}' || ch == ']' || ch == ')' {
            level = (level - 1).max(0);
        }
    }

    level
}

/// Detect language from file extension
pub fn detect_language(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("txt")
        .to_string()
}

/// Generate indentation string for a given nesting level
pub fn generate_indent_string(level: u32, use_spaces: bool, indent_size: u8) -> String {
    let size = indent_size as usize;
    if use_spaces {
        " ".repeat(level as usize * size)
    } else {
        "\t".repeat(level as usize)
    }
}

/// Apply auto-indentation to entire document
/// Analyzes nesting level for each line and applies appropriate indentation
/// Uses detected indentation style if found, otherwise uses user preferences
pub fn auto_indent_document(text: &str, use_spaces: bool, indent_size: u8, language: &str) -> String {
    let (detected_spaces, detected_size) = detect_indent_style(text);

    let final_use_spaces = if !text.is_empty() {
        detected_spaces
    } else {
        use_spaces
    };
    let final_indent_size = if !text.is_empty() {
        detected_size
    } else {
        indent_size
    };

    let mut result = String::new();
    let mut nesting_level: i32 = 0;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            result.push('\n');
            continue;
        }

        // Adjust nesting level based on closing braces/keywords before this line
        if trimmed.starts_with('}')
            || trimmed.starts_with(']')
            || trimmed.starts_with(')')
            || language == "py" && (trimmed.starts_with("else") || trimmed.starts_with("elif") || trimmed.starts_with("except"))
        {
            nesting_level = (nesting_level - 1).max(0);
        }

        let indent = generate_indent_string(nesting_level as u32, final_use_spaces, final_indent_size);
        result.push_str(&indent);
        result.push_str(trimmed);
        result.push('\n');

        nesting_level = calculate_nesting_level(trimmed, language, nesting_level);
    }

    // Remove trailing newline if original didn't have one
    if !text.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_filename_from_path() {
        assert_eq!(extract_filename("/home/user/test.txt"), "test.txt");
        assert_eq!(extract_filename("/home/user/document.md"), "document.md");
        assert_eq!(extract_filename("test.txt"), "test.txt");
        assert_eq!(
            extract_filename("/path/with/many/levels/file.rs"),
            "file.rs"
        );
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

    // Regex find tests

    #[test]
    fn test_find_regex_basic() {
        let text = "foo bar foo";
        let result = find_in_text_regex(text, "foo", 0, true).unwrap();
        assert_eq!(result, Some((0, 3)));
    }

    #[test]
    fn test_find_regex_from_pos() {
        let text = "foo bar foo";
        let result = find_in_text_regex(text, "foo", 1, true).unwrap();
        assert_eq!(result, Some((8, 11)));
    }

    #[test]
    fn test_find_regex_case_insensitive() {
        let text = "Hello HELLO hello";
        let result = find_in_text_regex(text, "hello", 0, false).unwrap();
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn test_find_regex_word_boundary() {
        let text = "foobar foo";
        let result = find_in_text_regex(text, r"\bfoo\b", 0, true).unwrap();
        assert_eq!(result, Some((7, 10)));
    }

    #[test]
    fn test_find_regex_no_match() {
        let text = "hello world";
        let result = find_in_text_regex(text, "xyz", 0, true).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_regex_invalid_pattern() {
        let result = find_in_text_regex("hello", "[", 0, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_regex_empty_pattern() {
        let result = find_in_text_regex("hello", "", 0, true).unwrap();
        assert_eq!(result, None);
    }

    // Regex backward find tests

    #[test]
    fn test_find_regex_backward_basic() {
        let text = "foo bar foo";
        let result = find_in_text_regex_backward(text, "foo", text.len(), true).unwrap();
        assert_eq!(result, Some((8, 11)));
    }

    #[test]
    fn test_find_regex_backward_before_pos() {
        let text = "foo bar foo";
        let result = find_in_text_regex_backward(text, "foo", 8, true).unwrap();
        assert_eq!(result, Some((0, 3)));
    }

    #[test]
    fn test_find_regex_backward_no_match() {
        let text = "foo bar foo";
        let result = find_in_text_regex_backward(text, "xyz", text.len(), true).unwrap();
        assert_eq!(result, None);
    }

    // Regex replace all tests

    #[test]
    fn test_replace_all_regex_basic() {
        let (result, count) = replace_all_in_text_regex("cat cat cat", "cat", "dog", true).unwrap();
        assert_eq!(result, "dog dog dog");
        assert_eq!(count, 3);
    }

    #[test]
    fn test_replace_all_regex_capture_groups() {
        let (result, count) =
            replace_all_in_text_regex("foo bar baz", r"(\w+)", "[$1]", true).unwrap();
        assert_eq!(result, "[foo] [bar] [baz]");
        assert_eq!(count, 3);
    }

    #[test]
    fn test_replace_all_regex_case_insensitive() {
        let (result, count) =
            replace_all_in_text_regex("Cat cat CAT", "cat", "dog", false).unwrap();
        assert_eq!(result, "dog dog dog");
        assert_eq!(count, 3);
    }

    #[test]
    fn test_replace_all_regex_invalid_pattern() {
        let result = replace_all_in_text_regex("hello", "[", "x", true);
        assert!(result.is_err());
    }

    // Regex replace first tests

    #[test]
    fn test_replace_first_regex_basic() {
        let result = replace_first_regex("cat cat cat", "cat", "dog", true).unwrap();
        assert_eq!(result, Some("dog cat cat".to_string()));
    }

    #[test]
    fn test_replace_first_regex_no_match() {
        let result = replace_first_regex("hello", "xyz", "dog", true).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_replace_first_regex_capture_groups() {
        let result = replace_first_regex("hello world", r"(\w+)", "[$1]", true).unwrap();
        assert_eq!(result, Some("[hello] world".to_string()));
    }
}
