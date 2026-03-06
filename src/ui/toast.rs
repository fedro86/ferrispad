//! Toast notification widget for transient messages.
//!
//! Shows brief messages at the top of the editor that auto-hide after a few seconds.
//! Used for plugin status, errors, and other transient feedback.

use fltk::{
    app::Sender,
    enums::{Align, Color, Font, FrameType},
    frame::Frame,
    prelude::*,
};

use crate::app::Message;

/// Toast notification levels with associated colors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    /// Success - green background
    Success,
    /// Info - blue background
    Info,
    /// Warning - orange background
    Warning,
    /// Error - red background
    Error,
}

impl ToastLevel {
    /// Get background color for this level (dark mode)
    fn bg_color_dark(&self) -> Color {
        match self {
            ToastLevel::Success => Color::from_rgb(46, 125, 50),   // Green
            ToastLevel::Info => Color::from_rgb(25, 118, 210),     // Blue
            ToastLevel::Warning => Color::from_rgb(245, 124, 0),   // Orange
            ToastLevel::Error => Color::from_rgb(198, 40, 40),     // Red
        }
    }

    /// Get background color for this level (light mode)
    fn bg_color_light(&self) -> Color {
        match self {
            ToastLevel::Success => Color::from_rgb(200, 230, 201), // Light green
            ToastLevel::Info => Color::from_rgb(187, 222, 251),    // Light blue
            ToastLevel::Warning => Color::from_rgb(255, 224, 178), // Light orange
            ToastLevel::Error => Color::from_rgb(255, 205, 210),   // Light red
        }
    }

    /// Get text color for this level
    fn text_color(&self, is_dark: bool) -> Color {
        if is_dark {
            Color::White
        } else {
            match self {
                ToastLevel::Success => Color::from_rgb(27, 94, 32),
                ToastLevel::Info => Color::from_rgb(13, 71, 161),
                ToastLevel::Warning => Color::from_rgb(230, 81, 0),
                ToastLevel::Error => Color::from_rgb(183, 28, 28),
            }
        }
    }
}

/// Height of the toast notification bar
pub const TOAST_HEIGHT: i32 = 28;

/// Toast notification widget
pub struct Toast {
    /// The frame widget
    frame: Frame,
    /// Whether currently visible
    visible: bool,
    /// Current dark mode state
    is_dark: bool,
    /// Message sender for auto-hide timer
    sender: Sender<Message>,
}

impl Toast {
    /// Create a new toast notification widget
    pub fn new(sender: Sender<Message>) -> Self {
        let mut frame = Frame::default()
            .with_size(0, TOAST_HEIGHT);
        frame.set_frame(FrameType::FlatBox);
        frame.set_label_font(Font::Helvetica);
        frame.set_label_size(12);
        frame.set_align(Align::Left | Align::Inside);
        frame.hide();

        Self {
            frame,
            visible: false,
            is_dark: true,
            sender,
        }
    }

    /// Get reference to the frame widget for layout
    pub fn widget(&self) -> &Frame {
        &self.frame
    }

    /// Show a toast message
    pub fn show(&mut self, level: ToastLevel, message: &str) {
        // Set colors based on level and theme
        let bg = if self.is_dark {
            level.bg_color_dark()
        } else {
            level.bg_color_light()
        };
        let fg = level.text_color(self.is_dark);

        self.frame.set_color(bg);
        self.frame.set_label_color(fg);
        self.frame.set_label(&format!("  {}", message));
        self.frame.show();
        self.frame.redraw();
        self.visible = true;

        // Schedule auto-hide after 4 seconds
        let sender = self.sender;
        fltk::app::add_timeout3(4.0, move |_| {
            sender.send(Message::ToastHide);
        });
    }

    /// Hide the toast
    pub fn hide(&mut self) {
        self.frame.hide();
        self.visible = false;
    }

    /// Apply theme
    pub fn apply_theme(&mut self, is_dark: bool) {
        self.is_dark = is_dark;
    }

    /// Get current height (0 if hidden)
    pub fn current_height(&self) -> i32 {
        if self.visible { TOAST_HEIGHT } else { 0 }
    }
}
