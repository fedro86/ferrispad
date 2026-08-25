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

/// Lowercased view of `text[start_pos..]` that remembers where every lowered
/// byte came from in the original string.
///
/// `str::to_lowercase()` is **not** length-preserving (e.g. `İ` U+0130 is 2
/// bytes and lowercases to 3; `ẞ` → `ß`), so an offset found in the lowered
/// string is not a valid index into the original. This map lets a
/// case-insensitive match be translated back to correct original byte offsets.
struct LowerMap {
    lowered: String,
    /// `orig_of[b]` = original byte offset of the character that produced
    /// lowered byte `b`.
    orig_of: Vec<usize>,
    /// `char_start[b]` = true if lowered byte `b` is the first byte of some
    /// original character's lowercasing. Used to reject matches that land on a
    /// proper sub-part of a character's (possibly multi-char) expansion.
    char_start: Vec<bool>,
    /// Original length of `text`, for mapping a match that ends at the very end.
    orig_end: usize,
}

impl LowerMap {
    fn build(text: &str, start_pos: usize) -> Self {
        let mut lowered = String::new();
        let mut orig_of = Vec::new();
        let mut char_start = Vec::new();
        for (i, ch) in text[start_pos..].char_indices() {
            let orig_off = start_pos + i;
            let mut is_first_byte = true;
            for lc in ch.to_lowercase() {
                let mut buf = [0u8; 4];
                let s = lc.encode_utf8(&mut buf);
                for _ in 0..s.len() {
                    orig_of.push(orig_off);
                    char_start.push(is_first_byte);
                    is_first_byte = false;
                }
                lowered.push_str(s);
            }
        }
        Self {
            lowered,
            orig_of,
            char_start,
            orig_end: text.len(),
        }
    }

    /// A lowered offset is "aligned" if it begins an original character (or is
    /// the very end), so a match spans whole original characters.
    fn aligned(&self, b: usize) -> bool {
        b == self.lowered.len() || self.char_start.get(b).copied().unwrap_or(false)
    }

    /// Map a lowered byte offset back to an original byte offset.
    fn orig_at(&self, b: usize) -> usize {
        self.orig_of.get(b).copied().unwrap_or(self.orig_end)
    }

    /// Advance a lowered offset to the next char boundary (so re-slicing on a
    /// rejected match never splits a codepoint).
    fn next_boundary(&self, b: usize) -> usize {
        let mut n = b + 1;
        while n < self.lowered.len() && !self.lowered.is_char_boundary(n) {
            n += 1;
        }
        n
    }
}

/// Case-insensitive forward search returning `(start, end)` byte offsets into
/// the ORIGINAL `text`. Only whole-character matches are reported, so offsets
/// are always valid indices into `text`.
fn find_ci(map: &LowerMap, needle_lower: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(rel) = map.lowered[from..].find(needle_lower) {
        let pos = from + rel;
        let end = pos + needle_lower.len();
        if map.aligned(pos) && map.aligned(end) {
            return Some((map.orig_at(pos), map.orig_at(end)));
        }
        from = map.next_boundary(pos);
        if from >= map.lowered.len() {
            break;
        }
    }
    None
}

/// Case-insensitive backward search returning `(start, end)` original offsets.
fn rfind_ci(map: &LowerMap, needle_lower: &str) -> Option<(usize, usize)> {
    let mut end_limit = map.lowered.len();
    while let Some(pos) = map.lowered[..end_limit].rfind(needle_lower) {
        let end = pos + needle_lower.len();
        if map.aligned(pos) && map.aligned(end) {
            return Some((map.orig_at(pos), map.orig_at(end)));
        }
        if pos == 0 {
            break;
        }
        // Search strictly before this rejected match; keep the slice on a
        // codepoint boundary.
        end_limit = end - 1;
        while end_limit > 0 && !map.lowered.is_char_boundary(end_limit) {
            end_limit -= 1;
        }
    }
    None
}

/// Find next occurrence of search string in text.
///
/// Returns `(start, end)` byte offsets of the match in `text` (valid indices,
/// even for case-insensitive matches where the matched region's byte length
/// differs from `search`), or None if not found. Searches from `start_pos`.
pub fn find_in_text(
    text: &str,
    search: &str,
    start_pos: usize,
    case_sensitive: bool,
) -> Option<(usize, usize)> {
    if search.is_empty() || start_pos >= text.len() {
        return None;
    }
    // Never slice on a non-boundary (start_pos comes from FLTK cursor state).
    let start_pos = floor_char_boundary(text, start_pos);

    if case_sensitive {
        text[start_pos..]
            .find(search)
            .map(|pos| (start_pos + pos, start_pos + pos + search.len()))
    } else {
        let map = LowerMap::build(text, start_pos);
        find_ci(&map, &search.to_lowercase())
    }
}

