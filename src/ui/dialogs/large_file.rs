//! Large file warning and error dialogs.
//!
//! Provides user-facing dialogs for files that may be too large to load safely.
//! Also provides progress dialog for loading large files.

use super::{DialogTheme, darken, lighten};
use fltk::{
    app,
    button::Button,
    dialog,
    enums::{Align, FrameType},
    frame::Frame,
    group::Flex,
    input::Input,
    misc::Progress,
    prelude::*,
    text::TextBuffer,
    window::Window,
};
use std::cell::RefCell;
use std::io::{self, Read};
use std::path::Path;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use crate::app::services::file_size::format_size;

/// Show warning for large file, return true if user wants to proceed
pub fn show_large_file_warning(path: &Path, size: u64) -> bool {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TooLargeAction {
    /// User cancelled - don't open the file
    Cancel,
    /// Open the last N lines of the file
    OpenTail,
    /// Open in read-only viewer mode
    ViewReadOnly,
    /// Open a specific line range (start_line, end_line)
    OpenChunk(usize, usize),
}

/// Show dialog for file that exceeds the configured max editable size, offering view and tail options
pub fn show_file_too_large_dialog(
    path: &Path,
    size: u64,
    theme_bg: (u8, u8, u8),
    max_editable_mb: u32,
) -> TooLargeAction {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();

    let theme = DialogTheme::from_theme_bg(theme_bg);

    // Use Rc<RefCell> to store the result from button callbacks
    let result = Rc::new(RefCell::new(TooLargeAction::Cancel));

    let mut dialog = Window::new(100, 100, 450, 280, "File Too Large");
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    let mut main_flex = Flex::new(15, 15, 420, 250, None);
    main_flex.set_type(fltk::group::FlexType::Column);
    main_flex.set_spacing(10);

    // Message
    let msg = format!(
        "\"{}\" is {} which exceeds the maximum\n\
        editable file size ({} MB).\n\n\
        Choose an option:",
        filename,
        format_size(size),
        max_editable_mb
    );
    let mut msg_frame = Frame::default().with_label(&msg);
    msg_frame.set_align(Align::Left | Align::Inside | Align::Wrap);
    msg_frame.set_label_color(theme.text);
    msg_frame.set_frame(FrameType::FlatBox);
    msg_frame.set_color(theme.bg);
    main_flex.fixed(&msg_frame, 80);

    // View Read-Only button
    let mut view_btn =
        Button::default().with_label("View Read-Only (browse entire file, no editing)");
    view_btn.set_frame(FrameType::RFlatBox);
    view_btn.set_color(theme.button_bg);
    view_btn.set_label_color(theme.text);
    main_flex.fixed(&view_btn, 30);

    // Open Tail button
    let mut tail_btn = Button::default().with_label("Open Tail (last 10,000 lines, editable)");
    tail_btn.set_frame(FrameType::RFlatBox);
    tail_btn.set_color(theme.button_bg);
    tail_btn.set_label_color(theme.text);
    main_flex.fixed(&tail_btn, 30);

    // Open Lines row
    let mut lines_row = Flex::default();
    lines_row.set_type(fltk::group::FlexType::Row);
    lines_row.set_spacing(5);

    let mut lines_btn = Button::default().with_label("Open Lines:");
    lines_btn.set_frame(FrameType::RFlatBox);
    lines_btn.set_color(theme.button_bg);
    lines_btn.set_label_color(theme.text);
    lines_row.fixed(&lines_btn, 100);

    let mut start_input = Input::default();
    start_input.set_value("1");
    start_input.set_tooltip("Start line");
    start_input.set_color(theme.input_bg);
    start_input.set_text_color(theme.text);
    start_input.set_frame(FrameType::FlatBox);
    lines_row.fixed(&start_input, 80);

    let mut dash_label = Frame::default().with_label("-");
    dash_label.set_label_color(theme.text);
    lines_row.fixed(&dash_label, 20);

    let mut end_input = Input::default();
    end_input.set_value("10000");
    end_input.set_tooltip("End line");
    end_input.set_color(theme.input_bg);
    end_input.set_text_color(theme.text);
    end_input.set_frame(FrameType::FlatBox);
    lines_row.fixed(&end_input, 80);

    Frame::default(); // Spacer

    lines_row.end();
    main_flex.fixed(&lines_row, 30);

    Frame::default(); // Spacer

    // Cancel button — visually distinct (secondary)
    // Derive cancel bg from theme_bg the same way DialogTheme derives dialog bg
    let mut cancel_btn = Button::default().with_label("Cancel");
    let (r, g, b) = theme_bg;
    let brightness = (r as u32 + g as u32 + b as u32) / 3;
    let is_dark = brightness < 128;
    let (bg_r, bg_g, bg_b) = if is_dark {
        darken(r, g, b, 0.65)
    } else {
        darken(r, g, b, 0.85)
    };
    let cancel_bg = if is_dark {
        let (cr, cg, cb) = lighten(bg_r, bg_g, bg_b, 0.10);
        fltk::enums::Color::from_rgb(cr, cg, cb)
    } else {
        let (cr, cg, cb) = darken(bg_r, bg_g, bg_b, 0.90);
        fltk::enums::Color::from_rgb(cr, cg, cb)
    };
    cancel_btn.set_frame(FrameType::RFlatBox);
    cancel_btn.set_color(cancel_bg);
    cancel_btn.set_label_color(theme.text_dim);
    main_flex.fixed(&cancel_btn, 30);

    main_flex.end();
    dialog.end();
    dialog.show();

    // Set up callbacks
    let result_view = Rc::clone(&result);
    let mut dialog_view = dialog.clone();
    view_btn.set_callback(move |_| {
        *result_view.borrow_mut() = TooLargeAction::ViewReadOnly;
        dialog_view.hide();
    });

    let result_tail = Rc::clone(&result);
    let mut dialog_tail = dialog.clone();
    tail_btn.set_callback(move |_| {
        *result_tail.borrow_mut() = TooLargeAction::OpenTail;
        dialog_tail.hide();
    });

    let result_lines = Rc::clone(&result);
    let mut dialog_lines = dialog.clone();
    let start_input_clone = start_input.clone();
    let end_input_clone = end_input.clone();
    lines_btn.set_callback(move |_| {
        let start = start_input_clone
            .value()
            .trim()
            .parse::<usize>()
            .unwrap_or(1);
        let end = end_input_clone
            .value()
            .trim()
            .parse::<usize>()
            .unwrap_or(10000);
        *result_lines.borrow_mut() = TooLargeAction::OpenChunk(start, end);
        dialog_lines.hide();
    });

    let mut dialog_cancel = dialog.clone();
    cancel_btn.set_callback(move |_| {
        dialog_cancel.hide();
    });

    // Run dialog event loop
    while dialog.shown() {
        app::wait();
        if app::should_program_quit() {
            dialog.hide();
        }
    }

    // Return the result
    Rc::try_unwrap(result)
        .unwrap_or_else(|rc| RefCell::new(rc.borrow().clone()))
        .into_inner()
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
fn read_file_in_chunks(path: &Path, cancelled: &AtomicBool, tx: mpsc::Sender<ChunkMessage>) {
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
