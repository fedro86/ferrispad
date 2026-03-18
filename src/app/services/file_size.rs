//! File size checking utilities for large file handling.
//!
//! Provides pre-flight validation before loading files to prevent crashes
//! from very large files that would consume too much memory.
//!
//! Also provides tail reading and chunk reading for viewing portions
//! of very large files.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Default number of lines to read in tail mode
pub const TAIL_LINE_COUNT: usize = 10_000;

/// Result of checking a file's size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSizeCheck {
    /// File is small enough to load normally
    Normal(u64),
    /// File is large but loadable - show warning
    Large(u64),
    /// File exceeds FLTK's limit - cannot be edited
    TooLarge(u64),
}

/// Check file size and return appropriate category.
///
/// Both thresholds are in megabytes.
/// If `warning_mb >= max_editable_mb` the Large tier is effectively skipped.
pub fn check_file_size(
    path: &Path,
    warning_mb: u64,
    max_editable_mb: u64,
) -> io::Result<FileSizeCheck> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();

    let max_editable = max_editable_mb * 1024 * 1024;
    let warning = warning_mb * 1024 * 1024;

    Ok(if size > max_editable {
        FileSizeCheck::TooLarge(size)
    } else if size > warning {
        FileSizeCheck::Large(size)
    } else {
        FileSizeCheck::Normal(size)
    })
}

/// Read the last N lines of a file efficiently.
///
/// Reads from the end of the file backwards in chunks to find newlines,
/// avoiding loading the entire file into memory.
///
/// # Arguments
/// * `path` - Path to the file to read
/// * `lines` - Number of lines to read from the end
///
/// # Returns
/// The last N lines as a String, or an error if the file cannot be read.
pub fn read_tail(path: &Path, lines: usize) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let file_size = file.metadata()?.len();

    if file_size == 0 || lines == 0 {
        return Ok(String::new());
    }

    // For small files, just read the whole thing
    if file_size < 1024 * 1024 {
        let mut content = String::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_string(&mut content)?;

        // Get last N lines
        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(lines);
        return Ok(all_lines[start..].join("\n"));
    }

    // For large files, read backwards in chunks
    let mut chunk_size: u64 = 1024 * 1024; // 1MB
    let mut collected_bytes: Vec<u8> = Vec::new();
    let mut newline_count = 0;
    let mut position = file_size;

    // Read backwards until we have enough newlines (lines + 1 to include the start of the first line)
    while newline_count <= lines && position > 0 {
        let read_size = chunk_size.min(position);
        position = position.saturating_sub(read_size);

        file.seek(SeekFrom::Start(position))?;
        let mut buffer = vec![0u8; read_size as usize];
        file.read_exact(&mut buffer)?;

        // Count newlines in this chunk
        newline_count += buffer.iter().filter(|&&b| b == b'\n').count();

        // Prepend this chunk
        buffer.append(&mut collected_bytes);
        collected_bytes = buffer;

        // Increase chunk size for efficiency
        chunk_size = (chunk_size * 2).min(16 * 1024 * 1024);
    }

    // Convert to string and get last N lines
    let content = String::from_utf8_lossy(&collected_bytes);
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines);
    Ok(all_lines[start..].join("\n"))
}

/// Read a specific line range from a file.
///
/// Reads lines from `start_line` to `end_line` (1-indexed, inclusive).
/// This is useful for opening specific chunks of very large files.
///
/// # Arguments
/// * `path` - Path to the file to read
/// * `start_line` - First line to read (1-indexed)
/// * `end_line` - Last line to read (1-indexed, inclusive)
///
/// # Returns
/// The specified lines as a String, or an error if the file cannot be read.
pub fn read_chunk(path: &Path, start_line: usize, end_line: usize) -> io::Result<String> {
    if start_line == 0 || end_line < start_line {
        return Ok(String::new());
    }

    let file = std::fs::File::open(path)?;
    let reader = io::BufReader::new(file);

    use std::io::BufRead;
    let mut result = String::new();
    let mut current_line = 0;

    for line in reader.lines() {
        current_line += 1;
        if current_line < start_line {
            continue;
        }
        if current_line > end_line {
            break;
        }

        let line = line?;
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&line);
    }

    Ok(result)
}

