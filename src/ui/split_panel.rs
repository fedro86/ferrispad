//! Split panel UI for displaying side-by-side content views.
//!
//! Used for showing diffs, AI suggestions, and file comparisons.
//! Plugin-driven via the Widget API.

use fltk::{
    app::Sender,
    button::Button,
    enums::{Align, Color, Font, FrameType},
    frame::Frame,
    group::{Flex, Tile},
    prelude::*,
    text::{TextBuffer, TextDisplay},
};

use crate::app::plugins::widgets::SplitViewRequest;
#[allow(unused_imports)]  // Used in apply_highlights when style buffer is implemented
use crate::app::plugins::widgets::HighlightColor;
use crate::app::Message;

/// Height of the split panel header
const HEADER_HEIGHT: i32 = 24;

/// Height of the action button bar
const ACTION_BAR_HEIGHT: i32 = 32;

/// Default height of the split panel content area
const DEFAULT_CONTENT_HEIGHT: i32 = 200;

/// Split panel widget for showing side-by-side content
pub struct SplitPanel {
    /// The outer container (Flex column)
    pub container: Flex,
    /// Header frame showing title
    header: Frame,
    /// Tile containing left and right panes
    tile: Tile,
    /// Left pane text display
    left_display: TextDisplay,
    /// Left pane buffer
    left_buffer: TextBuffer,
    /// Left pane label
    left_label: Frame,
    /// Right pane text display
    right_display: TextDisplay,
    /// Right pane buffer
    right_buffer: TextBuffer,
    /// Right pane label
    right_label: Frame,
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
}

impl SplitPanel {
    /// Create a new split panel
    pub fn new(sender: Sender<Message>) -> Self {
        let mut container = Flex::default().column();
        container.set_frame(FrameType::FlatBox);

        // Header bar with title
        let mut header = Frame::default().with_size(0, HEADER_HEIGHT);
        header.set_frame(FrameType::FlatBox);
        header.set_color(Color::from_rgb(60, 60, 60));
        header.set_label_color(Color::White);
        header.set_label_font(Font::HelveticaBold);
        header.set_label_size(12);
        header.set_align(Align::Left | Align::Inside);
        header.set_label("  Split View");
        container.fixed(&header, HEADER_HEIGHT);

        // Tile for resizable left/right split
        let mut tile = Tile::default();
        tile.set_frame(FrameType::FlatBox);

        // Left pane container
        let mut left_container = Flex::default().column();
        left_container.set_frame(FrameType::FlatBox);

        let mut left_label = Frame::default().with_size(0, 20);
        left_label.set_frame(FrameType::FlatBox);
        left_label.set_color(Color::from_rgb(50, 50, 50));
        left_label.set_label_color(Color::from_rgb(200, 200, 200));
        left_label.set_label_size(11);
        left_label.set_align(Align::Left | Align::Inside);
        left_label.set_label("  Left");
        left_container.fixed(&left_label, 20);

        let left_buffer = TextBuffer::default();
        let mut left_display = TextDisplay::default();
        left_display.set_buffer(left_buffer.clone());
        left_display.set_frame(FrameType::FlatBox);
        left_display.set_color(Color::from_rgb(30, 30, 30));
        left_display.set_text_color(Color::from_rgb(220, 220, 220));
        left_display.set_text_font(Font::Courier);
        left_display.set_text_size(13);
        left_display.set_linenumber_width(40);
        left_display.set_linenumber_bgcolor(Color::from_rgb(40, 40, 40));
        left_display.set_linenumber_fgcolor(Color::from_rgb(120, 120, 120));

        left_container.end();

        // Right pane container
        let mut right_container = Flex::default().column();
        right_container.set_frame(FrameType::FlatBox);

        let mut right_label = Frame::default().with_size(0, 20);
        right_label.set_frame(FrameType::FlatBox);
        right_label.set_color(Color::from_rgb(50, 50, 50));
        right_label.set_label_color(Color::from_rgb(200, 200, 200));
        right_label.set_label_size(11);
        right_label.set_align(Align::Left | Align::Inside);
        right_label.set_label("  Right");
        right_container.fixed(&right_label, 20);

        let right_buffer = TextBuffer::default();
        let mut right_display = TextDisplay::default();
        right_display.set_buffer(right_buffer.clone());
        right_display.set_frame(FrameType::FlatBox);
        right_display.set_color(Color::from_rgb(30, 30, 30));
        right_display.set_text_color(Color::from_rgb(220, 220, 220));
        right_display.set_text_font(Font::Courier);
        right_display.set_text_size(13);
        right_display.set_linenumber_width(40);
        right_display.set_linenumber_bgcolor(Color::from_rgb(40, 40, 40));
        right_display.set_linenumber_fgcolor(Color::from_rgb(120, 120, 120));

        right_container.end();

        tile.end();

        // Action button bar
        let mut action_bar = Flex::default().row();
        action_bar.set_frame(FrameType::FlatBox);
        action_bar.set_color(Color::from_rgb(50, 50, 50));

        // Spacer to push buttons to the right
        let spacer = Frame::default();
        action_bar.fixed(&spacer, 0);

        action_bar.end();
        container.fixed(&action_bar, ACTION_BAR_HEIGHT);

        container.end();
        container.hide();

        Self {
            container,
            header,
            tile,
            left_display,
            left_buffer,
            left_label,
            right_display,
            right_buffer,
            right_label,
            action_bar,
            sender,
            session_id: None,
            visible: false,
            is_dark: true,
        }
    }

