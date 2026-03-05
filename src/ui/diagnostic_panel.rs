//! Diagnostic panel UI for displaying lint errors and warnings.
//!
//! Shows a collapsible panel below the editor with colored diagnostics
//! sorted by severity (errors first, then warnings, then info).

use fltk::{
    app::Sender,
    browser::HoldBrowser,
    enums::{Align, Color, Event, Font, FrameType},
    frame::Frame,
    group::Flex,
    misc::Tooltip,
    prelude::*,
};
use std::cell::RefCell;
use std::rc::Rc;

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
    /// Shared diagnostics for hover handler
    shared_diagnostics: Option<Rc<RefCell<Vec<Diagnostic>>>>,
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
            shared_diagnostics: None,
        }
    }

    /// Get a reference to the container widget for layout
    pub fn widget(&self) -> &Flex {
        &self.container
    }

    /// Update the panel with new diagnostics
    pub fn update_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.diagnostics = diagnostics;
        self.sync_shared_diagnostics();
        self.refresh_display();
    }

    /// Clear all diagnostics and hide the panel (for documents not yet linted)
    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.browser.clear();
        self.container.hide();
        self.expanded = false;
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

    /// Get the selected diagnostic's documentation URL (for double-click)
    pub fn selected_url(&self) -> Option<String> {
        let idx = self.browser.value();
        if idx > 0 && (idx as usize) <= self.diagnostics.len() {
            self.diagnostics[idx as usize - 1].url.clone()
        } else {
            None
        }
    }

    /// Set up click and hover handlers for browser
    /// Single click = go to line, Double click = open docs URL, Hover = tooltip
    pub fn setup_click_handler(&mut self) {
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

    /// Set up hover handler for dynamic tooltips AND click handlers
    /// Must be called after setup_click_handler (combines all browser event handling)
    pub fn setup_hover_handler(&mut self) {
        // Enable tooltips and set a short delay
        Tooltip::enable(true);
        Tooltip::set_delay(0.5);  // 500ms delay before showing

        // Share diagnostics with the event handler
        let diagnostics: Rc<RefCell<Vec<Diagnostic>>> = Rc::new(RefCell::new(Vec::new()));
        self.shared_diagnostics = Some(Rc::clone(&diagnostics));

        // Track last item to avoid resetting tooltip unnecessarily
        let last_item: Rc<RefCell<i32>> = Rc::new(RefCell::new(-1));

        let diags = Rc::clone(&diagnostics);
        let last = Rc::clone(&last_item);
        let sender = self.sender;

        // Helper to update tooltip for a given item index
        let update_tooltip = |b: &mut HoldBrowser, diags: &Rc<RefCell<Vec<Diagnostic>>>, item_idx: i32| {
            let borrowed = diags.borrow();
            if item_idx >= 0 && (item_idx as usize) < borrowed.len() {
                let diag = &borrowed[item_idx as usize];
                let mut tooltip = format!(
                    "Line {}: {}\nSource: {}",
                    diag.line, diag.message, diag.source
                );
                if let Some(ref fix) = diag.fix_message {
                    tooltip.push_str(&format!("\n\nFix: {}", fix));
                }
                if let Some(ref url) = diag.url {
                    tooltip.push_str(&format!("\nDocs: {}  (double-click to open)", url));
                }
                b.set_tooltip(&tooltip);
            } else {
                b.set_tooltip("");
            }
        };

        // Combined handler for hover (tooltip) and click (goto/open docs)
        self.browser.handle(move |b, ev| {
            match ev {
                Event::Enter | Event::Move => {
                    // Hover: update tooltip based on mouse position
                    let y = fltk::app::event_y();
                    let browser_y = b.y();
                    let item_height = b.text_size() + 6;
                    let scroll_pixels = b.position();

                    if y >= browser_y {
                        let relative_y = y - browser_y + scroll_pixels;
                        let item_idx = relative_y / item_height;

                        let mut last_val = last.borrow_mut();
                        if item_idx != *last_val {
                            *last_val = item_idx;
                            update_tooltip(b, &diags, item_idx);
                        }
                    }
                    false
                }
                Event::Leave => {
                    *last.borrow_mut() = -1;
                    b.set_tooltip("");
                    false
                }
                Event::Released => {
                    // Single click - go to line
                    let idx = b.value();
                    if idx > 0 {
                        sender.send(Message::DiagnosticGoto(idx as u32));
                        // Update tooltip for clicked item (0-indexed)
                        *last.borrow_mut() = idx - 1;
                        update_tooltip(b, &diags, idx - 1);
                    }
                    false  // Don't consume - let FLTK handle selection
                }
                Event::Push => {
                    // Double click - open docs
                    if fltk::app::event_clicks() {
                        let idx = b.value();
                        if idx > 0 {
                            sender.send(Message::DiagnosticOpenDocs(idx as u32));
                        }
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            }
        });
    }

    /// Update shared diagnostics for hover handler
    pub fn sync_shared_diagnostics(&mut self) {
        if let Some(ref shared) = self.shared_diagnostics {
            *shared.borrow_mut() = self.diagnostics.clone();
        }
    }

    /// Returns true if showing the green "All checks passed" success bar
    pub fn is_showing_success(&self) -> bool {
        self.container.visible() && self.diagnostics.is_empty()
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
