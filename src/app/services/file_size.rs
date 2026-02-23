//! File size checking utilities for large file handling.
//!
//! Provides pre-flight validation before loading files to prevent crashes
//! from files exceeding FLTK TextBuffer's 2GB limit.
//!
//! Also provides tail reading for viewing the end of very large files.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Files larger than this show a warning before loading (100 MB)
pub const LARGE_FILE_THRESHOLD: u64 = 100 * 1024 * 1024;

/// Maximum file size that can be loaded into FLTK TextBuffer (~1.8 GB)
/// Set below i32::MAX to leave headroom for style buffer operations
pub const MAX_EDITABLE_SIZE: u64 = 1_800_000_000;

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

/// Check file size and return appropriate category
pub fn check_file_size(path: &Path) -> io::Result<FileSizeCheck> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();

    Ok(if size > MAX_EDITABLE_SIZE {
        FileSizeCheck::TooLarge(size)
    } else if size > LARGE_FILE_THRESHOLD {
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
    fn test_thresholds() {
        // Normal: under 100 MB
        assert!(matches!(
            check_file_size_from_value(50 * 1024 * 1024),
            FileSizeCheck::Normal(_)
        ));

        // Large: 100 MB to 1.8 GB
        assert!(matches!(
            check_file_size_from_value(500 * 1024 * 1024),
            FileSizeCheck::Large(_)
        ));

        // Too large: over 1.8 GB
        assert!(matches!(
            check_file_size_from_value(2_000_000_000),
            FileSizeCheck::TooLarge(_)
        ));
    }

    // Helper for testing without actual files
    fn check_file_size_from_value(size: u64) -> FileSizeCheck {
        if size > MAX_EDITABLE_SIZE {
            FileSizeCheck::TooLarge(size)
        } else if size > LARGE_FILE_THRESHOLD {
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
}
