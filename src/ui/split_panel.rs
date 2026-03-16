//! Split panel UI for displaying side-by-side content views.
//!
//! Used for showing diffs, AI suggestions, and file comparisons.
//! Plugin-driven via the Widget API.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use fltk::{
    app,
    app::Sender,
    button::Button,
    dialog,
    enums::{Align, Color, Cursor, Event, Font, FrameType},
    frame::Frame,
    group::Flex,
    prelude::*,
    text::{StyleTableEntryExt, TextAttr, TextBuffer, TextDisplay, TextEditor},
};

use crate::app::plugins::widgets::{HighlightColor, LineHighlight, SplitDisplayMode, SplitViewAction, SplitViewRequest};
use crate::app::Message;
use super::dialogs::{DialogTheme, SCROLLBAR_SIZE};

/// Get the vertical scrollbar value of a TextDisplay/TextEditor via FFI.
/// Works with any widget that is a Fl_Group subclass with scrollbar children.
/// The vertical scrollbar is typically at index 1.
fn get_vscrollbar_value_raw(widget_ptr: fltk::app::WidgetPtr) -> f64 {
    // SAFETY: widget_ptr is a valid Fl_Group subclass (TextDisplay/TextEditor).
    // Fl_Group_children/Fl_Group_child are stable FLTK C API. We null-check
    // the child pointer before reconstructing. The widget outlives this call.
    unsafe extern "C" {
        fn Fl_Group_children(grp: *mut std::ffi::c_void) -> std::ffi::c_int;
        fn Fl_Group_child(
            grp: *mut std::ffi::c_void,
            index: std::ffi::c_int,
        ) -> *mut std::ffi::c_void;
    }
    unsafe {
        use fltk::valuator::Scrollbar;
        let group_ptr = widget_ptr as *mut std::ffi::c_void;
        let nchildren = Fl_Group_children(group_ptr);
        // child[0] = horizontal scrollbar (wide, short)
        // child[1] = vertical scrollbar (narrow, tall)
        if nchildren > 1 {
            let ptr = Fl_Group_child(group_ptr, 1);
            if !ptr.is_null() {
                let sb = Scrollbar::from_widget_ptr(ptr as fltk::app::WidgetPtr);
                return sb.value();
            }
        }
        0.0
    }
}

/// Get the vertical scrollbar value of a TextDisplay.
fn get_vscrollbar_value(display: &TextDisplay) -> f64 {
    get_vscrollbar_value_raw(display.as_widget_ptr())
}

/// Get the vertical scrollbar value of a TextEditor.
fn get_vscrollbar_value_editor(editor: &TextEditor) -> f64 {
    get_vscrollbar_value_raw(editor.as_widget_ptr())
}

/// Maximum style entries for the split view style table.
/// Uses ASCII chars 'A' (65) through '~' (126) = 62 entries.
/// This accommodates ~10 syntax colors × 6 diff backgrounds + 6 base entries.
const MAX_STYLE_ENTRIES: usize = 62;

/// Diff background type for style combination lookup.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum DiffBg {
    Normal,
    Added,
    Removed,
    Modified,
    AddedEmphasis,
    RemovedEmphasis,
}

impl DiffBg {
    /// Base style char index (0-based) for fallback when no syntax color is available.
    fn base_index(self) -> usize {
        match self {
            DiffBg::Normal => 0,
            DiffBg::Added => 1,
            DiffBg::Removed => 2,
            DiffBg::Modified => 3,
            DiffBg::AddedEmphasis => 4,
            DiffBg::RemovedEmphasis => 5,
        }
    }
}

/// Builds a combined style table that merges syntax foreground colors with diff background colors.
struct SyntaxDiffMap {
    /// Maps (fg_r, fg_g, fg_b, diff_bg_variant) → style char
    combos: HashMap<(u8, u8, u8, u8), char>,
    entries: Vec<StyleTableEntryExt>,
    font: Font,
    font_size: i32,
}

impl SyntaxDiffMap {
    /// Create a new map, seeded with the 6 base diff styles.
    fn new(
        is_dark: bool,
        theme_bg: (u8, u8, u8),
        theme_fg: (u8, u8, u8),
        font: Font,
        font_size: i32,
    ) -> Self {
        let (r, g, b) = theme_bg;
        let fg = Color::from_rgb(theme_fg.0, theme_fg.1, theme_fg.2);
        let bg = Color::from_rgb(r, g, b);

        let added_bg = if is_dark {
            Color::from_rgb(20, 60, 20)
        } else {
            Color::from_rgb(200, 255, 200)
        };
        let removed_bg = if is_dark {
            Color::from_rgb(60, 20, 20)
        } else {
            Color::from_rgb(255, 200, 200)
        };
        let modified_bg = if is_dark {
            Color::from_rgb(60, 50, 10)
        } else {
            Color::from_rgb(255, 255, 200)
        };
        let added_emphasis_bg = if is_dark {
            Color::from_rgb(40, 100, 40)
        } else {
            Color::from_rgb(150, 255, 150)
        };
        let removed_emphasis_bg = if is_dark {
            Color::from_rgb(100, 40, 40)
        } else {
            Color::from_rgb(255, 150, 150)
        };

        let base = |bgcolor: Color| StyleTableEntryExt {
            color: fg,
            font,
            size: font_size,
            attr: TextAttr::BgColor,
            bgcolor,
        };

        let entries = vec![
            base(bg),                  // 'A' - normal
            base(added_bg),            // 'B' - added
            base(removed_bg),          // 'C' - removed
            base(modified_bg),         // 'D' - modified
            base(added_emphasis_bg),   // 'E' - added emphasis
            base(removed_emphasis_bg), // 'F' - removed emphasis
        ];

        Self {
            combos: HashMap::new(),
            entries,
            font,
            font_size,
        }
    }

