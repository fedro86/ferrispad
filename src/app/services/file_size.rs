//! File size checking utilities for large file handling.
//!
//! Provides pre-flight validation before loading files to prevent crashes
//! from files exceeding FLTK TextBuffer's 2GB limit.

use std::io;
use std::path::Path;

/// Files larger than this show a warning before loading (100 MB)
pub const LARGE_FILE_THRESHOLD: u64 = 100 * 1024 * 1024;

/// Maximum file size that can be loaded into FLTK TextBuffer (~1.8 GB)
/// Set below i32::MAX to leave headroom for style buffer operations
pub const MAX_EDITABLE_SIZE: u64 = 1_800_000_000;

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
}
