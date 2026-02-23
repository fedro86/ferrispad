//! Large file warning and error dialogs.
//!
//! Provides user-facing dialogs for files that may be too large to load safely.
//! Also provides progress dialog for loading large files.

use fltk::{
    app,
    dialog,
    frame::Frame,
    group::Flex,
    misc::Progress,
    prelude::*,
    window::Window,
};
use std::io::{self, Read};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc,
};

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

/// Result of the "file too large" dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooLargeAction {
    /// User cancelled - don't open the file
    Cancel,
    /// Open the last N lines of the file
    OpenTail,
}

/// Show dialog for file that exceeds FLTK limit, offering tail option
pub fn show_file_too_large_dialog(path: &Path, size: u64) -> TooLargeAction {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    let msg = format!(
        "File Too Large\n\n\
        \"{}\" is {} which exceeds the maximum \
        editable file size (~1.8 GB).\n\n\
        Would you like to open the last 10,000 lines instead?\n\
        This is useful for viewing the end of log files.",
        filename,
        format_size(size)
    );

    match dialog::choice2_default(&msg, "Open Tail", "Cancel", "") {
        Some(0) => TooLargeAction::OpenTail,
        _ => TooLargeAction::Cancel,
    }
}

/// Result of loading a large file with progress
#[derive(Debug)]
pub enum LoadResult {
    /// File loaded successfully
    Success(String),
    /// User cancelled the loading
    Cancelled,
    /// Error reading file
    Error(io::Error),
}

/// Load a large file with a progress dialog.
///
/// Shows a modal dialog with progress bar while reading the file.
/// User can cancel by closing the dialog window.
pub fn load_with_progress(path: &Path, size: u64) -> LoadResult {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();

    // Create progress dialog
    let mut dialog = Window::new(100, 100, 400, 120, "Loading File");
    dialog.make_modal(true);

    let mut flex = Flex::new(10, 10, 380, 100, None);
    flex.set_type(fltk::group::FlexType::Column);
    flex.set_spacing(10);

    // Status label
    let label_text = format!("Loading \"{}\"...", filename);
    let mut status = Frame::default().with_label(&label_text);
    status.set_label_size(14);
    flex.fixed(&status, 25);

    // Progress bar
    let mut progress = Progress::default();
    progress.set_minimum(0.0);
    progress.set_maximum(size as f64);
    progress.set_value(0.0);
    flex.fixed(&progress, 25);

    // Size info
    let size_text = format!("0 / {}", format_size(size));
    let mut size_label = Frame::default().with_label(&size_text);
    size_label.set_label_size(11);
    flex.fixed(&size_label, 20);

    flex.end();
    dialog.end();
    dialog.show();

    // Process pending events to make sure dialog is visible
    app::flush();
    app::awake();

    // Shared state for progress tracking
    let bytes_read = Arc::new(AtomicU64::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));

    // Channel for receiving content from reader thread
    let (tx, rx) = mpsc::channel::<Result<String, io::Error>>();

    // Clone for thread
    let path_owned = path.to_path_buf();
    let bytes_read_thread = Arc::clone(&bytes_read);
    let cancelled_thread = Arc::clone(&cancelled);

    // Spawn reader thread
    std::thread::spawn(move || {
        let result = read_file_with_progress(&path_owned, &bytes_read_thread, &cancelled_thread);
        let _ = tx.send(result);
    });

    // Clone widgets for event loop updates
    let mut progress_bar = progress.clone();
    let mut size_frame = size_label.clone();
    let total_size = size;

    // Event loop - process FLTK events while waiting for file
    while dialog.shown() {
        app::wait_for(0.05).ok(); // 50ms timeout

        // Check if reader thread is done
        if let Ok(result) = rx.try_recv() {
            dialog.hide();
            return match result {
                Ok(content) => LoadResult::Success(content),
                Err(e) => LoadResult::Error(e),
            };
        }

        // Update progress from shared state
        let current_bytes = bytes_read.load(Ordering::Relaxed);
        progress_bar.set_value(current_bytes as f64);
        size_frame.set_label(&format!(
            "{} / {}",
            format_size(current_bytes),
            format_size(total_size)
        ));
    }

    // Dialog was closed - signal cancellation to reader thread
    cancelled.store(true, Ordering::Relaxed);
    LoadResult::Cancelled
}

/// Read file in chunks, updating progress atomically
fn read_file_with_progress(
    path: &Path,
    bytes_read: &AtomicU64,
    cancelled: &AtomicBool,
) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len() as usize;

    // Pre-allocate buffer
    let mut content = Vec::with_capacity(size);

    // Read in 1MB chunks for progress updates
    let chunk_size = 1024 * 1024;
    let mut buffer = vec![0u8; chunk_size];
    let mut total_read: u64 = 0;

    loop {
        // Check for cancellation
        if cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
        }

        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        content.extend_from_slice(&buffer[..n]);
        total_read += n as u64;
        bytes_read.store(total_read, Ordering::Relaxed);
    }

    // Convert to string
    String::from_utf8(content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