    /// Get a reference to the container widget for layout
    pub fn widget(&self) -> &Flex {
        &self.container
    }

    /// Show the split panel with content from a plugin request
    pub fn show_request(&mut self, session_id: u32, request: &SplitViewRequest) {
        self.session_id = Some(session_id);

        // Update header title
        if !request.title.is_empty() {
            self.header.set_label(&format!("  {}", request.title));
        } else {
            self.header.set_label("  Split View");
        }

        // Update left pane
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

        // Update right pane
        self.right_buffer.set_text(&request.right.content);
        if !request.right.label.is_empty() {
            self.right_label.set_label(&format!("  {}", request.right.label));
        } else {
            self.right_label.set_label("  Right");
        }
        if request.right.line_numbers {
            self.right_display.set_linenumber_width(40);
        } else {
            self.right_display.set_linenumber_width(0);
        }

        // Apply highlights to right pane (diff coloring)
        self.apply_highlights(&request.right.highlights);

        // Rebuild action buttons
        self.rebuild_action_buttons(&request.actions, session_id);

        // Show the panel
        self.container.show();
        self.visible = true;
        self.container.redraw();
    }

    /// Apply line highlights to the right pane
    fn apply_highlights(&mut self, highlights: &[crate::app::plugins::widgets::LineHighlight]) {
        // For now, we'll just apply background colors to lines
        // A full implementation would use a style buffer
        for highlight in highlights {
            let (r, g, b) = if self.is_dark {
                highlight.color.to_rgb_dark()
            } else {
                highlight.color.to_rgb()
            };

            // Note: FLTK TextDisplay doesn't support per-line background colors natively
            // A full implementation would require a custom style buffer
            // For now, we just store the highlight info for potential future use
            let _ = (highlight.line, r, g, b);
        }
    }