    /// Get the background Color for a DiffBg variant from the base entries.
    fn diff_bgcolor(&self, diff: DiffBg) -> Color {
        self.entries[diff.base_index()].bgcolor
    }

    /// Get or create a style char that combines `syntax_fg` with `diff_bg`.
    /// Returns the base diff-only char as fallback if the table is full.
    fn get_or_insert(&mut self, syntax_fg: Color, diff: DiffBg) -> char {
        let (fr, fg, fb) = {
            let (r, g, b) = syntax_fg.to_rgb();
            (r, g, b)
        };
        let key = (fr, fg, fb, diff.base_index() as u8);

        if let Some(&ch) = self.combos.get(&key) {
            return ch;
        }

        // Check if we have room for a new entry
        if self.entries.len() >= MAX_STYLE_ENTRIES {
            // Overflow: fall back to diff-only style
            return (b'A' + diff.base_index() as u8) as char;
        }

        let ch = (b'A' + self.entries.len() as u8) as char;
        self.entries.push(StyleTableEntryExt {
            color: syntax_fg,
            font: self.font,
            size: self.font_size,
            attr: TextAttr::BgColor,
            bgcolor: self.diff_bgcolor(diff),
        });
        self.combos.insert(key, ch);
        ch
    }

    /// Return the DiffBg variant index for each entry (parallel to entries vec).
    /// First 6 entries are always [0,1,2,3,4,5]; additional entries
    /// carry the DiffBg index from their combo key.
    fn diff_bg_indices(&self) -> Vec<u8> {
        let mut indices: Vec<u8> = (0..6.min(self.entries.len() as u8)).collect();
        // For entries beyond the 6 base ones, look up via combos
        // Combos map (fr,fg,fb, diff_idx) → char. Invert: char → diff_idx.
        let mut char_to_diff: HashMap<char, u8> = HashMap::new();
        for (&(_fr, _fg, _fb, diff_idx), &ch) in &self.combos {
            char_to_diff.insert(ch, diff_idx);
        }
        for i in 6..self.entries.len() {
            let ch = (b'A' + i as u8) as char;
            indices.push(*char_to_diff.get(&ch).unwrap_or(&0));
        }
        indices
    }

    /// Consume into the final style table.
    fn into_entries(self) -> Vec<StyleTableEntryExt> {
        self.entries
    }
}

/// Build the diff-background map for a pane: for each byte position in `text`,
/// determine which DiffBg applies based on the line highlights.
fn build_diff_map(text: &str, highlights: &[LineHighlight]) -> Vec<DiffBg> {
    let mut map = vec![DiffBg::Normal; text.len()];

    // Pre-compute line start offsets
    let mut line_starts: Vec<usize> = vec![0];
    for (i, ch) in text.bytes().enumerate() {
        if ch == b'\n' && i + 1 < text.len() {
            line_starts.push(i + 1);
        }
    }

    for hl in highlights {
        let base_diff = match hl.color {
            HighlightColor::Added => DiffBg::Added,
            HighlightColor::Removed => DiffBg::Removed,
            HighlightColor::Modified => DiffBg::Modified,
            HighlightColor::Rgb(_, _, _) => DiffBg::Added,
        };
        let emphasis_diff = match hl.color {
            HighlightColor::Added => DiffBg::AddedEmphasis,
            HighlightColor::Removed => DiffBg::RemovedEmphasis,
            _ => base_diff,
        };

        let target_line = hl.line as usize;
        if target_line == 0 || target_line > line_starts.len() {
            continue;
        }

        let line_start = line_starts[target_line - 1];
        let line_end = text[line_start..]
            .find('\n')
            .map(|pos| line_start + pos)
            .unwrap_or(text.len());

        for d in &mut map[line_start..line_end] {
            *d = base_diff;
        }

        for span in &hl.spans {
            let span_start = line_start + span.start as usize;
            let span_end = (line_start + span.end as usize).min(line_end).min(map.len());
            if span_start < span_end && span_start < map.len() {
                for d in &mut map[span_start..span_end] {
                    *d = emphasis_diff;
                }
            }
        }
    }

    map
}

