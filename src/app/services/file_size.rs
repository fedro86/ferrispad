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

/// Hard ceiling for editable files.  FLTK's `Fl_Text_Buffer` uses 32-bit
/// `int` for buffer positions, so anything ≥ 2 GiB (2^31) overflows and
/// crashes.  We cap at 1.9 GiB to leave room for the gap-buffer overhead.
const FLTK_BUFFER_LIMIT: u64 = 1_900 * 1024 * 1024; // 1.9 GiB

/// Check file size and return appropriate category.
///
/// Both thresholds are in megabytes.
/// If `warning_mb >= max_editable_mb` the Large tier is effectively skipped.
/// Files ≥ 1.9 GiB are always `TooLarge` regardless of user settings
/// (FLTK `TextBuffer` 32-bit limit).
pub fn check_file_size(
    path: &Path,
    warning_mb: u64,
    max_editable_mb: u64,
) -> io::Result<FileSizeCheck> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();

    let max_editable = (max_editable_mb * 1024 * 1024).min(FLTK_BUFFER_LIMIT);
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
/// Returns `(content, start_byte)` where `start_byte` is the byte offset in
/// the original file where the returned content begins. This offset is used
/// by `save_partial` to write edits back to the correct position.
pub fn read_tail(path: &Path, lines: usize) -> io::Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let file_size = file.metadata()?.len();

    if file_size == 0 || lines == 0 {
        return Ok((String::new(), file_size));
    }

    // For small files, just read the whole thing
    if file_size < 1024 * 1024 {
        let mut content = String::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_string(&mut content)?;

        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(lines);
        let tail = all_lines[start..].join("\n");
        // Compute byte offset of the first returned line
        let skipped_bytes: usize = all_lines[..start]
            .iter()
            .map(|l| l.len() + 1) // +1 for newline
            .sum();
        return Ok((tail, skipped_bytes as u64));
    }

    // For large files, read backwards in chunks
    let mut chunk_size: u64 = 1024 * 1024; // 1MB
    let mut collected_bytes: Vec<u8> = Vec::new();
    let mut newline_count = 0;
    let mut position = file_size;

    while newline_count <= lines && position > 0 {
        let read_size = chunk_size.min(position);
        position = position.saturating_sub(read_size);

        file.seek(SeekFrom::Start(position))?;
        let mut buffer = vec![0u8; read_size as usize];
        file.read_exact(&mut buffer)?;

        newline_count += buffer.iter().filter(|&&b| b == b'\n').count();

        buffer.append(&mut collected_bytes);
        collected_bytes = buffer;

        chunk_size = (chunk_size * 2).min(16 * 1024 * 1024);
    }

    // Convert to string and get last N lines
    let content = String::from_utf8_lossy(&collected_bytes);
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines);
    let tail = all_lines[start..].join("\n");

    // Compute absolute byte offset: position is where we started reading,
    // plus the bytes of the lines we're skipping within collected_bytes.
    let skipped_bytes: usize = all_lines[..start]
        .iter()
        .map(|l| l.len() + 1)
        .sum();
    let start_byte = position + skipped_bytes as u64;

    Ok((tail, start_byte))
}

/// Read a specific line range from a file.
///
/// Reads lines from `start_line` to `end_line` (1-indexed, inclusive).
///
/// Returns `(content, start_byte, end_byte)` where `start_byte` and
/// `end_byte` are the byte offsets of the chunk in the original file.
/// Used by `save_partial` to splice edits back into the correct position.
pub fn read_chunk(
    path: &Path,
    start_line: usize,
    end_line: usize,
) -> io::Result<(String, u64, u64)> {
    if start_line == 0 || end_line < start_line {
        return Ok((String::new(), 0, 0));
    }

    let file = std::fs::File::open(path)?;
    let mut reader = io::BufReader::new(file);

    use std::io::BufRead;
    let mut result = String::new();
    let mut current_line = 0usize;
    let mut byte_pos: u64 = 0;
    let mut start_byte: u64 = 0;
    let mut line_buf = String::new();

    loop {
        line_buf.clear();
        let n = reader.read_line(&mut line_buf)?;
        if n == 0 {
            break; // EOF
        }

        current_line += 1;

        if current_line < start_line {
            byte_pos += n as u64;
            continue;
        }

        if current_line == start_line {
            start_byte = byte_pos;
        }

        byte_pos += n as u64;

        if current_line > end_line {
            break;
        }

        if !result.is_empty() {
            result.push('\n');
        }
        // Strip the trailing newline from read_line
        let trimmed = line_buf.trim_end_matches('\n').trim_end_matches('\r');
        result.push_str(trimmed);
    }

    let end_byte = if current_line <= end_line {
        byte_pos // hit EOF before end_line
    } else {
        // byte_pos is past the line after end_line; end_byte is where end_line ends
        byte_pos - line_buf.len() as u64
    };

    Ok((result, start_byte, end_byte))
}

