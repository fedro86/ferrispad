//! Read-only viewer for files too large to edit.
//!
//! Uses memory-mapped file access to view files larger than 1.8GB
//! without loading them entirely into memory. Supports navigation
//! by page, line number, and search.

use fltk::{
    app,
    button::Button,
    dialog,
    enums::{Align, Event, Font, Key},
    frame::Frame,
    group::Flex,
    input::Input,
    prelude::*,
    text::{TextBuffer, TextDisplay, WrapMode},
    window::Window,
};
use memmap2::Mmap;
use std::cell::RefCell;
use std::fs::File;
use std::path::Path;
use std::rc::Rc;

use crate::app::services::file_size::format_size;

/// Size of each page in bytes (1MB chunks for smooth scrolling)
const PAGE_SIZE: usize = 1024 * 1024;

/// Shared state for the viewer
struct ViewerState {
    mmap: Mmap,
    file_size: usize,
    total_pages: usize,
    current_page: usize,
    search_pos: usize,
}

impl ViewerState {
    fn new(mmap: Mmap, file_size: usize) -> Self {
        let total_pages = (file_size + PAGE_SIZE - 1) / PAGE_SIZE;
        Self {
            mmap,
            file_size,
            total_pages,
            current_page: 0,
            search_pos: 0,
        }
    }

    fn load_current_page(&self, buffer: &mut TextBuffer, pos_label: &mut Frame) {
        let start = self.current_page * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(self.file_size);

        if start >= self.file_size {
            return;
        }

        // Get the slice from memory map
        let slice = &self.mmap[start..end];

        // Convert to string (lossy for binary files)
        let text = String::from_utf8_lossy(slice);
        buffer.set_text(&text);

        // Count line numbers at start and end of current view
        let lines_before: usize = self.mmap[..start].iter().filter(|&&b| b == b'\n').count();
        let lines_in_page: usize = slice.iter().filter(|&&b| b == b'\n').count();
        let start_line = lines_before + 1;
        let end_line = start_line + lines_in_page;

        pos_label.set_label(&format!(
            "Page {}/{} | Lines {}-{} | Offset: {}",
            self.current_page + 1,
            self.total_pages,
            start_line,
            end_line,
            format_size(start as u64)
        ));
    }

    fn go_to_page(&mut self, page: usize, buffer: &mut TextBuffer, pos_label: &mut Frame) {
        if page < self.total_pages {
            self.current_page = page;
            self.load_current_page(buffer, pos_label);
        }
    }

    fn prev_page(&mut self, buffer: &mut TextBuffer, pos_label: &mut Frame) {
        if self.current_page > 0 {
            self.current_page -= 1;
            self.load_current_page(buffer, pos_label);
        }
    }

    fn next_page(&mut self, buffer: &mut TextBuffer, pos_label: &mut Frame) {
        if self.current_page < self.total_pages - 1 {
            self.current_page += 1;
            self.load_current_page(buffer, pos_label);
        }
    }

    /// Find byte offset for a given line number
    fn find_line_offset(&self, target_line: usize) -> usize {
        if target_line <= 1 {
            return 0;
        }
        let mut line_count = 1usize;
        for (i, &byte) in self.mmap.iter().enumerate() {
            if line_count >= target_line {
                return i;
            }
            if byte == b'\n' {
                line_count += 1;
            }
        }
        self.file_size
    }

    fn go_to_line(
        &mut self,
        target_line: usize,
        buffer: &mut TextBuffer,
        pos_label: &mut Frame,
    ) {
        if target_line == 0 {
            return;
        }
        let byte_offset = self.find_line_offset(target_line);
        let target_page = byte_offset / PAGE_SIZE;
        self.go_to_page(target_page, buffer, pos_label);
    }

    /// Load a specific line range into buffer (for chunk-based viewing)
    fn load_line_range(
        &mut self,
        start_line: usize,
        end_line: usize,
        buffer: &mut TextBuffer,
        pos_label: &mut Frame,
    ) {
        if start_line == 0 || end_line < start_line {
            return;
        }

        let start_offset = self.find_line_offset(start_line);
        let end_offset = self.find_line_offset(end_line + 1); // Include end line

        if start_offset >= self.file_size {
            return;
        }

        let end_offset = end_offset.min(self.file_size);
        let slice = &self.mmap[start_offset..end_offset];
        let text = String::from_utf8_lossy(slice);
        buffer.set_text(&text);

        // Update current page to match start
        self.current_page = start_offset / PAGE_SIZE;

        pos_label.set_label(&format!(
            "Lines {}-{} | Offset: {}-{}",
            start_line,
            end_line,
            format_size(start_offset as u64),
            format_size(end_offset as u64)
        ));
    }