/// Height of the split panel header
const HEADER_HEIGHT: i32 = 24;

/// Height of the pane labels
const LABEL_HEIGHT: i32 = 20;

/// Height of the action button bar
const ACTION_BAR_HEIGHT: i32 = 34;

/// Default height of the split panel content area
const DEFAULT_CONTENT_HEIGHT: i32 = 250;

/// Split panel widget for showing side-by-side content
pub struct SplitPanel {
    /// The outer container (Flex column)
    pub container: Flex,
    /// Draggable divider between editor and split panel (4px, managed externally).
    /// None until `create_divider()` is called from main_window.rs.
    pub divider: Option<Frame>,
    /// Header frame showing title
    header: Frame,
    /// Left pane text display
    left_display: TextDisplay,
    /// Left pane buffer
    left_buffer: TextBuffer,
    /// Left pane style buffer
    left_style_buffer: TextBuffer,
    /// Left pane label
    left_label: Frame,
    /// Right pane editor (TextEditor used as display; read-only controlled via handle closure)
    right_editor: TextEditor,
    /// Whether the right pane is currently read-only (shared with handle closure)
    right_read_only: Rc<Cell<bool>>,
    /// Right pane buffer
    right_buffer: TextBuffer,
    /// Right pane style buffer
    right_style_buffer: TextBuffer,
    /// Right pane label
    right_label: Frame,
    /// Labels row (HEAD / Working Copy)
    labels_row: Flex,
    /// Content row (holds left and right displays)
    content_row: Flex,
    /// Action button bar
    action_bar: Flex,
    /// Message sender
    sender: Sender<Message>,
    /// Current session ID
    session_id: Option<u32>,
    /// Whether panel is currently visible
    visible: bool,
    /// Dark mode flag
    is_dark: bool,
    /// Cached theme background for button rebuilds
    theme_bg: (u8, u8, u8),
    /// Whether the split panel is currently in tab mode (full editor area)
    is_tab_mode: bool,
    /// Cached diff title for tab bar display
    diff_title: String,
    /// Cached action definitions for rebuilding buttons after toggle
    cached_actions: Vec<SplitViewAction>,
    /// Cached session ID for action button rebuilds
    cached_session_id: u32,
    /// Editor font (from user settings), used in tab mode
    editor_font: Font,
    /// Editor font size (from user settings), used in tab mode
    editor_font_size: i32,
    /// Cached style table for reapplying on mode toggle (font changes)
    cached_style_table: Vec<StyleTableEntryExt>,
    /// DiffBg variant index for each cached style entry (parallel vec).
    /// Used to rebuild bgcolors on theme change.
    cached_diff_bg_indices: Vec<u8>,
}

