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
    text::TextBuffer,
    window::Window,
};
use std::io::{self, Read};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
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

/// Result of streaming load directly to TextBuffer
pub enum StreamLoadResult {
    /// File loaded successfully into TextBuffer
    Success(TextBuffer),
    /// User cancelled the loading
    Cancelled,
    /// Error reading file
    Error(io::Error),
}

/// Message sent from reader thread to main thread
/// Message sent from reader thread to main thread
enum ChunkMessage {
    /// A chunk of text data
    Chunk(String),
    /// Reading completed successfully
    Done,
    /// Error occurred
    Error(io::Error),
}

/// Load a large file with progress, streaming directly to TextBuffer.
///
/// This is a memory-optimized version that avoids keeping the full file
/// content in memory twice. Chunks are read in a background thread and
/// appended directly to the TextBuffer on the main thread.
///
/// Memory usage: ~1x file size (TextBuffer) + 1MB chunk buffer
/// vs. old method: ~2x file size (String + TextBuffer copy)
pub fn load_to_buffer_with_progress(path: &Path, size: u64) -> StreamLoadResult {
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

    let label_text = format!("Loading \"{}\"...", filename);
    let mut status = Frame::default().with_label(&label_text);
    status.set_label_size(14);
    flex.fixed(&status, 25);

    let mut progress = Progress::default();
    progress.set_minimum(0.0);
    progress.set_maximum(size as f64);
    progress.set_value(0.0);
    flex.fixed(&progress, 25);

    let size_text = format!("0 / {}", format_size(size));
    let mut size_label = Frame::default().with_label(&size_text);
    size_label.set_label_size(11);
    flex.fixed(&size_label, 20);

    flex.end();
    dialog.end();
    dialog.show();

    app::flush();
    app::awake();

    // Create TextBuffer to receive content
    let buffer = TextBuffer::default();

    // Cancellation flag
    let cancelled = Arc::new(AtomicBool::new(false));

    // Channel for receiving chunks from reader thread
    let (tx, rx) = mpsc::channel::<ChunkMessage>();

    let path_owned = path.to_path_buf();
    let cancelled_thread = Arc::clone(&cancelled);

    // Spawn reader thread that sends chunks
    std::thread::spawn(move || {
        read_file_in_chunks(&path_owned, &cancelled_thread, tx);
    });

    let mut progress_bar = progress.clone();
    let mut size_frame = size_label.clone();
    let total_size = size;
    let mut bytes_loaded: u64 = 0;
    let mut buf = buffer.clone();

    // Event loop - process chunks and FLTK events
    while dialog.shown() {
        app::wait_for(0.01).ok(); // 10ms timeout for responsiveness

        // Process all available chunks (don't block)
        loop {
            match rx.try_recv() {
                Ok(ChunkMessage::Chunk(text)) => {
                    bytes_loaded += text.len() as u64;
                    buf.append(&text);

                    // Update progress UI
                    progress_bar.set_value(bytes_loaded as f64);
                    size_frame.set_label(&format!(
                        "{} / {}",
                        format_size(bytes_loaded),
                        format_size(total_size)
                    ));
                }
                Ok(ChunkMessage::Done) => {
                    dialog.hide();
                    return StreamLoadResult::Success(buffer);
                }
                Ok(ChunkMessage::Error(e)) => {
                    dialog.hide();
                    return StreamLoadResult::Error(e);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    dialog.hide();
                    return StreamLoadResult::Error(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "Reader thread disconnected",
                    ));
                }
            }
        }
    }

    // Dialog was closed - signal cancellation
    cancelled.store(true, Ordering::Relaxed);
    StreamLoadResult::Cancelled
}

/// Read file in chunks and send each chunk to the channel.
/// This runs in a background thread.
fn read_file_in_chunks(
    path: &Path,
    cancelled: &AtomicBool,
    tx: mpsc::Sender<ChunkMessage>,
) {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(ChunkMessage::Error(e));
            return;
        }
    };

    let mut reader = std::io::BufReader::with_capacity(1024 * 1024, file);
    let mut buffer = vec![0u8; 1024 * 1024]; // 1MB chunks

    loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = tx.send(ChunkMessage::Error(io::Error::new(
                io::ErrorKind::Interrupted,
                "Cancelled",
            )));
            return;
        }

        let n = match reader.read(&mut buffer) {
            Ok(n) => n,
            Err(e) => {
                let _ = tx.send(ChunkMessage::Error(e));
                return;
            }
        };

        if n == 0 {
            let _ = tx.send(ChunkMessage::Done);
            return;
        }

        // Convert chunk to string (lossy for robustness)
        let text = String::from_utf8_lossy(&buffer[..n]).into_owned();
        if tx.send(ChunkMessage::Chunk(text)).is_err() {
            // Receiver dropped, likely cancelled
            return;
        }
    }
}
