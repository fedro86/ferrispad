//! Terminal panel UI for embedded terminal emulation.
//!
//! Provides a PTY-backed terminal widget with VTE parsing.
//! Lazy-loaded: zero PTY/grid/parser until first show().
//! Plugin-driven via the Widget API.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use fltk::{
    app::Sender,
    button::Button,
    enums::{Align, Color, Cursor, Event, Font, FrameType, Key},
    frame::Frame,
    group::Flex,
    prelude::*,
};

use super::dialogs::DialogTheme;
use crate::app::Message;
use crate::app::plugins::widgets::TerminalViewRequest;
use crate::app::services::terminal::grid::{Cell, TerminalGrid};
use crate::app::services::terminal::pty::PtySession;

/// Height of the terminal panel header
const HEADER_HEIGHT: i32 = 32;

/// Default width of the terminal panel (right-side position)
const DEFAULT_WIDTH: i32 = 550;

/// Font size for terminal content
const TERM_FONT_SIZE: i32 = 14;

/// Font for terminal content (monospace)
const TERM_FONT: Font = Font::Courier;

/// Internal padding around the terminal content (px)
const TERM_PAD: i32 = 4;

/// Colors derived from the editor theme for terminal rendering.
#[derive(Clone, Copy)]
struct TerminalTheme {
    /// Terminal content background (slightly darker than editor)
    bg: Color,
}

impl TerminalTheme {
    /// Readable text on dark backgrounds
    const TEXT_ON_DARK: Color = Color::from_rgb(220, 220, 220);
    /// Readable text on light backgrounds
    const TEXT_ON_LIGHT: Color = Color::from_rgb(30, 30, 30);

    fn from_theme(is_dark: bool, theme_bg: (u8, u8, u8)) -> Self {
        let (r, g, b) = theme_bg;
        let bg = if is_dark {
            Color::from_rgb(r.saturating_sub(10), g.saturating_sub(10), b.saturating_sub(10))
        } else {
            Color::from_rgb(r.saturating_sub(15), g.saturating_sub(15), b.saturating_sub(15))
        };
        Self { bg }
    }

    /// Compute the effective fg color for a cell, applying contrast adjustment.
    /// Used for both text rendering and cursor block color.
    fn effective_fg(cell_fg: Color, cell_bg: Color, terminal_bg: Color) -> Color {
        let effective_bg = if cell_bg != Color::TransparentBg {
            cell_bg
        } else {
            terminal_bg
        };
        let (br, bg_g, bb) = effective_bg.to_rgb();
        let bg_bright = (br as u32 + bg_g as u32 + bb as u32) / 3;
        let (fr, fg_g, fb) = cell_fg.to_rgb();
        let fg_bright = (fr as u32 + fg_g as u32 + fb as u32) / 3;

        if bg_bright < 100 && fg_bright < 80 {
            Self::TEXT_ON_DARK
        } else if bg_bright > 160 && fg_bright > 180 {
            Self::TEXT_ON_LIGHT
        } else {
            cell_fg
        }
    }
}

/// Snapshot of grid state for the draw callback (avoids lifetime issues)
struct GridSnapshot {
    cells: Vec<Vec<Cell>>,
    rows: usize,
    cols: usize,
    cursor_row: usize,
    cursor_col: usize,
    cursor_visible: bool,
    /// Scrollback lines (oldest first) for scroll-up viewing
    scrollback: Vec<Vec<Cell>>,
}

/// Terminal panel widget for embedded terminal emulation
pub struct TerminalPanel {
    /// The outer container (Flex column)
    pub container: Flex,
    /// Header frame showing title
    header: Frame,
    /// Close button in header
    _close_btn: Button,
    /// Canvas frame for custom-drawn terminal surface
    canvas: Frame,
    /// Message sender
    sender: Sender<Message>,
    /// Current session ID
    session_id: Option<u32>,
    /// Whether the panel is visible
    visible: bool,
    /// Optional divider frame for resizing
    pub divider: Option<Frame>,
    /// Terminal state (lazy-loaded: None until first show)
    state: Option<Box<TerminalState>>,
    /// Shared output buffer — reader thread pushes, FLTK event loop drains
    output_buf: Arc<Mutex<Vec<u8>>>,
    /// Whether the terminal is in dark mode
    is_dark: bool,
    /// Editor theme background (r, g, b) — used to derive terminal bg
    theme_bg: (u8, u8, u8),
    /// Shared grid snapshot for the draw callback
    snapshot: Arc<Mutex<Option<GridSnapshot>>>,
    /// Scroll offset from bottom (0 = at bottom, shared with draw callback)
    scroll_offset: Arc<Mutex<usize>>,
}