impl SplitPanel {
    /// Create a new split panel
    pub fn new(sender: Sender<Message>) -> Self {
        let mut container = Flex::default().column();
        container.set_frame(FrameType::FlatBox);
        container.set_margin(0);
        container.set_pad(0);

        // Header bar with title
        let mut header = Frame::default();
        header.set_frame(FrameType::FlatBox);
        header.set_label_font(Font::HelveticaBold);
        header.set_label_size(12);
        header.set_align(Align::Left | Align::Inside);
        header.set_label("  Split View");
        container.fixed(&header, HEADER_HEIGHT);

        // Action button bar (between header and content)
        let mut action_bar = Flex::default().row();
        action_bar.set_frame(FrameType::FlatBox);
        action_bar.set_margin(8);
        action_bar.set_pad(4);

        Frame::default(); // spacer

        action_bar.end();
        container.fixed(&action_bar, ACTION_BAR_HEIGHT);

        // Labels row (side by side)
        let mut labels_row = Flex::default().row();
        labels_row.set_frame(FrameType::FlatBox);
        labels_row.set_margin(0);
        labels_row.set_pad(0);

        let mut left_label = Frame::default();
        left_label.set_frame(FrameType::FlatBox);
        left_label.set_label_size(11);
        left_label.set_label_font(Font::HelveticaBold);
        left_label.set_align(Align::Left | Align::Inside);
        left_label.set_label("  Left");

        let mut right_label = Frame::default();
        right_label.set_frame(FrameType::FlatBox);
        right_label.set_label_size(11);
        right_label.set_label_font(Font::HelveticaBold);
        right_label.set_align(Align::Left | Align::Inside);
        right_label.set_label("  Right");

        labels_row.end();
        container.fixed(&labels_row, LABEL_HEIGHT);

        // Content row: left and right text displays side by side
        let mut content_row = Flex::default().row();
        content_row.set_frame(FrameType::BorderBox);
        content_row.set_margin(2);
        content_row.set_pad(1); // 1px gap between panes

        let left_buffer = TextBuffer::default();
        let left_style_buffer = TextBuffer::default();
        let mut left_display = TextDisplay::default();
        left_display.set_buffer(left_buffer.clone());
        left_display.set_frame(FrameType::FlatBox);
        left_display.set_text_font(Font::Courier);
        left_display.set_text_size(13);
        left_display.set_linenumber_width(40);

        let right_buffer = TextBuffer::default();
        let right_style_buffer = TextBuffer::default();
        let mut right_editor = TextEditor::default();
        right_editor.set_buffer(right_buffer.clone());
        right_editor.set_frame(FrameType::FlatBox);
        right_editor.set_text_font(Font::Courier);
        right_editor.set_text_size(13);
        right_editor.set_linenumber_width(40);

        content_row.end();

        container.end();
        container.hide();

        // Read-only flag shared between SplitPanel and the right pane's handle closure
        let right_read_only = Rc::new(Cell::new(true));

        // Event-driven synchronized scrolling.
        // Combines mouse wheel sync and scrollbar drag sync in a single handler
        // per pane (FLTK's handle() replaces, not stacks).
        let syncing = Rc::new(Cell::new(false));

        {
            let flag = syncing.clone();
            let mut other = right_editor.clone();
            left_display.handle(move |disp, event| {
                match event {
                    Event::MouseWheel if !flag.get() => {
                        flag.set(true);
                        let dy = app::event_dy();
                        let delta = match dy {
                            app::MouseWheel::Up => -3,
                            app::MouseWheel::Down => 3,
                            _ => 0,
                        };
                        let current = get_vscrollbar_value(disp) as i32;
                        let new_line = (current + delta).max(1);
                        disp.scroll(new_line, 0);
                        other.scroll(new_line, 0);
                        flag.set(false);
                        true // consume
                    }
                    Event::Released if !flag.get() => {
                        // After scrollbar drag release, sync the other pane
                        flag.set(true);
                        let val = get_vscrollbar_value(disp) as i32;
                        other.scroll(val, 0);
                        flag.set(false);
                        false // don't consume — let FLTK finish handling
                    }
                    _ => false,
                }
            });
        }

        // Right pane handler: combines scroll sync + read-only blocking
        {
            let flag = syncing;
            let read_only = right_read_only.clone();
            let mut other = left_display.clone();
            right_editor.handle(move |ed, event| {
                // Block keyboard input and paste when read-only
                if read_only.get() {
                    match event {
                        Event::KeyDown | Event::Paste => return true, // consume to block
                        _ => {}
                    }
                }
                // Scroll sync
                match event {
                    Event::MouseWheel if !flag.get() => {
                        flag.set(true);
                        let dy = app::event_dy();
                        let delta = match dy {
                            app::MouseWheel::Up => -3,
                            app::MouseWheel::Down => 3,
                            _ => 0,
                        };
                        let current = get_vscrollbar_value_editor(ed) as i32;
                        let new_line = (current + delta).max(1);
                        ed.scroll(new_line, 0);
                        other.scroll(new_line, 0);
                        flag.set(false);
                        true
                    }
                    Event::Released if !flag.get() => {
                        flag.set(true);
                        let val = get_vscrollbar_value_editor(ed) as i32;
                        other.scroll(val, 0);
                        flag.set(false);
                        false
                    }
                    _ => false,
                }
            });
        }

        Self {
            container,
            divider: None,
            header,
            left_display,
            left_buffer,
            left_style_buffer,
            left_label,
            right_editor,
            right_read_only,
            right_buffer,
            right_style_buffer,
            right_label,
            labels_row,
            content_row,
            action_bar,
            sender,
            session_id: None,
            visible: false,
            is_dark: true,
            theme_bg: (30, 30, 30),
            is_tab_mode: false,
            diff_title: String::new(),
            cached_actions: Vec::new(),
            cached_session_id: 0,
            editor_font: Font::Courier,
            editor_font_size: 13,
            cached_style_table: Vec::new(),
            cached_diff_bg_indices: Vec::new(),
        }
    }

    /// Height of the draggable divider
    pub const DIVIDER_HEIGHT: i32 = 4;

    /// Get a reference to the container widget for layout
    pub fn widget(&self) -> &Flex {
        &self.container
    }

    /// Create the draggable divider widget (standalone).
    /// Call this within the parent Flex group BEFORE creating the SplitPanel,
    /// so the divider appears above the split panel in the column layout.
    /// Assign the returned Frame to `split_panel.divider` after construction.
    pub fn new_divider(sender: Sender<Message>) -> Frame {
        let mut divider = Frame::default();
        divider.set_frame(FrameType::FlatBox);
        divider.set_color(Color::from_rgb(80, 80, 80));
        divider.hide();

        let dragging = Rc::new(Cell::new(false));
        let drag_flag = dragging.clone();
        divider.handle(move |div, ev| {
            match ev {
                Event::Enter => {
                    if let Some(mut win) = div.window() {
                        win.set_cursor(Cursor::NS);
                    }
                    true
                }
                Event::Leave => {
                    if !drag_flag.get() && let Some(mut win) = div.window() {
                        win.set_cursor(Cursor::Default);
                    }
                    true
                }
                Event::Push => {
                    drag_flag.set(true);
                    true
                }
                Event::Drag => {
                    if drag_flag.get() {
                        let mouse_y = fltk::app::event_y();
                        sender.send(Message::SplitViewResize(mouse_y));
                    }
                    true
                }
                Event::Released => {
                    drag_flag.set(false);
                    if let Some(mut win) = div.window() {
                        win.set_cursor(Cursor::Default);
                    }
                    true
                }
                _ => false,
            }
        });

        divider
    }