    /// Rebuild action buttons based on the request
    fn rebuild_action_buttons(
        &mut self,
        actions: &[crate::app::plugins::widgets::SplitViewAction],
        session_id: u32,
    ) {
        // Clear existing buttons (keep spacer)
        self.action_bar.clear();

        let mut action_bar = Flex::default().row();
        action_bar.set_frame(FrameType::FlatBox);
        action_bar.set_color(Color::from_rgb(50, 50, 50));

        // Spacer to push buttons to the right
        let spacer = Frame::default();
        action_bar.fixed(&spacer, 0);

        // Add action buttons
        for action_def in actions {
            let mut btn = Button::default()
                .with_size(80, 24)
                .with_label(&action_def.label);
            btn.set_frame(FrameType::FlatBox);
            btn.set_color(Color::from_rgb(70, 130, 180));
            btn.set_label_color(Color::White);
            btn.set_label_size(11);

            let sender = self.sender;
            let action_name = action_def.action.clone();

            btn.set_callback(move |_| {
                if action_name == "accept" {
                    sender.send(Message::SplitViewAccept(session_id));
                } else if action_name == "reject" {
                    sender.send(Message::SplitViewReject(session_id));
                }
            });

            action_bar.fixed(&btn, 80);
        }

        // Add close button
        let mut close_btn = Button::default()
            .with_size(60, 24)
            .with_label("Close");
        close_btn.set_frame(FrameType::FlatBox);
        close_btn.set_color(Color::from_rgb(100, 100, 100));
        close_btn.set_label_color(Color::White);
        close_btn.set_label_size(11);

        let sender = self.sender;
        close_btn.set_callback(move |_| {
            sender.send(Message::SplitViewHide(session_id));
        });

        action_bar.fixed(&close_btn, 60);
        action_bar.end();

        // Replace old action bar
        self.action_bar = action_bar;
    }

    /// Hide the split panel
    pub fn hide(&mut self) {
        self.container.hide();
        self.visible = false;
        self.session_id = None;
        self.left_buffer.set_text("");
        self.right_buffer.set_text("");
    }

    /// Check if the panel is visible
    #[allow(dead_code)]
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
            HEADER_HEIGHT + DEFAULT_CONTENT_HEIGHT + ACTION_BAR_HEIGHT
        } else {
            0
        }
    }

    /// Apply theme colors
    pub fn apply_theme(&mut self, is_dark: bool) {
        self.is_dark = is_dark;

        if is_dark {
            self.header.set_color(Color::from_rgb(60, 60, 60));
            self.header.set_label_color(Color::White);
            self.left_label.set_color(Color::from_rgb(50, 50, 50));
            self.left_label.set_label_color(Color::from_rgb(200, 200, 200));
            self.right_label.set_color(Color::from_rgb(50, 50, 50));
            self.right_label.set_label_color(Color::from_rgb(200, 200, 200));
            self.left_display.set_color(Color::from_rgb(30, 30, 30));
            self.left_display.set_text_color(Color::from_rgb(220, 220, 220));
            self.left_display.set_linenumber_bgcolor(Color::from_rgb(40, 40, 40));
            self.left_display.set_linenumber_fgcolor(Color::from_rgb(120, 120, 120));
            self.right_display.set_color(Color::from_rgb(30, 30, 30));
            self.right_display.set_text_color(Color::from_rgb(220, 220, 220));
            self.right_display.set_linenumber_bgcolor(Color::from_rgb(40, 40, 40));
            self.right_display.set_linenumber_fgcolor(Color::from_rgb(120, 120, 120));
            self.action_bar.set_color(Color::from_rgb(50, 50, 50));
        } else {
            self.header.set_color(Color::from_rgb(220, 220, 220));
            self.header.set_label_color(Color::from_rgb(30, 30, 30));
            self.left_label.set_color(Color::from_rgb(240, 240, 240));
            self.left_label.set_label_color(Color::from_rgb(60, 60, 60));
            self.right_label.set_color(Color::from_rgb(240, 240, 240));
            self.right_label.set_label_color(Color::from_rgb(60, 60, 60));
            self.left_display.set_color(Color::White);
            self.left_display.set_text_color(Color::from_rgb(30, 30, 30));
            self.left_display.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240));
            self.left_display.set_linenumber_fgcolor(Color::from_rgb(120, 120, 120));
            self.right_display.set_color(Color::White);
            self.right_display.set_text_color(Color::from_rgb(30, 30, 30));
            self.right_display.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240));
            self.right_display.set_linenumber_fgcolor(Color::from_rgb(120, 120, 120));
            self.action_bar.set_color(Color::from_rgb(230, 230, 230));
        }
    }
}
