//! Read-only viewer for files too large to edit.
//!
//! Uses memory-mapped file access to view files larger than 1.8GB
//! without loading them entirely into memory. Supports navigation
//! by page, line number, and search.

use fltk::{
    app,
    button::Button,
    dialog,
    enums::{Align, Event, Font, FrameType, Key},
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

use super::{DialogTheme, SCROLLBAR_SIZE, darken, lighten};
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
    /// First line number of the currently displayed content (1-based)
    current_start_line: usize,
    /// Width of the line-number prefix (digits + separator) for current page
    current_prefix_width: usize,
}

impl ViewerState {
    fn new(mmap: Mmap, file_size: usize) -> Self {
        let total_pages = file_size.div_ceil(PAGE_SIZE);
        Self {
            mmap,
            file_size,
            total_pages,
            current_page: 0,
            search_pos: 0,
            current_start_line: 1,
            current_prefix_width: 0,
        }
    }

    fn load_current_page(&mut self, buffer: &mut TextBuffer, pos_label: &mut Frame) {
        let start = self.current_page * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(self.file_size);

        if start >= self.file_size {
            return;
        }

        // Get the slice from memory map
        let slice = &self.mmap[start..end];

        // Convert to string (lossy for binary files)
        let text = String::from_utf8_lossy(slice);

        // Count line numbers at start and end of current view
        let lines_before: usize = self.mmap[..start].iter().filter(|&&b| b == b'\n').count();
        let lines_in_page: usize = slice.iter().filter(|&&b| b == b'\n').count();
        let start_line = lines_before + 1;
        let end_line = start_line + lines_in_page;

        // Prepend real line numbers to each line
        let prefixed = Self::prepend_line_numbers(&text, start_line, end_line);
        self.current_start_line = start_line;
        self.current_prefix_width = Self::line_number_prefix_width(end_line);
        buffer.set_text(&prefixed);

        pos_label.set_label(&format!(
            "Page {}/{} | Lines {}-{}",
            self.current_page + 1,
            self.total_pages,
            start_line,
            end_line,
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

    fn go_to_line(&mut self, target_line: usize, buffer: &mut TextBuffer, pos_label: &mut Frame) {
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

        // Prepend real line numbers
        let prefixed = Self::prepend_line_numbers(&text, start_line, end_line);
        self.current_start_line = start_line;
        self.current_prefix_width = Self::line_number_prefix_width(end_line);
        buffer.set_text(&prefixed);

        // Update current page to match start
        self.current_page = start_offset / PAGE_SIZE;

        pos_label.set_label(&format!("Lines {}-{}", start_line, end_line,));
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

            // Position cursor at the match in the prefixed buffer.
            // We need to account for the line-number prefixes added to each line.
            let page_start = self.current_page * PAGE_SIZE;
            let local_byte_pos = found_pos - page_start;

            // Count how many newlines precede local_byte_pos in the raw page text
            let page_end = (page_start + PAGE_SIZE).min(self.file_size);
            let page_slice = &self.mmap[page_start..page_end];
            let newlines_before = page_slice[..local_byte_pos.min(page_slice.len())]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();

            // Each line gets a prefix of current_prefix_width bytes.
            // First line has 1 prefix, each newline adds another prefix for the next line.
            let prefix_bytes_added = (newlines_before + 1) * self.current_prefix_width;
            let adjusted_pos = (local_byte_pos + prefix_bytes_added) as i32;

            display.set_insert_position(adjusted_pos);
            display.show_insert_position();

            return true;
        }
        false
    }

    fn search_backward(
        &mut self,
        query: &str,
        buffer: &mut TextBuffer,
        pos_label: &mut Frame,
        display: &mut TextDisplay,
    ) -> bool {
        if query.is_empty() || self.search_pos == 0 {
            return false;
        }

        let query_bytes = query.as_bytes();
        // Search backward from just before the current match
        let search_end = self.search_pos.saturating_sub(query_bytes.len());
        if search_end == 0 {
            return false;
        }

        let slice = &self.mmap[..search_end];
        // Find the LAST occurrence by scanning backward
        if let Some(rel_pos) = slice
            .windows(query_bytes.len())
            .rposition(|w| w == query_bytes)
        {
            self.search_pos = rel_pos;

            let target_page = rel_pos / PAGE_SIZE;
            self.go_to_page(target_page, buffer, pos_label);

            let page_start = self.current_page * PAGE_SIZE;
            let local_byte_pos = rel_pos - page_start;

            let page_end = (page_start + PAGE_SIZE).min(self.file_size);
            let page_slice = &self.mmap[page_start..page_end];
            let newlines_before = page_slice[..local_byte_pos.min(page_slice.len())]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();

            let prefix_bytes_added = (newlines_before + 1) * self.current_prefix_width;
            let adjusted_pos = (local_byte_pos + prefix_bytes_added) as i32;

            display.set_insert_position(adjusted_pos);
            display.show_insert_position();

            return true;
        }
        false
    }

    /// Calculate the byte width of a line-number prefix for a given max line number.
    /// Format: "{number}│ " where number is right-aligned to the width of max_line.
    fn line_number_prefix_width(max_line: usize) -> usize {
        let digits = if max_line == 0 {
            1
        } else {
            ((max_line as f64).log10().floor() as usize) + 1
        };
        // digits + "│ " (│ is 3 bytes in UTF-8, space is 1)
        digits + 4
    }

    /// Prepend line numbers to each line of text.
    /// Returns a new string with "NNNNN│ " prefixed to each line.
    fn prepend_line_numbers(text: &str, start_line: usize, end_line: usize) -> String {
        let digits = if end_line == 0 {
            1
        } else {
            ((end_line as f64).log10().floor() as usize) + 1
        };

        let lines: Vec<&str> = text.split('\n').collect();
        let mut result = String::with_capacity(text.len() + lines.len() * (digits + 4));

        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            let line_num = start_line + i;
            // Right-align the line number, then box-drawing separator
            result.push_str(&format!("{:>width$}│ ", line_num, width = digits));
            result.push_str(line);
        }

        result
    }
}

/// Data returned when the user clicks "Open" in the read-only viewer.
pub struct ViewerOpenRequest {
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: u64,
    pub end_byte: u64,
}

/// Open a read-only viewer for a large file.
///
/// This creates a modal window with:
/// - Memory-mapped file access (no full load into RAM)
/// - Page-based navigation
/// - Go to line number
/// - Search functionality
/// - Display of current position/total size
///
/// Returns a `ViewerOpenRequest` if the user clicked "Open" to edit the
/// currently displayed lines, or `None` if the viewer was simply closed.
pub fn show_readonly_viewer(path: &Path, theme_bg: (u8, u8, u8)) -> Option<ViewerOpenRequest> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            dialog::alert_default(&format!("Failed to open file: {}", e));
            return None;
        }
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(e) => {
            dialog::alert_default(&format!("Failed to read file metadata: {}", e));
            return None;
        }
    };

    let file_size = metadata.len() as usize;
    if file_size == 0 {
        dialog::alert_default("File is empty.");
        return None;
    }

    // Memory-map the file
    // SAFETY: We open the file read-only and don't modify it while mapped.
    // The file remains open for the duration of the viewer window.
    let mmap = match unsafe { Mmap::map(&file) } {
        Ok(m) => m,
        Err(e) => {
            dialog::alert_default(&format!("Failed to memory-map file: {}", e));
            return None;
        }
    };

    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");

    // Create shared state
    let state = Rc::new(RefCell::new(ViewerState::new(mmap, file_size)));

    // Create viewer window
    let theme = DialogTheme::from_theme_bg(theme_bg);

    let title = format!("{} (Read-Only Viewer)", filename);
    let mut window = Window::new(100, 100, 900, 700, None);
    window.set_label(&title);
    window.make_modal(true);
    window.set_color(theme.bg);

    let mut main_flex = Flex::new(5, 5, 890, 690, None);
    main_flex.set_type(fltk::group::FlexType::Column);
    main_flex.set_spacing(5);

    // Top toolbar (navigation + search — page prev/next moved to bottom)
    let mut toolbar = Flex::default();
    toolbar.set_type(fltk::group::FlexType::Row);
    toolbar.set_spacing(5);

    let mut page_input = Input::default();
    page_input.set_tooltip("Page number (e.g., 5)");
    page_input.set_color(theme.input_bg);
    page_input.set_text_color(theme.text);
    page_input.set_frame(FrameType::FlatBox);
    toolbar.fixed(&page_input, 60);

    let mut page_btn = Button::default().with_label("Go to Page");
    page_btn.set_frame(FrameType::RFlatBox);
    page_btn.set_color(theme.button_bg);
    page_btn.set_label_color(theme.text);
    toolbar.fixed(&page_btn, 90);

    let mut spacer_page = Frame::default();
    spacer_page.set_frame(FrameType::FlatBox);
    spacer_page.set_color(theme.bg);

    let mut goto_input = Input::default();
    goto_input.set_tooltip("Line number or range (e.g., 100 or 100-200)");
    goto_input.set_color(theme.input_bg);
    goto_input.set_text_color(theme.text);
    goto_input.set_frame(FrameType::FlatBox);
    toolbar.fixed(&goto_input, 100);

    let mut goto_btn = Button::default().with_label("Go to Line");
    goto_btn.set_frame(FrameType::RFlatBox);
    goto_btn.set_color(theme.button_bg);
    goto_btn.set_label_color(theme.text);
    toolbar.fixed(&goto_btn, 80);

    let mut spacer2 = Frame::default();
    spacer2.set_frame(FrameType::FlatBox);
    spacer2.set_color(theme.bg);

    let mut search_input = Input::default();
    search_input.set_tooltip("Search text");
    search_input.set_color(theme.input_bg);
    search_input.set_text_color(theme.text);
    search_input.set_frame(FrameType::FlatBox);
    toolbar.fixed(&search_input, 150);

    let mut search_btn = Button::default().with_label("Find");
    search_btn.set_tooltip("Find from start");
    search_btn.set_frame(FrameType::RFlatBox);
    search_btn.set_color(theme.button_bg);
    search_btn.set_label_color(theme.text);
    toolbar.fixed(&search_btn, 60);

    let mut search_prev_btn = Button::default().with_label("@<");
    search_prev_btn.set_tooltip("Find previous (Shift+F3)");
    search_prev_btn.set_frame(FrameType::RFlatBox);
    search_prev_btn.set_color(theme.button_bg);
    search_prev_btn.set_label_color(theme.text);
    toolbar.fixed(&search_prev_btn, 30);

    let mut search_next_btn = Button::default().with_label("@>");
    search_next_btn.set_tooltip("Find next (F3)");
    search_next_btn.set_frame(FrameType::RFlatBox);
    search_next_btn.set_color(theme.button_bg);
    search_next_btn.set_label_color(theme.text);
    toolbar.fixed(&search_next_btn, 30);

    toolbar.end();
    main_flex.fixed(&toolbar, 30);

    // Text display area with line numbers
    let mut display = TextDisplay::default();
    let buffer = TextBuffer::default();
    display.set_buffer(buffer.clone());
    display.set_text_font(Font::Courier);
    display.set_text_size(14);
    display.wrap_mode(WrapMode::None, 0);
    // Line numbers are prepended as text, so disable the built-in gutter
    display.set_linenumber_width(0);
    display.set_frame(FrameType::FlatBox);
    display.set_color(theme.input_bg);
    display.set_text_color(theme.text);
    display.set_scrollbar_size(SCROLLBAR_SIZE);
    // SAFETY: TextDisplay inherits Fl_Group. Fl_Group_children/Fl_Group_child
    // are stable FLTK C API. We null-check child pointers and clamp index to
    // min(2) before reconstructing Scrollbar widgets.
    unsafe extern "C" {
        fn Fl_Group_children(grp: *mut std::ffi::c_void) -> std::ffi::c_int;
        fn Fl_Group_child(
            grp: *mut std::ffi::c_void,
            index: std::ffi::c_int,
        ) -> *mut std::ffi::c_void;
    }
    unsafe {
        use fltk::valuator::Scrollbar;
        let group_ptr = display.as_widget_ptr() as *mut std::ffi::c_void;
        let nchildren = Fl_Group_children(group_ptr);
        for i in 0..nchildren.min(2) {
            let ptr = Fl_Group_child(group_ptr, i);
            if !ptr.is_null() {
                let mut sb = Scrollbar::from_widget_ptr(ptr as fltk::app::WidgetPtr);
                if i == 0 {
                    // Hide horizontal scrollbar (not needed for line-oriented logs)
                    sb.hide();
                    sb.resize(0, 0, 0, 0);
                } else {
                    sb.set_frame(FrameType::FlatBox);
                    sb.set_color(theme.scroll_track);
                    sb.set_slider_frame(FrameType::FlatBox);
                    sb.set_selection_color(theme.scroll_thumb);
                }
            }
        }
    }

    // Status bar
    let mut status_flex = Flex::default();
    status_flex.set_type(fltk::group::FlexType::Row);

    let mut position_label = Frame::default();
    position_label.set_align(Align::Left | Align::Inside);
    position_label.set_label_color(theme.text_dim);
    position_label.set_frame(FrameType::FlatBox);
    position_label.set_color(theme.bg);
    status_flex.fixed(&position_label, 500);

    let mut size_label = Frame::default();
    size_label.set_align(Align::Right | Align::Inside);
    size_label.set_label(&format!("Total: {}", format_size(file_size as u64)));
    size_label.set_label_color(theme.text_dim);
    size_label.set_frame(FrameType::FlatBox);
    size_label.set_color(theme.bg);

    status_flex.end();
    main_flex.fixed(&status_flex, 25);

    // Bottom row: [<< Prev] [Next >>] ... [Open (lines X-Y)] ... [Close]
    let mut btn_row = Flex::default();
    btn_row.set_type(fltk::group::FlexType::Row);
    btn_row.set_spacing(5);

    let mut prev_btn = Button::default().with_label("@<  Prev Page");
    prev_btn.set_tooltip("Previous page (Page Up)");
    prev_btn.set_frame(FrameType::RFlatBox);
    prev_btn.set_color(theme.button_bg);
    prev_btn.set_label_color(theme.text);
    btn_row.fixed(&prev_btn, 110);

    let mut next_btn = Button::default().with_label("Next Page  @>");
    next_btn.set_tooltip("Next page (Page Down)");
    next_btn.set_frame(FrameType::RFlatBox);
    next_btn.set_color(theme.button_bg);
    next_btn.set_label_color(theme.text);
    btn_row.fixed(&next_btn, 110);

    let mut spacer_left = Frame::default();
    spacer_left.set_frame(FrameType::FlatBox);
    spacer_left.set_color(theme.bg);

    let mut open_btn = Button::default().with_label("Open (editable)");
    open_btn.set_tooltip("Open current page as editable chunk");
    open_btn.set_frame(FrameType::RFlatBox);
    open_btn.set_color(theme.button_bg);
    open_btn.set_label_color(theme.text);
    btn_row.fixed(&open_btn, 140);

    let mut spacer_right = Frame::default();
    spacer_right.set_frame(FrameType::FlatBox);
    spacer_right.set_color(theme.bg);

    let mut close_btn = Button::default().with_label("Close");
    let (r, g, b) = theme_bg;
    let brightness = (r as u32 + g as u32 + b as u32) / 3;
    let is_dark = brightness < 128;
    let (bg_r, bg_g, bg_b) = if is_dark {
        darken(r, g, b, 0.65)
    } else {
        darken(r, g, b, 0.85)
    };
    let close_bg = if is_dark {
        let (cr, cg, cb) = lighten(bg_r, bg_g, bg_b, 0.10);
        fltk::enums::Color::from_rgb(cr, cg, cb)
    } else {
        let (cr, cg, cb) = darken(bg_r, bg_g, bg_b, 0.90);
        fltk::enums::Color::from_rgb(cr, cg, cb)
    };
    close_btn.set_frame(FrameType::RFlatBox);
    close_btn.set_color(close_bg);
    close_btn.set_label_color(theme.text_dim);
    btn_row.fixed(&close_btn, 100);

    btn_row.end();
    main_flex.fixed(&btn_row, 35);

    // Track the open request (set by the Open button)
    let open_request: Rc<RefCell<Option<ViewerOpenRequest>>> = Rc::new(RefCell::new(None));

    main_flex.end();
    window.end();
    window.show();

    // Load initial page
    {
        let mut buf = buffer.clone();
        let mut pos_label = position_label.clone();
        state
            .borrow_mut()
            .load_current_page(&mut buf, &mut pos_label);
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

    // Go to page button
    let state_page = Rc::clone(&state);
    let mut buf_page = buffer.clone();
    let mut pos_label_page = position_label.clone();
    let page_input_val = page_input.clone();
    page_btn.set_callback(move |_| {
        if let Ok(page_num) = page_input_val.value().trim().parse::<usize>()
            && page_num >= 1
        {
            state_page
                .borrow_mut()
                .go_to_page(page_num - 1, &mut buf_page, &mut pos_label_page);
        }
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
            state_search_next.borrow_mut().search_pos = 0;
        }
    });

    // Search previous button
    let state_search_prev = Rc::clone(&state);
    let mut buf_search_prev = buffer.clone();
    let mut pos_label_search_prev = position_label.clone();
    let mut display_search_prev = display.clone();
    let search_input_prev = search_input.clone();
    search_prev_btn.set_callback(move |_| {
        let query = search_input_prev.value();
        let found = state_search_prev.borrow_mut().search_backward(
            &query,
            &mut buf_search_prev,
            &mut pos_label_search_prev,
            &mut display_search_prev,
        );
        if !found {
            dialog::message_default("No previous matches found.");
        }
    });

    // Open button — extract current page content from mmap and return it
    let state_open = Rc::clone(&state);
    let open_req = Rc::clone(&open_request);
    let mut win_open = window.clone();
    open_btn.set_callback(move |_| {
        let s = state_open.borrow();
        let start_line = s.current_start_line;
        let page_start = s.current_page * PAGE_SIZE;
        let page_end = (page_start + PAGE_SIZE).min(s.file_size);
        let slice = &s.mmap[page_start..page_end];
        let lines_in_page = slice.iter().filter(|&&b| b == b'\n').count();
        let end_line = start_line + lines_in_page;

        // Extract raw content directly from the mmap (no re-read needed)
        let content = String::from_utf8_lossy(slice).into_owned();

        *open_req.borrow_mut() = Some(ViewerOpenRequest {
            content,
            start_line,
            end_line,
            start_byte: page_start as u64,
            end_byte: page_end as u64,
        });
        win_open.hide();
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

    open_request.borrow_mut().take()
}