    /// Show split view with syntax highlighting overlaid on diff backgrounds.
    ///
    /// `left_syntax` / `right_syntax` are the syntect style strings (one char per byte of content).
    /// `main_style_table` is the editor's style table mapping style chars → foreground colors.
    #[allow(clippy::too_many_arguments)]
    pub fn show_request_with_syntax(
        &mut self,
        session_id: u32,
        request: &SplitViewRequest,
        left_syntax: Option<&crate::app::services::syntax::FullHighlightResult>,
        right_syntax: Option<&crate::app::services::syntax::FullHighlightResult>,
        main_style_table: &[StyleTableEntryExt],
        theme_bg: (u8, u8, u8),
        theme_fg: (u8, u8, u8),
        font: Font,
        font_size: i32,
    ) {
        // Store font settings for later use (show_existing, refresh_action_buttons)
        self.editor_font = font;
        self.editor_font_size = font_size;

        // Set up common state (title, labels, buffers, actions) — same as show_request
        self.session_id = Some(session_id);
        self.is_tab_mode = request.display_mode == SplitDisplayMode::Tab;
        self.diff_title = request.title.clone();
        self.cached_actions = request.actions.clone();
        self.cached_session_id = session_id;

        if !request.title.is_empty() {
            self.header.set_label(&format!("  {}", request.title));
        } else {
            self.header.set_label("  Split View");
        }

        self.left_buffer.set_text(&request.left.content);
        if !request.left.label.is_empty() {
            self.left_label.set_label(&format!("  {}", request.left.label));
        } else {
            self.left_label.set_label("  Left");
        }
        if request.left.line_numbers {
            self.left_display.set_linenumber_width(40);
        } else {
            self.left_display.set_linenumber_width(0);
        }

        self.right_buffer.set_text(&request.right.content);
        if !request.right.label.is_empty() {
            self.right_label.set_label(&format!("  {}", request.right.label));
        } else {
            self.right_label.set_label("  Right");
        }
        if request.right.line_numbers {
            self.right_editor.set_linenumber_width(40);
        } else {
            self.right_editor.set_linenumber_width(0);
        }

        // Apply font in tab mode
        let (pane_font, pane_size) = if self.is_tab_mode {
            (font, font_size)
        } else {
            (Font::Courier, 13)
        };
        self.left_display.set_text_font(pane_font);
        self.left_display.set_text_size(pane_size);
        self.right_editor.set_text_font(pane_font);
        self.right_editor.set_text_size(pane_size);

        // Build combined syntax+diff style table
        let mut sdm = SyntaxDiffMap::new(
            self.is_dark, theme_bg, theme_fg, pane_font, pane_size,
        );

        // Apply syntax+diff highlights for each pane
        self.apply_syntax_diff_pane(
            true, // left
            &request.left.content,
            &request.left.highlights,
            left_syntax,
            main_style_table,
            &mut sdm,
        );
        self.apply_syntax_diff_pane(
            false, // right
            &request.right.content,
            &request.right.highlights,
            right_syntax,
            main_style_table,
            &mut sdm,
        );

        // Cache diff bg indices before consuming the map
        self.cached_diff_bg_indices = sdm.diff_bg_indices();
        let final_table = sdm.into_entries();

        // Cache and apply the style table to both displays
        self.cached_style_table = final_table.clone();
        self.left_display.set_highlight_data_ext(
            self.left_style_buffer.clone(),
            final_table.clone(),
        );
        self.right_editor.set_highlight_data_ext(
            self.right_style_buffer.clone(),
            final_table,
        );

        // Set right pane editability based on request
        self.set_right_editable(!request.right.read_only);

        // Rebuild action buttons
        self.rebuild_action_buttons(&request.actions, session_id);

        // Show the panel
        self.container.show();
        self.visible = true;

        self.update_tab_mode_colors();

        self.left_display.scroll(0, 0);
        self.right_editor.scroll(0, 0);

        self.container.redraw();
    }