    fn search(
        &mut self,
        query: &str,
        from_start: bool,
        buffer: &mut TextBuffer,
        pos_label: &mut Frame,
        display: &mut TextDisplay,
    ) -> bool {
        if query.is_empty() {
            return false;
        }

        let query_bytes = query.as_bytes();
        let search_start = if from_start { 0 } else { self.search_pos };

        if search_start >= self.file_size {
            return false;
        }

        // Simple byte search
        let slice = &self.mmap[search_start..];
        if let Some(rel_pos) = slice
            .windows(query_bytes.len())
            .position(|w| w == query_bytes)
        {
            let found_pos = search_start + rel_pos;
            self.search_pos = found_pos + query_bytes.len();

            // Navigate to the page containing this match
            let target_page = found_pos / PAGE_SIZE;
            self.go_to_page(target_page, buffer, pos_label);

            // Position cursor at the match in the current view
            let page_start = self.current_page * PAGE_SIZE;
            let local_pos = (found_pos - page_start) as i32;
            display.set_insert_position(local_pos);
            display.show_insert_position();

            return true;
        }
        false
    }
}

/// Open a read-only viewer for a large file.
///
/// This creates a modal window with:
/// - Memory-mapped file access (no full load into RAM)
/// - Page-based navigation
/// - Go to line number
/// - Search functionality
/// - Display of current position/total size
pub fn show_readonly_viewer(path: &Path) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            dialog::alert_default(&format!("Failed to open file: {}", e));
            return;
        }
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(e) => {
            dialog::alert_default(&format!("Failed to read file metadata: {}", e));
            return;
        }
    };

    let file_size = metadata.len() as usize;
    if file_size == 0 {
        dialog::alert_default("File is empty.");
        return;
    }

    // Memory-map the file
    // SAFETY: We open the file read-only and don't modify it while mapped.
    // The file remains open for the duration of the viewer window.
    let mmap = match unsafe { Mmap::map(&file) } {
        Ok(m) => m,
        Err(e) => {
            dialog::alert_default(&format!("Failed to memory-map file: {}", e));
            return;
        }
    };

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    // Create shared state
    let state = Rc::new(RefCell::new(ViewerState::new(mmap, file_size)));

    // Create viewer window
    let title = format!("{} (Read-Only Viewer)", filename);
    let mut window = Window::new(100, 100, 900, 700, None);
    window.set_label(&title);
    window.make_modal(true);

    let mut main_flex = Flex::new(5, 5, 890, 690, None);
    main_flex.set_type(fltk::group::FlexType::Column);
    main_flex.set_spacing(5);

    // Top toolbar
    let mut toolbar = Flex::default();
    toolbar.set_type(fltk::group::FlexType::Row);
    toolbar.set_spacing(5);

    let mut prev_btn = Button::default().with_label("@<  Prev");
    prev_btn.set_tooltip("Previous page (Page Up)");
    toolbar.fixed(&prev_btn, 80);

    let mut next_btn = Button::default().with_label("Next  @>");
    next_btn.set_tooltip("Next page (Page Down)");
    toolbar.fixed(&next_btn, 80);

    Frame::default(); // Spacer

    let mut goto_input = Input::default();
    goto_input.set_tooltip("Line number or range (e.g., 100 or 100-200)");
    toolbar.fixed(&goto_input, 100);

    let mut goto_btn = Button::default().with_label("Go to Line");
    toolbar.fixed(&goto_btn, 80);

    Frame::default(); // Spacer

    let mut search_input = Input::default();
    search_input.set_tooltip("Search text (Enter to find next)");
    toolbar.fixed(&search_input, 150);

    let mut search_btn = Button::default().with_label("Find");
    toolbar.fixed(&search_btn, 60);

    let mut search_next_btn = Button::default().with_label("Next");
    search_next_btn.set_tooltip("Find next (F3)");
    toolbar.fixed(&search_next_btn, 60);

    toolbar.end();
    main_flex.fixed(&toolbar, 30);

    // Text display area with line numbers
    let mut display = TextDisplay::default();
    let buffer = TextBuffer::default();
    display.set_buffer(buffer.clone());
    display.set_text_font(Font::Courier);
    display.set_text_size(14);
    display.wrap_mode(WrapMode::None, 0);
    display.set_linenumber_width(60);
    display.set_linenumber_font(Font::Courier);
    display.set_linenumber_size(12);

    // Status bar
    let mut status_flex = Flex::default();
    status_flex.set_type(fltk::group::FlexType::Row);

    let mut position_label = Frame::default();
    position_label.set_align(Align::Left | Align::Inside);
    status_flex.fixed(&position_label, 300);

    let mut size_label = Frame::default();
    size_label.set_align(Align::Right | Align::Inside);
    size_label.set_label(&format!("Total: {}", format_size(file_size as u64)));

    status_flex.end();
    main_flex.fixed(&status_flex, 25);

    // Close button row
    let mut btn_row = Flex::default();
    btn_row.set_type(fltk::group::FlexType::Row);
    Frame::default(); // Left spacer
    let mut close_btn = Button::default().with_label("Close");
    btn_row.fixed(&close_btn, 100);
    Frame::default(); // Right spacer
    btn_row.end();
    main_flex.fixed(&btn_row, 35);

    main_flex.end();
    window.end();
    window.show();

    // Load initial page
    {
        let mut buf = buffer.clone();
        let mut pos_label = position_label.clone();
        state.borrow().load_current_page(&mut buf, &mut pos_label);
    }

    // Close button
    let mut win_close = window.clone();
    close_btn.set_callback(move |_| {
        win_close.hide();
    });

    // Previous page button
    let state_prev = Rc::clone(&state);
    let mut buf_prev = buffer.clone();
    let mut pos_label_prev = position_label.clone();
    prev_btn.set_callback(move |_| {
        state_prev
            .borrow_mut()
            .prev_page(&mut buf_prev, &mut pos_label_prev);
    });

    // Next page button
    let state_next = Rc::clone(&state);
    let mut buf_next = buffer.clone();
    let mut pos_label_next = position_label.clone();
    next_btn.set_callback(move |_| {
        state_next
            .borrow_mut()
            .next_page(&mut buf_next, &mut pos_label_next);
    });

    // Go to line button (supports single line or range like "100-200")
    let state_goto = Rc::clone(&state);
    let mut buf_goto = buffer.clone();
    let mut pos_label_goto = position_label.clone();
    let goto_input_val = goto_input.clone();
    goto_btn.set_callback(move |_| {
        let line_str = goto_input_val.value();
        // Check for range format (100-200 or 100:200)
        if let Some((start, end)) = line_str
            .split_once('-')
            .or_else(|| line_str.split_once(':'))
        {
            if let (Ok(start_line), Ok(end_line)) =
                (start.trim().parse::<usize>(), end.trim().parse::<usize>())
            {
                state_goto.borrow_mut().load_line_range(
                    start_line,
                    end_line,
                    &mut buf_goto,
                    &mut pos_label_goto,
                );
            }
        } else if let Ok(target_line) = line_str.parse::<usize>() {
            state_goto
                .borrow_mut()
                .go_to_line(target_line, &mut buf_goto, &mut pos_label_goto);
        }
    });

    // Search button (from start)
    let state_search = Rc::clone(&state);
    let mut buf_search = buffer.clone();
    let mut pos_label_search = position_label.clone();
    let mut display_search = display.clone();
    let search_input_val = search_input.clone();
    search_btn.set_callback(move |_| {
        let query = search_input_val.value();
        let found = state_search.borrow_mut().search(
            &query,
            true,
            &mut buf_search,
            &mut pos_label_search,
            &mut display_search,
        );
        if !found {
            dialog::message_default("Text not found.");
        }
    });

    // Search next button
    let state_search_next = Rc::clone(&state);
    let mut buf_search_next = buffer.clone();
    let mut pos_label_search_next = position_label.clone();
    let mut display_search_next = display.clone();
    let search_input_next = search_input.clone();
    search_next_btn.set_callback(move |_| {
        let query = search_input_next.value();
        let found = state_search_next.borrow_mut().search(
            &query,
            false,
            &mut buf_search_next,
            &mut pos_label_search_next,
            &mut display_search_next,
        );
        if !found {
            dialog::message_default("No more matches found.");
            state_search_next.borrow_mut().search_pos = 0; // Reset for next search
        }
    });

    // Handle keyboard shortcuts
    let state_key = Rc::clone(&state);
    let mut buf_key = buffer.clone();
    let mut pos_label_key = position_label.clone();
    window.handle(move |_, ev| match ev {
        Event::KeyDown => {
            let key = app::event_key();
            match key {
                Key::PageUp => {
                    state_key
                        .borrow_mut()
                        .prev_page(&mut buf_key, &mut pos_label_key);
                    true
                }
                Key::PageDown => {
                    state_key
                        .borrow_mut()
                        .next_page(&mut buf_key, &mut pos_label_key);
                    true
                }
                Key::Home if app::event_state().contains(fltk::enums::Shortcut::Ctrl) => {
                    state_key
                        .borrow_mut()
                        .go_to_page(0, &mut buf_key, &mut pos_label_key);
                    true
                }
                Key::End if app::event_state().contains(fltk::enums::Shortcut::Ctrl) => {
                    let total = state_key.borrow().total_pages;
                    state_key
                        .borrow_mut()
                        .go_to_page(total - 1, &mut buf_key, &mut pos_label_key);
                    true
                }
                Key::Escape => {
                    // Don't close on Escape
                    false
                }
                _ => false,
            }
        }
        _ => false,
    });

    // Run the dialog event loop
    while window.shown() {
        app::wait();
        if app::should_program_quit() {
            window.hide();
        }
    }
}
