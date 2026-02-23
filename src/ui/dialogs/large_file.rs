//! Large file warning and error dialogs.
//!
//! Provides user-facing dialogs for files that may be too large to load safely.

use fltk::dialog;
use std::path::Path;

use crate::app::services::file_size::format_size;

/// Show warning for large file, return true if user wants to proceed
pub fn show_large_file_warning(path: &Path, size: u64) -> bool {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    let msg = format!(
        "Large File Warning\n\n\
        \"{}\" is {} which may take a while to load \
        and use significant memory.\n\n\
        Do you want to continue?",
        filename,
        format_size(size)
    );

    // choice2_default returns Some(0) for first button, Some(1) for second
    dialog::choice2_default(&msg, "Open", "Cancel", "") == Some(0)
}

/// Show error for file that exceeds FLTK limit
pub fn show_file_too_large_error(path: &Path, size: u64) {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    let msg = format!(
        "File Too Large\n\n\
        \"{}\" is {} which exceeds the maximum \
        editable file size (~1.8 GB).\n\n\
        This is a limitation of the text buffer system.\n\
        Consider using a specialized tool like 'less' or 'tail' \
        for viewing large log files.",
        filename,
        format_size(size)
    );

    dialog::alert_default(&msg);
}