/// Format file size for display (e.g., "1.5 GB")
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(500), "500 bytes");
    }

    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(150 * 1024 * 1024), "150.0 MB");
    }

    #[test]
    fn test_format_size_gb() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn test_thresholds_default() {
        // Normal: under 50 MB
        assert!(matches!(
            check_file_size_from_value(30 * 1024 * 1024, 50, 150),
            FileSizeCheck::Normal(_)
        ));

        // Large: 50 MB to 150 MB (show warning but allow editing)
        assert!(matches!(
            check_file_size_from_value(100 * 1024 * 1024, 50, 150),
            FileSizeCheck::Large(_)
        ));

        // Too large: over 150 MB (read-only viewer only)
        assert!(matches!(
            check_file_size_from_value(200 * 1024 * 1024, 50, 150),
            FileSizeCheck::TooLarge(_)
        ));
    }

    #[test]
    fn test_thresholds_custom() {
        // Custom: warning at 10 MB, max at 20 MB
        assert!(matches!(
            check_file_size_from_value(5 * 1024 * 1024, 10, 20),
            FileSizeCheck::Normal(_)
        ));
        assert!(matches!(
            check_file_size_from_value(15 * 1024 * 1024, 10, 20),
            FileSizeCheck::Large(_)
        ));
        assert!(matches!(
            check_file_size_from_value(25 * 1024 * 1024, 10, 20),
            FileSizeCheck::TooLarge(_)
        ));
    }

    #[test]
    fn test_thresholds_warning_equals_max() {
        // When warning >= max, Large tier is skipped
        assert!(matches!(
            check_file_size_from_value(40 * 1024 * 1024, 50, 50),
            FileSizeCheck::Normal(_)
        ));
        assert!(matches!(
            check_file_size_from_value(60 * 1024 * 1024, 50, 50),
            FileSizeCheck::TooLarge(_)
        ));
    }

    // Helper for testing without actual files
    fn check_file_size_from_value(
        size: u64,
        warning_mb: u64,
        max_editable_mb: u64,
    ) -> FileSizeCheck {
        let max_editable = max_editable_mb * 1024 * 1024;
        let warning = warning_mb * 1024 * 1024;
        if size > max_editable {
            FileSizeCheck::TooLarge(size)
        } else if size > warning {
            FileSizeCheck::Large(size)
        } else {
            FileSizeCheck::Normal(size)
        }
    }

    #[test]
    fn test_read_tail_basic() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line 1").unwrap();
        writeln!(file, "line 2").unwrap();
        writeln!(file, "line 3").unwrap();
        writeln!(file, "line 4").unwrap();
        writeln!(file, "line 5").unwrap();
        file.flush().unwrap();

        // Read last 3 lines
        let tail = read_tail(file.path(), 3).unwrap();
        assert!(tail.contains("line 3"));
        assert!(tail.contains("line 4"));
        assert!(tail.contains("line 5"));
        assert!(!tail.contains("line 1"));
        assert!(!tail.contains("line 2"));
    }

    #[test]
    fn test_read_tail_more_than_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "only line").unwrap();
        file.flush().unwrap();

        // Request more lines than exist
        let tail = read_tail(file.path(), 100).unwrap();
        assert!(tail.contains("only line"));
    }

    #[test]
    fn test_read_tail_empty_file() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let tail = read_tail(file.path(), 10).unwrap();
        assert!(tail.is_empty());
    }

    #[test]
    fn test_read_chunk_basic() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        for i in 1..=10 {
            writeln!(file, "line {}", i).unwrap();
        }
        file.flush().unwrap();

        // Read lines 3-5
        let chunk = read_chunk(file.path(), 3, 5).unwrap();
        assert!(chunk.contains("line 3"));
        assert!(chunk.contains("line 4"));
        assert!(chunk.contains("line 5"));
        assert!(!chunk.contains("line 1"));
        assert!(!chunk.contains("line 2"));
        assert!(!chunk.contains("line 6"));
    }

    #[test]
    fn test_read_chunk_from_start() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        for i in 1..=5 {
            writeln!(file, "line {}", i).unwrap();
        }
        file.flush().unwrap();

        // Read lines 1-2
        let chunk = read_chunk(file.path(), 1, 2).unwrap();
        assert!(chunk.contains("line 1"));
        assert!(chunk.contains("line 2"));
        assert!(!chunk.contains("line 3"));
    }

    #[test]
    fn test_read_chunk_beyond_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line 1").unwrap();
        writeln!(file, "line 2").unwrap();
        file.flush().unwrap();

        // Request more lines than exist
        let chunk = read_chunk(file.path(), 1, 100).unwrap();
        assert!(chunk.contains("line 1"));
        assert!(chunk.contains("line 2"));
    }

    #[test]
    fn test_read_chunk_invalid_range() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line 1").unwrap();
        file.flush().unwrap();

        // Invalid range (start > end)
        let chunk = read_chunk(file.path(), 5, 2).unwrap();
        assert!(chunk.is_empty());

        // Zero start line
        let chunk = read_chunk(file.path(), 0, 2).unwrap();
        assert!(chunk.is_empty());
    }
}