/// Internal terminal state, created lazily on first show
struct TerminalState {
    grid: TerminalGrid,
    pty: PtySession,
    parser: vte::Parser,
    _reader_thread: JoinHandle<()>,
}

impl TerminalPanel {
    /// Create a new terminal panel (container + header + canvas only, no PTY).
    pub fn new(sender: Sender<Message>) -> Self {
        let mut container = Flex::default().column();
        container.set_margin(0);
        container.set_pad(0);

        // Header row
        let mut header_row = Flex::default().row();
        header_row.set_margin(0);
        header_row.set_pad(0);

        let mut header = Frame::default();
        header.set_frame(FrameType::FlatBox);
        header.set_label("Terminal");
        header.set_label_size(13);
        header.set_align(Align::Left | Align::Inside);

        let mut close_btn = Button::default().with_label("\u{2715}");
        close_btn.set_frame(FrameType::FlatBox);
        close_btn.set_label_size(14);
        header_row.fixed(&close_btn, 30);

        header_row.end();
        container.fixed(&header_row, HEADER_HEIGHT);

        // Canvas for terminal rendering
        let canvas = Frame::default();

        container.end();
        container.hide();

        // Close button callback
        {
            let s = sender;
            close_btn.set_callback(move |_| {
                s.send(Message::TerminalViewHide(0));
            });
        }

        Self {
            container,
            header,
            _close_btn: close_btn,
            canvas,
            sender,
            session_id: None,
            visible: false,
            divider: None,
            state: None,
            output_buf: Arc::new(Mutex::new(Vec::new())),
            is_dark: true,
            theme_bg: (40, 44, 52),
            snapshot: Arc::new(Mutex::new(None)),
            scroll_offset: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a draggable divider frame (call before `new()` in layout).
    pub fn new_divider(sender: Sender<Message>) -> Frame {
        let mut div = Frame::default();
        div.set_frame(FrameType::FlatBox);
        div.hide();

        div.handle(move |f, ev| match ev {
            Event::Enter => {
                f.window().unwrap().set_cursor(Cursor::WE);
                true
            }
            Event::Leave => {
                f.window().unwrap().set_cursor(Cursor::Default);
                true
            }
            Event::Push => true,
            Event::Drag => {
                let mouse_x = fltk::app::event_x();
                sender.send(Message::TerminalViewResize(mouse_x));
                true
            }
            _ => false,
        });

        div
    }


    /// Show a terminal from a plugin request. Lazy-initializes PTY on first call.
    pub fn show_request(&mut self, session_id: u32, request: &TerminalViewRequest) {
        self.session_id = Some(session_id);

        // Update header
        self.header.set_label(&request.title);

        // If we already have a live terminal state, just show the container
        if self.state.is_some() {
            self.container.show();
            self.visible = true;
            self.update_snapshot();
            self.canvas.redraw();
            return;
        }

        // Lazy init: calculate grid dimensions from canvas size (minus padding)
        let canvas_w = if self.canvas.w() > 0 {
            self.canvas.w() - TERM_PAD * 2
        } else {
            DEFAULT_WIDTH - 10
        };
        let canvas_h = if self.canvas.h() > 0 {
            self.canvas.h() - TERM_PAD * 2
        } else {
            400
        };

        let (char_w, char_h) = Self::char_metrics();
        let cols = (canvas_w / char_w).max(20) as usize;
        let rows = (canvas_h / char_h).max(5) as usize;

        // Spawn PTY
        let pty_result = PtySession::spawn(
            request.command.as_deref(),
            &request.args,
            request.working_dir.as_deref(),
            cols as u16,
            rows as u16,
        );

        match pty_result {
            Ok((pty, reader)) => {
                let grid = TerminalGrid::new(cols, rows);
                let parser = vte::Parser::new();

                // Start background reader thread
                let output_buf = Arc::clone(&self.output_buf);
                let sender = self.sender;
                let reader_thread = std::thread::spawn(move || {
                    Self::reader_loop(reader, output_buf, sender);
                });

                self.state = Some(Box::new(TerminalState {
                    grid,
                    pty,
                    parser,
                    _reader_thread: reader_thread,
                }));

                // Set up canvas draw callback and input handling
                self.setup_draw_callback();
                self.setup_input_handler();

                self.container.show();
                self.visible = true;
            }
            Err(e) => {
                eprintln!("[terminal] Failed to spawn PTY: {}", e);
                self.header.set_label(&format!("{} (error)", request.title));
                self.container.show();
                self.visible = true;
            }
        }
    }

    /// Background reader thread: reads PTY output, buffers it, wakes FLTK
    fn reader_loop(
        mut reader: Box<dyn std::io::Read + Send>,
        output_buf: Arc<Mutex<Vec<u8>>>,
        sender: Sender<Message>,
    ) {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Ok(mut ob) = output_buf.lock() {
                        ob.extend_from_slice(&buf[..n]);
                    }
                    sender.send(Message::TerminalOutput(Vec::new())); // Signal only
                    fltk::app::awake();
                }
            }
        }
        sender.send(Message::TerminalExited);
        fltk::app::awake();
    }

    /// Process buffered output from the reader thread
    pub fn process_output(&mut self) {
        let data = {
            let mut ob = match self.output_buf.lock() {
                Ok(ob) => ob,
                Err(_) => return,
            };
            if ob.is_empty() {
                return;
            }
            std::mem::take(&mut *ob)
        };

        if let Some(ref mut ts) = self.state {
            let mut handler =
                crate::app::services::terminal::vte_handler::VteHandler::new(&mut ts.grid);
            ts.parser.advance(&mut handler, &data);
            self.update_snapshot();
            self.canvas.redraw();
        }
    }

    /// Update the shared snapshot with current grid state
    fn update_snapshot(&self) {
        if let Some(ref ts) = self.state
            && let Ok(mut snap) = self.snapshot.lock()
        {
            let mut scrollback = Vec::with_capacity(ts.grid.scrollback_len());
            for i in 0..ts.grid.scrollback_len() {
                if let Some(line) = ts.grid.scrollback_line(i) {
                    scrollback.push(line.to_vec());
                }
            }
            *snap = Some(GridSnapshot {
                cells: ts.grid.cells.clone(),
                rows: ts.grid.rows,
                cols: ts.grid.cols,
                cursor_row: ts.grid.cursor_row,
                cursor_col: ts.grid.cursor_col,
                cursor_visible: ts.grid.cursor_visible,
                scrollback,
            });
        }
        // New output arrives → snap to bottom
        if let Ok(mut off) = self.scroll_offset.lock() {
            *off = 0;
        }
    }

    /// Set up the canvas draw callback using a shared snapshot
    fn setup_draw_callback(&mut self) {
        let snapshot = Arc::clone(&self.snapshot);
        let scroll_offset = Arc::clone(&self.scroll_offset);
        let term_theme = TerminalTheme::from_theme(self.is_dark, self.theme_bg);
        let bg_color = term_theme.bg;

        self.canvas.draw(move |f| {
            let x = f.x();
            let y = f.y();
            let w = f.w();
            let h = f.h();

            // Fill background
            fltk::draw::set_draw_color(bg_color);
            fltk::draw::draw_rectf(x, y, w, h);

            let snap = match snapshot.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            let grid = match snap.as_ref() {
                Some(g) => g,
                None => return,
            };

            let offset = scroll_offset.lock().map(|g| *g).unwrap_or(0);

            fltk::draw::set_font(TERM_FONT, TERM_FONT_SIZE);
            let char_w = fltk::draw::width("M") as i32;
            let char_h = fltk::draw::height();

            // Build a combined view: scrollback + visible grid
            // When offset=0, show the bottom (normal view = just grid.cells)
            // When offset>0, shift up into scrollback
            let sb_len = grid.scrollback.len();
            let total_lines = sb_len + grid.rows;

            // First visible line index in the combined buffer
            let first_visible = total_lines.saturating_sub(grid.rows + offset);

            let ox = x + TERM_PAD;
            let oy = y + TERM_PAD;

            for screen_row in 0..grid.rows {
                let py = oy + (screen_row as i32) * char_h;
                if py + char_h < y || py > y + h {
                    continue;
                }

                let line_idx = first_visible + screen_row;
                let line: Option<&[Cell]> = if line_idx < sb_len {
                    Some(&grid.scrollback[line_idx])
                } else {
                    let grid_row = line_idx - sb_len;
                    if grid_row < grid.cells.len() {
                        Some(&grid.cells[grid_row])
                    } else {
                        None
                    }
                };

                let Some(line) = line else { continue };

                for col in 0..grid.cols {
                    let px = ox + (col as i32) * char_w;
                    if px > x + w {
                        break;
                    }
                    if col >= line.len() {
                        continue;
                    }
                    let cell = &line[col];

                    if cell.bg != Color::TransparentBg {
                        fltk::draw::set_draw_color(cell.bg);
                        fltk::draw::draw_rectf(px, py, char_w, char_h);
                    }

                    if cell.ch != ' ' {
                        let fg = TerminalTheme::effective_fg(cell.fg, cell.bg, bg_color);
                        fltk::draw::set_draw_color(fg);
                        if cell.bold {
                            fltk::draw::set_font(Font::CourierBold, TERM_FONT_SIZE);
                        }

                        let mut buf = [0u8; 4];
                        let s = cell.ch.encode_utf8(&mut buf);
                        fltk::draw::draw_text2(
                            s,
                            px,
                            py,
                            char_w,
                            char_h,
                            Align::Left | Align::Inside,
                        );

                        if cell.bold {
                            fltk::draw::set_font(TERM_FONT, TERM_FONT_SIZE);
                        }
                    }
                }
            }

            // Draw VT cursor when visible. TUI apps that hide the cursor
            // (CSI ?25l) render their own via reverse video (SGR 7).
            if offset == 0
                && grid.cursor_visible
                && grid.cursor_row < grid.rows
                && grid.cursor_col < grid.cols
                && grid.cursor_row < grid.cells.len()
                && grid.cursor_col < grid.cells[grid.cursor_row].len()
            {
                let cx = ox + (grid.cursor_col as i32) * char_w;
                let cy = oy + (grid.cursor_row as i32) * char_h;
                let cell = &grid.cells[grid.cursor_row][grid.cursor_col];
                let cursor_color = TerminalTheme::effective_fg(cell.fg, cell.bg, bg_color);
                fltk::draw::set_draw_color(cursor_color);
                fltk::draw::draw_rectf(cx, cy, char_w, char_h);

                if cell.ch != ' ' {
                    fltk::draw::set_draw_color(bg_color);
                    let mut buf = [0u8; 4];
                    let s = cell.ch.encode_utf8(&mut buf);
                    fltk::draw::draw_text2(s, cx, cy, char_w, char_h, Align::Left | Align::Inside);
                }
            }
        });
    }

    /// Set up input handler to forward key events to PTY
    fn setup_input_handler(&mut self) {
        if let Some(ref ts) = self.state {
            let writer = Arc::new(Mutex::new(PtyWriteHandle {
                pty: &ts.pty as *const PtySession,
            }));
            let scroll_offset = Arc::clone(&self.scroll_offset);
            let snapshot = Arc::clone(&self.snapshot);

            self.canvas.handle({
                let writer = writer.clone();
                move |f, ev| match ev {
                    Event::Push => {
                        let _ = f.take_focus();
                        true
                    }
                    Event::Focus | Event::Unfocus => true,
                    Event::MouseWheel => {
                        let dy = fltk::app::event_dy();
                        if let Ok(mut off) = scroll_offset.lock() {
                            let max_scroll = snapshot
                                .lock()
                                .ok()
                                .and_then(|s| s.as_ref().map(|g| g.scrollback.len()))
                                .unwrap_or(0);
                            match dy {
                                fltk::app::MouseWheel::Up => {
                                    *off = (*off + 3).min(max_scroll);
                                }
                                fltk::app::MouseWheel::Down => {
                                    *off = off.saturating_sub(3);
                                }
                                _ => {}
                            }
                        }
                        f.redraw();
                        true
                    }
                    Event::KeyDown => {
                        let key = fltk::app::event_key();
                        let text = fltk::app::event_text();

                        let bytes = encode_key(key, &text);
                        if !bytes.is_empty()
                            && let Ok(w) = writer.lock()
                        {
                            // SAFETY: pty pointer is valid as long as state exists
                            unsafe { (*w.pty).write(&bytes) };
                        }
                        // Snap to bottom on keypress
                        if let Ok(mut off) = scroll_offset.lock() {
                            *off = 0;
                        }
                        true
                    }
                    _ => false,
                }
            });
        }
    }

    /// Get character metrics for the terminal font
    fn char_metrics() -> (i32, i32) {
        fltk::draw::set_font(TERM_FONT, TERM_FONT_SIZE);
        let char_w = fltk::draw::width("M") as i32;
        let char_h = fltk::draw::height();
        (char_w.max(1), char_h.max(1))
    }

    /// Hide the terminal panel
    pub fn hide(&mut self) {
        self.container.hide();
        self.visible = false;
    }

    /// Close the terminal — kill PTY, drop state
    pub fn close(&mut self) {
        if let Some(ts) = self.state.take() {
            ts.pty.kill();
        }
        self.session_id = None;
        if let Ok(mut snap) = self.snapshot.lock() {
            *snap = None;
        }
        self.hide();
    }

    /// Apply theme colors
    pub fn apply_theme(&mut self, is_dark: bool, theme_bg: (u8, u8, u8)) {
        self.is_dark = is_dark;
        self.theme_bg = theme_bg;
        let theme = DialogTheme::from_theme_bg(theme_bg);

        self.header.set_color(theme.bg);
        self.header.set_label_color(theme.text);

        if let Some(ref mut div) = self.divider {
            div.set_color(super::theme::divider_color_from_bg(theme_bg));
        }

        // Re-set the draw callback with updated dark mode
        if self.state.is_some() {
            self.setup_draw_callback();
        }

        if self.visible {
            self.canvas.redraw();
        }
    }

    /// Get the container widget
    pub fn widget(&self) -> &Flex {
        &self.container
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the current session ID
    #[allow(dead_code)]
    pub fn session_id(&self) -> Option<u32> {
        self.session_id
    }

    /// Get the current width
    pub fn current_width(&self) -> i32 {
        if self.container.w() > 0 {
            self.container.w()
        } else {
            DEFAULT_WIDTH
        }
    }

    /// Handle resize — recalculate grid dimensions and notify PTY
    pub fn handle_resize(&mut self) {
        if let Some(ref mut ts) = self.state {
            let (char_w, char_h) = Self::char_metrics();
            if char_w <= 0 || char_h <= 0 || self.canvas.w() <= 0 || self.canvas.h() <= 0 {
                return;
            }
            let usable_w = self.canvas.w() - TERM_PAD * 2;
            let usable_h = self.canvas.h() - TERM_PAD * 2;
            let new_cols = (usable_w / char_w).max(10) as usize;
            let new_rows = (usable_h / char_h).max(3) as usize;

            if new_cols != ts.grid.cols || new_rows != ts.grid.rows {
                ts.grid.resize(new_cols, new_rows);
                ts.pty.resize(new_cols as u16, new_rows as u16);
                self.update_snapshot();
                self.canvas.redraw();
            }
        }
    }

    /// Send raw input bytes to the terminal PTY (as if typed by the user).
    pub fn send_input(&self, data: &[u8]) {
        if let Some(ref ts) = self.state {
            ts.pty.write(data);
        }
    }
}

