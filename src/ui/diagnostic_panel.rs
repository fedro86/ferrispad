//! Diagnostic panel UI for displaying lint errors and warnings.
//!
//! Shows a collapsible panel below the editor with colored diagnostics
//! sorted by severity (errors first, then warnings, then info).

use fltk::{
    app::Sender,
    browser::HoldBrowser,
    enums::{Align, Color, Font, FrameType},
    frame::Frame,
    group::Flex,
    prelude::*,
};

use crate::app::plugins::{Diagnostic, DiagnosticLevel};
use crate::app::Message;

/// Height of the diagnostic panel when visible
pub const DIAGNOSTIC_PANEL_HEIGHT: i32 = 120;

/// Height of just the header bar
const HEADER_HEIGHT: i32 = 24;

/// Diagnostic panel widget
pub struct DiagnosticPanel {
    /// The outer container (Flex column)
    pub container: Flex,
    /// Header frame showing summary
    header: Frame,
    /// Browser widget listing diagnostics
    browser: HoldBrowser,
    /// Current diagnostics
    diagnostics: Vec<Diagnostic>,
    /// Whether panel is expanded
    expanded: bool,
    /// Message sender
    sender: Sender<Message>,
}

impl DiagnosticPanel {
    /// Create a new diagnostic panel
    pub fn new(sender: Sender<Message>) -> Self {
        let mut container = Flex::default().column();
        container.set_frame(FrameType::FlatBox);

        // Header bar with summary
        let mut header = Frame::default()
            .with_size(0, HEADER_HEIGHT);
        header.set_frame(FrameType::FlatBox);
        header.set_color(Color::from_rgb(60, 60, 60));
        header.set_label_color(Color::White);
        header.set_label_font(Font::HelveticaBold);
        header.set_label_size(12);
        header.set_align(Align::Left | Align::Inside);
        header.set_label("  \u{2714} All checks passed");
        container.fixed(&header, HEADER_HEIGHT);

        // Browser for listing diagnostics
        let mut browser = HoldBrowser::default();
        browser.set_frame(FrameType::FlatBox);
        browser.set_color(Color::from_rgb(40, 40, 40));
        browser.set_selection_color(Color::from_rgb(80, 80, 80));
        browser.set_text_size(12);
        browser.hide();

        container.end();
        container.hide();

        Self {
            container,
            header,
            browser,
            diagnostics: Vec::new(),
            expanded: false,
            sender,
        }
    }

    /// Get a reference to the container widget for layout
    pub fn widget(&self) -> &Flex {
        &self.container
    }

    /// Update the panel with new diagnostics
    pub fn update_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.diagnostics = diagnostics;
        self.refresh_display();
    }

    /// Clear all diagnostics (show success state)
    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.refresh_display();
    }

    /// Refresh the visual display based on current diagnostics
    fn refresh_display(&mut self) {
        self.browser.clear();

        if self.diagnostics.is_empty() {
            // All good - show green success state
            self.header.set_color(Color::from_rgb(46, 125, 50));  // Green
            self.header.set_label("  \u{2714} All checks passed");
            self.browser.hide();
            self.expanded = false;
            self.container.show();
        } else {
            // Count by severity
            let error_count = self.diagnostics.iter()
                .filter(|d| d.level == DiagnosticLevel::Error)
                .count();
            let warning_count = self.diagnostics.iter()
                .filter(|d| d.level == DiagnosticLevel::Warning)
                .count();
            let info_count = self.diagnostics.iter()
                .filter(|d| d.level == DiagnosticLevel::Info || d.level == DiagnosticLevel::Hint)
                .count();

            // Set header color based on most severe
            if error_count > 0 {
                self.header.set_color(Color::from_rgb(198, 40, 40));  // Red
            } else if warning_count > 0 {
                self.header.set_color(Color::from_rgb(245, 124, 0));  // Orange
            } else {
                self.header.set_color(Color::from_rgb(25, 118, 210));  // Blue
            }

            // Build header label
            let mut parts = Vec::new();
            if error_count > 0 {
                parts.push(format!("{} error{}", error_count, if error_count == 1 { "" } else { "s" }));
            }
            if warning_count > 0 {
                parts.push(format!("{} warning{}", warning_count, if warning_count == 1 { "" } else { "s" }));
            }
            if info_count > 0 {
                parts.push(format!("{} info", info_count));
            }
            self.header.set_label(&format!("  \u{26A0} {}", parts.join(", ")));

            // Populate browser with diagnostic entries
            for diag in &self.diagnostics {
                let (icon, color) = match diag.level {
                    DiagnosticLevel::Error => ("\u{2718}", "@C88"),    // Red X
                    DiagnosticLevel::Warning => ("\u{26A0}", "@C94"),  // Orange warning
                    DiagnosticLevel::Info => ("\u{2139}", "@C12"),     // Blue info
                    DiagnosticLevel::Hint => ("\u{2022}", "@C8"),      // Gray dot
                };

                let col_info = if let Some(col) = diag.column {
                    format!(":{}", col)
                } else {
                    String::new()
                };

                // Format: @C<color> icon Line N:col - message (source)
                let line = format!(
                    "{} {} Line {}{}: {} [{}]",
                    color, icon, diag.line, col_info, diag.message, diag.source
                );
                self.browser.add(&line);
            }

            self.browser.show();
            self.expanded = true;
            self.container.show();
        }

        self.container.redraw();
    }

    /// Show the panel
    #[allow(dead_code)]  // Reserved for future manual show/hide toggle
    pub fn show(&mut self) {
        self.container.show();
    }

    /// Hide the panel
    pub fn hide(&mut self) {
        self.container.hide();
    }

    /// Check if panel is currently visible
    #[allow(dead_code)]  // Reserved for future visibility checks
    pub fn visible(&self) -> bool {
        self.container.visible()
    }

    /// Get the selected diagnostic's line number (for goto)
    pub fn selected_line(&self) -> Option<u32> {
        let idx = self.browser.value();
        if idx > 0 && (idx as usize) <= self.diagnostics.len() {
            Some(self.diagnostics[idx as usize - 1].line)
        } else {
            None
        }
    }

    /// Set up click handler for browser
    pub fn setup_click_handler(&mut self) {
        let sender = self.sender;
        self.browser.set_callback(move |b| {
            let idx = b.value();
            if idx > 0 {
                // We need to get the line from the diagnostic
                // For now, just send a message to handle it in state
                sender.send(Message::DiagnosticGoto(idx as u32));
            }
        });

        // Header click to toggle expand/collapse
        let mut browser = self.browser.clone();
        self.header.set_callback(move |_| {
            if browser.visible() {
                browser.hide();
            } else {
                browser.show();
            }
        });
    }

    /// Get current panel height for flex layout
    pub fn current_height(&self) -> i32 {
        if !self.container.visible() {
            0
        } else if self.expanded && !self.diagnostics.is_empty() {
            DIAGNOSTIC_PANEL_HEIGHT
        } else {
            HEADER_HEIGHT
        }
    }

    /// Apply theme colors
    #[allow(dead_code)]  // Reserved for future theme support
    pub fn apply_theme(&mut self, is_dark: bool) {
        if is_dark {
            self.browser.set_color(Color::from_rgb(40, 40, 40));
        } else {
            self.browser.set_color(Color::from_rgb(250, 250, 250));
        }
    }
}