/// Save edited content back to the correct position in the original file.
///
/// - **Tail** (`end_byte == file_size`): seeks to `start_byte`, writes content,
///   and truncates.  Instant regardless of file size.
/// - **Chunk, same byte length**: overwrites in place.  Instant.
/// - **Chunk, different byte length**: writes prefix + content + suffix to a
///   temp file in the same directory, then atomically renames over the original.
pub fn save_partial(
    path: &Path,
    content: &str,
    start_byte: u64,
    end_byte: u64,
) -> io::Result<()> {
    use std::fs::{File, OpenOptions};
    use std::io::Write;

    let file_size = std::fs::metadata(path)?.len();
    let original_len = end_byte - start_byte;
    let new_len = content.len() as u64;
    let is_tail = end_byte >= file_size;

    if is_tail || original_len == new_len {
        // Fast path: seek + overwrite (+ truncate for tail / size change)
        let mut file = OpenOptions::new().write(true).open(path)?;
        file.seek(SeekFrom::Start(start_byte))?;
        file.write_all(content.as_bytes())?;

        let new_file_size = if is_tail {
            start_byte + new_len
        } else {
            file_size // same-size chunk: file size unchanged
        };
        file.set_len(new_file_size)?;
        file.flush()?;
        return Ok(());
    }

    // Slow path: content length changed for a mid-file chunk.
    // Write prefix + new content + suffix to a temp file, then rename.
    let dir = path.parent().unwrap_or(Path::new("."));
    let temp_path = dir.join(format!(
        ".ferrispad_save_{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("tmp")
    ));

    {
        let mut src = File::open(path)?;
        let mut dst = File::create(&temp_path)?;
        const BUF: usize = 8 * 1024 * 1024; // 8 MB copy buffer

        // Copy prefix [0..start_byte)
        let mut remaining = start_byte;
        let mut buf = vec![0u8; BUF];
        while remaining > 0 {
            let to_read = (remaining as usize).min(BUF);
            src.read_exact(&mut buf[..to_read])?;
            dst.write_all(&buf[..to_read])?;
            remaining -= to_read as u64;
        }

        // Write edited content
        dst.write_all(content.as_bytes())?;

        // Copy suffix [end_byte..file_size)
        src.seek(SeekFrom::Start(end_byte))?;
        remaining = file_size - end_byte;
        while remaining > 0 {
            let to_read = (remaining as usize).min(BUF);
            src.read_exact(&mut buf[..to_read])?;
            dst.write_all(&buf[..to_read])?;
            remaining -= to_read as u64;
        }

        dst.flush()?;
    }

    // Atomic rename (same filesystem)
    std::fs::rename(&temp_path, path)?;

    Ok(())
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

        let (tail, start_byte) = read_tail(file.path(), 3).unwrap();
        assert!(tail.contains("line 3"));
        assert!(tail.contains("line 4"));
        assert!(tail.contains("line 5"));
        assert!(!tail.contains("line 1"));
        assert!(!tail.contains("line 2"));
        // "line 1\n" + "line 2\n" = 14 bytes skipped
        assert_eq!(start_byte, 14);
    }

    #[test]
    fn test_read_tail_more_than_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "only line").unwrap();
        file.flush().unwrap();

        let (tail, start_byte) = read_tail(file.path(), 100).unwrap();
        assert!(tail.contains("only line"));
        assert_eq!(start_byte, 0); // entire file returned
    }

    #[test]
    fn test_read_tail_empty_file() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let (tail, _) = read_tail(file.path(), 10).unwrap();
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

        let (chunk, start_byte, end_byte) = read_chunk(file.path(), 3, 5).unwrap();
        assert!(chunk.contains("line 3"));
        assert!(chunk.contains("line 4"));
        assert!(chunk.contains("line 5"));
        assert!(!chunk.contains("line 1"));
        assert!(!chunk.contains("line 2"));
        assert!(!chunk.contains("line 6"));
        assert!(start_byte > 0);
        assert!(end_byte > start_byte);
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

        let (chunk, start_byte, _) = read_chunk(file.path(), 1, 2).unwrap();
        assert!(chunk.contains("line 1"));
        assert!(chunk.contains("line 2"));
        assert!(!chunk.contains("line 3"));
        assert_eq!(start_byte, 0);
    }

    #[test]
    fn test_read_chunk_beyond_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line 1").unwrap();
        writeln!(file, "line 2").unwrap();
        file.flush().unwrap();

        let (chunk, _, _) = read_chunk(file.path(), 1, 100).unwrap();
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

        let (chunk, _, _) = read_chunk(file.path(), 5, 2).unwrap();
        assert!(chunk.is_empty());

        let (chunk, _, _) = read_chunk(file.path(), 0, 2).unwrap();
        assert!(chunk.is_empty());
    }
}