/// Encode an FLTK key event into terminal bytes
fn encode_key(key: Key, text: &str) -> Vec<u8> {
    match key {
        Key::Enter => vec![b'\r'],
        Key::BackSpace => vec![0x7f],
        Key::Tab => {
            if fltk::app::event_state().contains(fltk::enums::Shortcut::Shift) {
                b"\x1b[Z".to_vec() // Shift+Tab (reverse tab / back tab)
            } else {
                vec![b'\t']
            }
        }
        Key::Escape => vec![0x1b],
        Key::Up => b"\x1b[A".to_vec(),
        Key::Down => b"\x1b[B".to_vec(),
        Key::Right => b"\x1b[C".to_vec(),
        Key::Left => b"\x1b[D".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::Insert => b"\x1b[2~".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::F1 => b"\x1bOP".to_vec(),
        Key::F2 => b"\x1bOQ".to_vec(),
        Key::F3 => b"\x1bOR".to_vec(),
        Key::F4 => b"\x1bOS".to_vec(),
        Key::F5 => b"\x1b[15~".to_vec(),
        Key::F6 => b"\x1b[17~".to_vec(),
        Key::F7 => b"\x1b[18~".to_vec(),
        Key::F8 => b"\x1b[19~".to_vec(),
        Key::F9 => b"\x1b[20~".to_vec(),
        Key::F10 => b"\x1b[21~".to_vec(),
        Key::F11 => b"\x1b[23~".to_vec(),
        Key::F12 => b"\x1b[24~".to_vec(),
        _ => {
            // Check for Ctrl+key combinations
            let state = fltk::app::event_state();
            if state.contains(fltk::enums::Shortcut::Ctrl) && !text.is_empty() {
                let ch = text.bytes().next().unwrap_or(0);
                if ch.is_ascii_lowercase() {
                    return vec![ch - b'a' + 1];
                }
                if ch.is_ascii_uppercase() {
                    return vec![ch - b'A' + 1];
                }
            }
            // Regular text input
            if !text.is_empty() {
                text.as_bytes().to_vec()
            } else {
                Vec::new()
            }
        }
    }
}

/// Helper to pass PTY write handle into callbacks.
/// SAFETY: The PtySession pointer is valid as long as TerminalState exists,
/// which is owned by TerminalPanel. The panel outlives all callbacks.
struct PtyWriteHandle {
    pty: *const PtySession,
}

// SAFETY: PtySession.write() is thread-safe (uses Arc<Mutex<Writer>>)
unsafe impl Send for PtyWriteHandle {}
unsafe impl Sync for PtyWriteHandle {}