/// Find previous occurrence of search string in text (backward search).
///
/// Returns `(start, end)` byte offsets of the match in `text`, or None.
/// Searches backwards from `start_pos` (exclusive).
pub fn find_in_text_backward(
    text: &str,
    search: &str,
    start_pos: usize,
    case_sensitive: bool,
) -> Option<(usize, usize)> {
    if search.is_empty() || start_pos == 0 {
        return None;
    }
    // Guard the slice: start_pos may be off a codepoint boundary.
    let end = floor_char_boundary(text, start_pos.min(text.len()));
    let haystack = &text[..end];

    if case_sensitive {
        haystack.rfind(search).map(|pos| (pos, pos + search.len()))
    } else {
        let map = LowerMap::build(haystack, 0);
        rfind_ci(&map, &search.to_lowercase())
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

    // `find_in_text` returns the match's real (start, end) in the original — for
    // a case-insensitive match the byte length can differ from `search.len()`,
    // so we must use `end`, not `start + search.len()`, for the replace range.
    while let Some((start, end)) = find_in_text(&result, search, pos, case_sensitive) {
        result.replace_range(start..end, replace);
        count += 1;

        // Continue past the inserted replacement. `end > start` always holds
        // (matches span whole characters), so the string strictly shrinks when
        // `replace` is empty and `pos` strictly advances otherwise — no infinite
        // loop even when `replace` contains `search`.
        pos = start + replace.len();
    }

    (result, count)
}

/// Snap a byte index to the nearest preceding UTF-8 codepoint boundary.
/// Defensive helper: positions handed in from FLTK should already land on a
/// boundary, but a single-byte miss would panic on `&text[idx..]` slicing.
pub(crate) fn floor_char_boundary(text: &str, mut idx: usize) -> usize {
    if idx >= text.len() {
        return text.len();
    }
    while !text.is_char_boundary(idx) {
        idx = idx.saturating_sub(1);
    }
    idx
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
        .multi_line(true)
        .build()
        .map_err(|e| e.to_string())?;
    let slice_start = floor_char_boundary(text, start_pos);
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
        .multi_line(true)
        .build()
        .map_err(|e| e.to_string())?;
    let slice = &text[..floor_char_boundary(text, end_pos)];
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
        .multi_line(true)
        .build()
        .map_err(|e| e.to_string())?;
    let count = re.find_iter(text).count();
    let result = re.replace_all(text, replacement).into_owned();
    Ok((result, count))
}

/// Replace the regex match starting exactly at `start_pos`, expanding capture
/// groups (`$1`, `${name}`) from the live document — anchors like `^`/`$`
/// resolve against the surrounding text, not the stripped selection.
///
/// Returns `Ok(Some((replacement_text, match_byte_len)))` when there is a
/// match starting at `start_pos`, `Ok(None)` otherwise. The caller should
/// verify (or ignore) that `match_byte_len` matches the current selection.
pub fn replace_at_position_regex(
    text: &str,
    pattern: &str,
    replacement: &str,
    start_pos: usize,
    case_sensitive: bool,
) -> Result<Option<(String, usize)>, String> {
    if pattern.is_empty() {
        return Ok(None);
    }
    let re = RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .multi_line(true)
        .build()
        .map_err(|e| e.to_string())?;
    let start = floor_char_boundary(text, start_pos);
    let Some(caps) = re.captures_at(text, start) else {
        return Ok(None);
    };
    let m = match caps.get(0) {
        Some(m) if m.start() == start => m,
        _ => return Ok(None),
    };
    let mut expanded = String::new();
    caps.expand(replacement, &mut expanded);
    Ok(Some((expanded, m.end() - m.start())))
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
        .multi_line(true)
        .build()
        .map_err(|e| e.to_string())?;
    if re.is_match(text) {
        Ok(Some(re.replace(text, replacement).into_owned()))
    } else {
        Ok(None)
    }
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
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn test_find_case_sensitive() {
        let text = "Hello world, hello Rust, hello FerrisPad";
        let search = "Hello";
        let result = find_in_text(text, search, 0, true);
        assert_eq!(result, Some((0, 5)));
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
        assert_eq!(result, Some((18, 21)));
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

    // Regression (T0013, audit S6): `İ` (U+0130) lowercases to 2 chars / 3 bytes,
    // so an offset taken from the lowercased haystack is not a valid index into
    // the original. Case-insensitive find must return offsets valid for `text`.
    #[test]
    fn test_find_ci_maps_offsets_back_to_original() {
        let text = "aİb"; // bytes: a=0, İ=1..3, b=3..4
        let found = find_in_text(text, "b", 0, false);
        assert_eq!(found, Some((3, 4)));
        let (s, e) = found.unwrap();
        assert_eq!(&text[s..e], "b"); // valid slice, no panic
    }

    // Replacing next to such a character must not panic (old code sliced out of
    // bounds) or corrupt: the offset from the lowercased haystack was wrong.
    #[test]
    fn test_replace_ci_near_multibyte_lowercasing() {
        let (result, count) = replace_all_in_text("aİb", "b", "X", false);
        assert_eq!(result, "aİX");
        assert_eq!(count, 1);
    }

    // A whole-character case-insensitive match maps to the full original span.
    #[test]
    fn test_replace_ci_replaces_full_original_char() {
        let needle = "İ".to_lowercase(); // "i̇" (i + combining dot)
        let (result, count) = replace_all_in_text("aİb", &needle, "I", false);
        assert_eq!(result, "aIb");
        assert_eq!(count, 1);
    }

    // Replace-all over a string mixing multi-byte-lowercasing characters.
    #[test]
    fn test_replace_all_ci_mixed_multibyte() {
        // x at original 0, 3, 6; İ at 1..3; ı (U+0131) at 4..6.
        let (result, count) = replace_all_in_text("xİxıx", "x", "-", false);
        assert_eq!(result, "-İ-ı-");
        assert_eq!(count, 3);
    }

    // Backward search must also return valid original offsets and never slice
    // off a codepoint boundary.
    #[test]
    fn test_find_backward_ci_multibyte() {
        let text = "aİb";
        let found = find_in_text_backward(text, "b", text.len(), false);
        assert_eq!(found, Some((3, 4)));
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
        assert_eq!(result, Some((18, 21)));
    }

    #[test]
    fn test_find_backward_from_middle() {
        let text = "cat dog cat mouse cat";
        // Search backward from position 18 (last "cat"), should find middle "cat"
        let result = find_in_text_backward(text, "cat", 18, false);
        assert_eq!(result, Some((8, 11)));
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
        assert_eq!(result, Some((12, 17)));
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
        // 世界 = two 3-byte chars starting at byte 6.
        assert_eq!(find_in_text(text, "世界", 0, false), Some((6, 12)));
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

    // Multi-line anchor tests — `^`/`$` should match per-line, not per-buffer.

    #[test]
    fn test_find_regex_anchor_start_of_line() {
        let text = "first\nsecond\nthird";
        let result = find_in_text_regex(text, r"^second", 0, true).unwrap();
        assert_eq!(result, Some((6, 12)));
    }

    #[test]
    fn test_find_regex_anchor_end_of_line() {
        let text = "first\nsecond\nthird";
        let result = find_in_text_regex(text, r"second$", 0, true).unwrap();
        assert_eq!(result, Some((6, 12)));
    }

    #[test]
    fn test_replace_all_regex_per_line_anchor() {
        let (result, count) = replace_all_in_text_regex("a\nb\nc", "^", "> ", true).unwrap();
        // ^ matches before each line, so every line gets the prefix.
        assert_eq!(result, "> a\n> b\n> c");
        assert_eq!(count, 3);
    }

    // replace_at_position_regex — anchor-aware "replace current selection"

    #[test]
    fn test_replace_at_position_basic() {
        let text = "foo bar foo";
        let r = replace_at_position_regex(text, "foo", "baz", 8, true).unwrap();
        assert_eq!(r, Some(("baz".to_string(), 3)));
    }

    #[test]
    fn test_replace_at_position_only_matches_at_start() {
        // No match starting exactly at byte 1 — there's a match at 0 but we
        // require start to align.
        let text = "foo";
        let r = replace_at_position_regex(text, "foo", "x", 1, true).unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn test_replace_at_position_capture_groups() {
        let text = "alice bob";
        let r = replace_at_position_regex(text, r"(\w+)", "[$1]", 6, true).unwrap();
        assert_eq!(r, Some(("[bob]".to_string(), 3)));
    }

    #[test]
    fn test_replace_at_position_anchor_uses_full_context() {
        // `^foo` only matches `foo` at the start of a line. Position 6 is the
        // start of "foo" on the second line, so the anchor resolves correctly
        // because the regex sees the surrounding newline.
        let text = "bar\nfoo";
        let r = replace_at_position_regex(text, "^foo", "baz", 4, true).unwrap();
        assert_eq!(r, Some(("baz".to_string(), 3)));
    }

    #[test]
    fn test_replace_at_position_invalid_pattern() {
        let r = replace_at_position_regex("text", "[", "x", 0, true);
        assert!(r.is_err());
    }

    #[test]
    fn test_replace_at_position_no_match() {
        let r = replace_at_position_regex("hello", "xyz", "x", 0, true).unwrap();
        assert_eq!(r, None);
    }
}