    /// Apply combined syntax+diff highlighting for one pane.
    /// Writes the result into the appropriate style buffer.
    fn apply_syntax_diff_pane(
        &mut self,
        is_left: bool,
        content: &str,
        highlights: &[LineHighlight],
        syntax_result: Option<&crate::app::services::syntax::FullHighlightResult>,
        main_style_table: &[StyleTableEntryExt],
        sdm: &mut SyntaxDiffMap,
    ) {
        if content.is_empty() {
            if is_left {
                self.left_style_buffer.set_text("");
            } else {
                self.right_style_buffer.set_text("");
            }
            return;
        }

        let diff_map = build_diff_map(content, highlights);

        let style_string = match syntax_result {
            Some(result) => {
                let syntax_bytes = result.style_string.as_bytes();
                let mut combined = Vec::with_capacity(content.len());

                for (i, &diff_bg) in diff_map.iter().enumerate() {
                    // Get syntax foreground color from the main style table
                    let syntax_fg = if i < syntax_bytes.len() {
                        let style_idx = (syntax_bytes[i] - b'A') as usize;
                        if style_idx < main_style_table.len() {
                            Some(main_style_table[style_idx].color)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let ch = match syntax_fg {
                        Some(fg) => sdm.get_or_insert(fg, diff_bg),
                        None => (b'A' + diff_bg.base_index() as u8) as char,
                    };
                    combined.push(ch as u8);
                }

                String::from_utf8(combined).unwrap_or_default()
            }
            None => {
                // No syntax info: fall back to diff-only highlighting
                diff_map
                    .iter()
                    .map(|d| (b'A' + d.base_index() as u8) as char)
                    .collect()
            }
        };

        if is_left {
            self.left_style_buffer.set_text(&style_string);
        } else {
            self.right_style_buffer.set_text(&style_string);
        }
    }

    /// Rebuild action buttons based on the request.
    /// Re-uses the existing `self.action_bar` Flex (which is parented to the container)
    /// rather than creating a new orphaned Flex.
    fn rebuild_action_buttons(
        &mut self,
        actions: &[SplitViewAction],
        session_id: u32,
    ) {
        let theme = DialogTheme::from_theme_bg(self.theme_bg);

        // Clear existing children but keep the same Flex in the container
        self.action_bar.clear();
        self.action_bar.set_color(theme.bg);

        // Begin adding children to the existing action_bar
        self.action_bar.begin();

        Frame::default(); // spacer (flexible, pushes buttons right)

        // Add action buttons
        for action_def in actions {
            let mut btn = Button::default()
                .with_size(100, 28)
                .with_label(&action_def.label);
            btn.set_frame(FrameType::RFlatBox);
            btn.set_color(theme.button_bg);
            btn.set_label_color(theme.text);
            btn.set_label_size(12);

            let sender = self.sender;
            let action_name = action_def.action.clone();
            let action_label = action_def.label.clone();

            btn.set_callback(move |_| {
                if action_name == "accept" {
                    // Confirmation dialog for destructive accept actions (e.g. Revert to HEAD)
                    let msg = format!(
                        "Are you sure you want to \"{}\"?\n\nThis will replace the current editor content and cannot be undone.",
                        action_label
                    );
                    if dialog::choice2_default(&msg, "Cancel", &action_label, "") == Some(1) {
                        sender.send(Message::SplitViewAccept(session_id));
                    }
                } else if action_name == "reject" {
                    sender.send(Message::SplitViewReject(session_id));
                }
            });

            self.action_bar.fixed(&btn, 100);
        }

        // Add mode toggle button (shows opposite mode as label)
        let toggle_label = if self.is_tab_mode { "Panel Mode" } else { "Tab Mode" };
        let mut toggle_btn = Button::default()
            .with_size(90, 28)
            .with_label(toggle_label);
        toggle_btn.set_frame(FrameType::RFlatBox);
        toggle_btn.set_color(theme.button_bg);
        toggle_btn.set_label_color(theme.text);
        toggle_btn.set_label_size(12);

        let sender = self.sender;
        toggle_btn.set_callback(move |_| {
            sender.send(Message::SplitViewToggleMode(session_id));
        });

        self.action_bar.fixed(&toggle_btn, 90);

        // Add close button only in panel mode (tab mode uses the tab X button)
        if !self.is_tab_mode {
            let mut close_btn = Button::default()
                .with_size(60, 28)
                .with_label("Close");
            close_btn.set_frame(FrameType::RFlatBox);
            close_btn.set_color(theme.tab_active_bg);
            close_btn.set_label_color(theme.text);
            close_btn.set_label_size(12);

            let sender = self.sender;
            close_btn.set_callback(move |_| {
                sender.send(Message::SplitViewReject(session_id));
            });

            self.action_bar.fixed(&close_btn, 60);
        }

        self.action_bar.end();
    }

    /// Hide the split panel
    pub fn hide(&mut self) {
        self.container.hide();
        self.visible = false;
        self.session_id = None;
        self.is_tab_mode = false;
        self.diff_title.clear();
        self.cached_actions.clear();
        self.left_buffer.set_text("");
        self.right_buffer.set_text("");
        self.left_style_buffer.set_text("");
        self.right_style_buffer.set_text("");
    }

    /// Check if the panel is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the current session ID
    pub fn session_id(&self) -> Option<u32> {
        self.session_id
    }

    /// Get the content of the right pane (for accept action)
    pub fn right_content(&self) -> String {
        self.right_buffer.text()
    }

    /// Get the current panel height for flex layout
    pub fn current_height(&self) -> i32 {
        if self.visible {
            HEADER_HEIGHT + LABEL_HEIGHT + DEFAULT_CONTENT_HEIGHT + ACTION_BAR_HEIGHT
        } else {
            0
        }
    }

    /// Apply theme colors derived from the syntax theme background.
    pub fn apply_theme(&mut self, _is_dark: bool, theme_bg: (u8, u8, u8)) {
        let theme = DialogTheme::from_theme_bg(theme_bg);
        let (r, g, b) = theme_bg;
        self.is_dark = theme.is_dark();
        self.theme_bg = theme_bg;

        self.header.set_color(theme.bg);
        self.header.set_label_color(theme.text);

        // Labels row uses the diff tab tint color with dark text
        let label_bg = if theme.is_dark() {
            Color::from_rgb(r.saturating_add(8), g.saturating_add(4), b.saturating_add(15))
        } else {
            Color::from_rgb(r.saturating_sub(2), g.saturating_sub(2), b)
        };
        let label_text = if theme.is_dark() {
            Color::from_rgb(200, 200, 210)
        } else {
            Color::from_rgb(30, 30, 30)
        };
        self.labels_row.set_color(label_bg);
        self.left_label.set_color(label_bg);
        self.left_label.set_label_color(label_text);
        self.right_label.set_color(label_bg);
        self.right_label.set_label_color(label_text);

        let editor_bg = Color::from_rgb(r, g, b);
        self.content_row.set_color(editor_bg);
        self.left_display.set_color(editor_bg);
        self.left_display.set_text_color(theme.text);
        self.right_editor.set_color(editor_bg);
        self.right_editor.set_text_color(theme.text);

        let (ln_r, ln_g, ln_b) = if theme.is_dark() {
            super::dialogs::darken(r, g, b, 0.80)
        } else {
            super::dialogs::darken(r, g, b, 0.95)
        };
        self.left_display.set_linenumber_bgcolor(Color::from_rgb(ln_r, ln_g, ln_b));
        self.left_display.set_linenumber_fgcolor(theme.text_dim);
        self.right_editor.set_linenumber_bgcolor(Color::from_rgb(ln_r, ln_g, ln_b));
        self.right_editor.set_linenumber_fgcolor(theme.text_dim);

        self.action_bar.set_color(theme.bg);

        // In tab mode, override header/action_bar to match the diff tab's tinted background
        self.update_tab_mode_colors();

        self.style_text_display_scrollbars(&mut self.left_display.clone(), &theme);
        self.style_text_editor_scrollbars(&mut self.right_editor.clone(), &theme);

        // Rebuild diff background colors in the cached style table.
        // Each entry's bgcolor needs updating based on its DiffBg variant.
        if !self.cached_style_table.is_empty() && self.cached_diff_bg_indices.len() == self.cached_style_table.len() {
            let is_dark = theme.is_dark();
            let diff_bgs: [Color; 6] = [
                // Normal
                Color::from_rgb(r, g, b),
                // Added
                if is_dark { Color::from_rgb(20, 60, 20) } else { Color::from_rgb(200, 255, 200) },
                // Removed
                if is_dark { Color::from_rgb(60, 20, 20) } else { Color::from_rgb(255, 200, 200) },
                // Modified
                if is_dark { Color::from_rgb(60, 50, 10) } else { Color::from_rgb(255, 255, 200) },
                // AddedEmphasis
                if is_dark { Color::from_rgb(40, 100, 40) } else { Color::from_rgb(150, 255, 150) },
                // RemovedEmphasis
                if is_dark { Color::from_rgb(100, 40, 40) } else { Color::from_rgb(255, 150, 150) },
            ];

            // Update the foreground of the 6 base entries to theme fg
            let theme_fg = theme.text;
            for entry in self.cached_style_table.iter_mut().take(6) {
                entry.color = theme_fg;
            }

            // Update all entries' bgcolor based on their DiffBg variant
            for (entry, &diff_idx) in self.cached_style_table.iter_mut().zip(self.cached_diff_bg_indices.iter()) {
                if (diff_idx as usize) < diff_bgs.len() {
                    entry.bgcolor = diff_bgs[diff_idx as usize];
                }
            }

            self.left_display.set_highlight_data_ext(
                self.left_style_buffer.clone(),
                self.cached_style_table.clone(),
            );
            self.right_editor.set_highlight_data_ext(
                self.right_style_buffer.clone(),
                self.cached_style_table.clone(),
            );
        }
    }

    /// Update header and action bar colors based on tab mode.
    /// In tab mode, uses the diff tab's tinted background; otherwise uses dialog theme bg.
    fn update_tab_mode_colors(&mut self) {
        let (r, g, b) = self.theme_bg;
        if self.is_tab_mode {
            let tinted = if self.is_dark {
                Color::from_rgb(r.saturating_add(8), g.saturating_add(4), b.saturating_add(15))
            } else {
                Color::from_rgb(r.saturating_sub(2), g.saturating_sub(2), b)
            };
            self.header.set_color(tinted);
            self.action_bar.set_color(tinted);
        } else {
            let theme = DialogTheme::from_theme_bg(self.theme_bg);
            self.header.set_color(theme.bg);
            self.action_bar.set_color(theme.bg);
        }
    }

    /// Whether the panel is currently in tab mode (full editor area)
    pub fn is_tab_mode(&self) -> bool {
        self.is_tab_mode
    }

    /// Get the cached diff title for tab bar display
    pub fn diff_title(&self) -> &str {
        &self.diff_title
    }

    /// Set tab mode on or off
    pub fn set_tab_mode(&mut self, tab_mode: bool) {
        self.is_tab_mode = tab_mode;
    }

    /// Show the panel container without re-parsing data (buffers still populated).
    /// Used when switching back to the diff tab from a document tab.
    pub fn show_existing(&mut self) {
        self.container.show();
        self.visible = true;
        self.container.redraw();
    }

    /// Rebuild action buttons from cached data (e.g., after toggle mode changes label).
    /// Also updates font/size on TextDisplays and style table entries to match the mode.
    pub fn refresh_action_buttons(&mut self) {
        let actions = self.cached_actions.clone();
        let session_id = self.cached_session_id;
        self.rebuild_action_buttons(&actions, session_id);
        self.update_tab_mode_colors();
        self.update_display_fonts();
        self.container.redraw();
    }

    /// Update TextDisplay fonts and style table entry fonts to match the current mode.
    /// Tab mode uses the editor font/size; panel mode uses Courier 13.
    fn update_display_fonts(&mut self) {
        let (font, size) = if self.is_tab_mode {
            (self.editor_font, self.editor_font_size)
        } else {
            (Font::Courier, 13)
        };

        self.left_display.set_text_font(font);
        self.left_display.set_text_size(size);
        self.right_editor.set_text_font(font);
        self.right_editor.set_text_size(size);

        // Update all style table entries and reapply
        if !self.cached_style_table.is_empty() {
            for entry in &mut self.cached_style_table {
                entry.font = font;
                entry.size = size;
            }
            self.left_display.set_highlight_data_ext(
                self.left_style_buffer.clone(),
                self.cached_style_table.clone(),
            );
            self.right_editor.set_highlight_data_ext(
                self.right_style_buffer.clone(),
                self.cached_style_table.clone(),
            );
        }
    }

    /// Style scrollbars on a widget via FFI using its raw pointer.
    fn style_scrollbars_raw(widget_ptr: fltk::app::WidgetPtr, theme: &DialogTheme) {
        // SAFETY: widget_ptr is a valid Fl_Group subclass (TextDisplay/TextEditor).
        // Fl_Group_children/Fl_Group_child are stable FLTK C API. We null-check
        // child pointers and clamp the index to min(2) before access.
        unsafe extern "C" {
            fn Fl_Group_children(grp: *mut std::ffi::c_void) -> std::ffi::c_int;
            fn Fl_Group_child(
                grp: *mut std::ffi::c_void,
                index: std::ffi::c_int,
            ) -> *mut std::ffi::c_void;
        }
        unsafe {
            use fltk::valuator::Scrollbar;
            let group_ptr = widget_ptr as *mut std::ffi::c_void;
            let nchildren = Fl_Group_children(group_ptr);
            for i in 0..nchildren.min(2) {
                let ptr = Fl_Group_child(group_ptr, i);
                if !ptr.is_null() {
                    let mut sb = Scrollbar::from_widget_ptr(ptr as fltk::app::WidgetPtr);
                    sb.set_frame(FrameType::FlatBox);
                    sb.set_color(theme.scroll_track);
                    sb.set_slider_frame(FrameType::FlatBox);
                    sb.set_selection_color(theme.scroll_thumb);
                }
            }
        }
    }

    /// Style scrollbars on a TextDisplay widget.
    fn style_text_display_scrollbars(&self, display: &mut TextDisplay, theme: &DialogTheme) {
        display.set_scrollbar_size(SCROLLBAR_SIZE);
        Self::style_scrollbars_raw(display.as_widget_ptr(), theme);
    }

    /// Style scrollbars on a TextEditor widget.
    fn style_text_editor_scrollbars(&self, editor: &mut TextEditor, theme: &DialogTheme) {
        editor.set_scrollbar_size(SCROLLBAR_SIZE);
        Self::style_scrollbars_raw(editor.as_widget_ptr(), theme);
    }

    /// Set the right pane editable or read-only.
    pub fn set_right_editable(&mut self, editable: bool) {
        self.right_read_only.set(!editable);
    }
}
