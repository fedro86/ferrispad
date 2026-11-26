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

use super::theme::{PanelTheme, Themeable};
use crate::app::Message;
use crate::app::plugins::{Diagnostic, DiagnosticLevel};
use crate::ui::contrast::best_black_or_white;

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
    /// Text color of each level on the current background (T0104)
    level_colors: [(u8, u8, u8); 4],
}

impl DiagnosticPanel {
    /// Create a new diagnostic panel
    pub fn new(sender: Sender<Message>) -> Self {
        let mut container = Flex::default().column();
        container.set_frame(FrameType::FlatBox);

        // Header bar with summary
        let mut header = Frame::default().with_size(0, HEADER_HEIGHT);
        header.set_frame(FrameType::FlatBox);
        header.set_label_font(Font::HelveticaBold);
        header.set_label_size(12);
        header.set_align(Align::Left | Align::Inside);
        header.set_label("  \u{2714} All checks passed");
        container.fixed(&header, HEADER_HEIGHT);

        // Browser for listing diagnostics
        let mut browser = HoldBrowser::default();
        browser.set_frame(FrameType::FlatBox);
        browser.set_text_size(12);
        browser.hide();

        container.end();
        container.hide();

        // Until the first theme arrives: FLTK's own list background.
        let panel_bg = browser.color().to_rgb();
        let mut panel = Self {
            container,
            header,
            browser,
            diagnostics: Vec::new(),
            expanded: false,
            sender,
            shared_diagnostics: None,
            level_colors: diagnostic_level_colors(panel_bg),
        };
        panel.set_panel_bg(panel_bg);
        panel
    }

    /// Derive every color of the panel from its list background (T0104).
    fn set_panel_bg(&mut self, (r, g, b): (u8, u8, u8)) {
        self.browser.set_color(Color::from_rgb(r, g, b));
        // Selected row as in the file tree.
        crate::ui::theme::style_panel_selection(&mut self.browser, (r, g, b));
        self.level_colors = diagnostic_level_colors((r, g, b));
        self.paint_header(HeaderState::of(&self.diagnostics));
    }

    /// The header bar's fill and text for `state`.
    fn paint_header(&mut self, state: HeaderState) {
        let ((r, g, b), text) = diagnostic_header_colors(state);
        self.header.set_color(Color::from_rgb(r, g, b));
        self.header.set_label_color(text);
    }

    /// Get a reference to the container widget for layout
    pub fn widget(&self) -> &Flex {
        &self.container
    }

    /// Update the font size used to render diagnostic entries.
    /// `font` is currently ignored: fltk-rs `HoldBrowser` exposes `set_text_size`
    /// but not `set_text_font`. The header label keeps its bold UI-chrome font.
    #[allow(unused_variables)]
    pub fn set_code_font(&mut self, font: Font, size: i32) {
        let size = size.clamp(6, 96);
        self.browser.set_text_size(size);
        self.browser.redraw();
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
            // All good - show the success state
            self.paint_header(HeaderState::Clean);
            self.header.set_label("  \u{2714} All checks passed");
            self.browser.hide();
            self.expanded = false;
            self.container.show();
        } else {
            // Count by severity
            let error_count = self
                .diagnostics
                .iter()
                .filter(|d| d.level == DiagnosticLevel::Error)
                .count();
            let warning_count = self
                .diagnostics
                .iter()
                .filter(|d| d.level == DiagnosticLevel::Warning)
                .count();
            let info_count = self
                .diagnostics
                .iter()
                .filter(|d| d.level == DiagnosticLevel::Info || d.level == DiagnosticLevel::Hint)
                .count();

            // Header in the hue of the most severe level present
            self.paint_header(HeaderState::of(&self.diagnostics));

            // Build header label
            let mut parts = Vec::new();
            if error_count > 0 {
                parts.push(format!(
                    "{} error{}",
                    error_count,
                    if error_count == 1 { "" } else { "s" }
                ));
            }
            if warning_count > 0 {
                parts.push(format!(
                    "{} warning{}",
                    warning_count,
                    if warning_count == 1 { "" } else { "s" }
                ));
            }
            if info_count > 0 {
                parts.push(format!("{} info", info_count));
            }
            self.header
                .set_label(&format!("  \u{26A0} {}", parts.join(", ")));

            // Populate browser with diagnostic entries
            for diag in &self.diagnostics {
                let (icon, level) = match diag.level {
                    DiagnosticLevel::Error => ("\u{2718}", 0),   // Red X
                    DiagnosticLevel::Warning => ("\u{26A0}", 1), // Orange warning
                    DiagnosticLevel::Info => ("\u{2139}", 2),    // Blue info
                    DiagnosticLevel::Hint => ("\u{2022}", 3),    // Gray dot
                };
                let color = color_code(self.level_colors[level]);

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
    pub fn show(&mut self) {
        self.container.show();
    }

    /// Hide the panel
    pub fn hide(&mut self) {
        self.container.hide();
    }

    /// Check if panel is currently visible
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
        Tooltip::set_delay(0.5); // 500ms delay before showing

        // Share diagnostics with the event handler
        let diagnostics: Rc<RefCell<Vec<Diagnostic>>> = Rc::new(RefCell::new(Vec::new()));
        self.shared_diagnostics = Some(Rc::clone(&diagnostics));

        // Track last item to avoid resetting tooltip unnecessarily
        let last_item: Rc<RefCell<i32>> = Rc::new(RefCell::new(-1));

        let diags = Rc::clone(&diagnostics);
        let last = Rc::clone(&last_item);
        let sender = self.sender;

        // Helper to update tooltip for a given item index
        let update_tooltip =
            |b: &mut HoldBrowser, diags: &Rc<RefCell<Vec<Diagnostic>>>, item_idx: i32| {
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
                    false // Don't consume - let FLTK handle selection
                }
                Event::Push
                    // Double click - open docs
                    if fltk::app::event_clicks() => {
                        let idx = b.value();
                        if idx > 0 {
                            sender.send(Message::DiagnosticOpenDocs(idx as u32));
                        }
                        true
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
}

impl Themeable for DiagnosticPanel {
    fn apply_theme(&mut self, theme: PanelTheme) {
        // The list follows the theme (T0104): the editor background, and the
        // level colors re-derived to stay readable on it.
        self.set_panel_bg(diagnostic_panel_bg(theme.bg));
        if !self.diagnostics.is_empty() {
            self.refresh_display();
        }
    }
}

/// The diagnostic list's background for a theme whose editor background is
/// `editor_bg`: the editor's own, like the file tree (T0104).
pub fn diagnostic_panel_bg(editor_bg: (u8, u8, u8)) -> (u8, u8, u8) {
    editor_bg
}

/// The hue of each diagnostic level (error, warning, info, hint): FLTK's
/// palette colors the list used before (indexes 88, 94, 12 and 8).
pub const DIAGNOSTIC_LEVEL_BASES: [(u8, u8, u8); 4] =
    [(255, 0, 0), (255, 218, 0), (113, 113, 198), (85, 85, 85)];

/// Minimum WCAG contrast of a diagnostic line on the list.
const DIAGNOSTIC_TEXT_CONTRAST: f64 = 4.5;

/// The text color of each diagnostic level on the list background `bg`: its
/// hue, made readable (>= 4.5:1) on `bg` by changing only its lightness
/// (T0104).
pub fn diagnostic_level_colors(bg: (u8, u8, u8)) -> [(u8, u8, u8); 4] {
    use crate::ui::color::{ThemeRgb, push_to_contrast};
    DIAGNOSTIC_LEVEL_BASES.map(|base| {
        let bg = ThemeRgb::from_tuple(bg);
        let c = push_to_contrast(
            ThemeRgb::from_tuple(base),
            bg,
            DIAGNOSTIC_TEXT_CONTRAST,
            bg.brightness() < 128,
        );
        (c.r, c.g, c.b)
    })
}

/// The header bar's fills, per state (clean, error, warning, info): the
/// pre-T0104 colors, kept on every theme by the user's choice. The bar's
/// information is its label, which stays readable; its contrast against the
/// panel is decorative.
pub const DIAGNOSTIC_HEADER_BASES: [(u8, u8, u8); 4] =
    [(46, 125, 50), (198, 40, 40), (245, 124, 0), (25, 118, 210)];

/// WCAG AA contrast for the header's label.
const DIAGNOSTIC_HEADER_TEXT_CONTRAST: f64 = 4.5;

/// What the header bar summarizes: the most severe level present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderState {
    /// No diagnostics.
    Clean,
    /// At least one error.
    Error,
    /// Warnings, no error.
    Warning,
    /// Only info or hint lines.
    Info,
}

impl HeaderState {
    /// Every state, for tests and screenshots.
    pub const ALL: [Self; 4] = [Self::Clean, Self::Error, Self::Warning, Self::Info];

    /// The state for a list of diagnostics.
    pub fn of(diagnostics: &[Diagnostic]) -> Self {
        let has = |level: DiagnosticLevel| diagnostics.iter().any(|d| d.level == level);
        if diagnostics.is_empty() {
            Self::Clean
        } else if has(DiagnosticLevel::Error) {
            Self::Error
        } else if has(DiagnosticLevel::Warning) {
            Self::Warning
        } else {
            Self::Info
        }
    }

    /// The state's header fill.
    pub fn base(self) -> (u8, u8, u8) {
        match self {
            Self::Clean => DIAGNOSTIC_HEADER_BASES[0],
            Self::Error => DIAGNOSTIC_HEADER_BASES[1],
            Self::Warning => DIAGNOSTIC_HEADER_BASES[2],
            Self::Info => DIAGNOSTIC_HEADER_BASES[3],
        }
    }
}

/// The header bar's fill and label color for `state` (T0104): the state's
/// fill, with the old white label wherever white reads (>= 4.5:1), and
/// otherwise black or white, whichever reads better (the orange bar).
pub fn diagnostic_header_colors(state: HeaderState) -> ((u8, u8, u8), Color) {
    use crate::ui::color::{ThemeRgb, wcag_contrast};
    let fill = state.base();
    let text = if wcag_contrast(ThemeRgb::WHITE.to_tuple(), fill) >= DIAGNOSTIC_HEADER_TEXT_CONTRAST
    {
        Color::White
    } else {
        best_black_or_white(fill)
    };
    (fill, text)
}

/// FLTK's `@C<n>` browser format code for an RGB color (`0xRRGGBB00`).
/// Pure black would pack to 0, which FLTK reads as palette index 0 (the
/// foreground color), so it is written as `FL_BLACK` (index 56) instead.
fn color_code((r, g, b): (u8, u8, u8)) -> String {
    if (r, g, b) == (0, 0, 0) {
        return format!("@C{}", Color::Black.bits());
    }
    format!(
        "@C{}",
        u32::from(r) << 24 | u32::from(g) << 16 | u32::from(b) << 8
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::color::wcag_contrast;

    /// Hue in degrees, or `None` for a gray.
    fn hue((r, g, b): (u8, u8, u8)) -> Option<f64> {
        let (r, g, b) = (f64::from(r), f64::from(g), f64::from(b));
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let d = max - min;
        if d < 8.0 {
            return None;
        }
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        Some(h * 60.0)
    }

    /// T0104: the diagnostic list follows the theme (its background is the
    /// editor's, never a fixed gray), and every level color stays readable on
    /// it (>= 4.5:1). On the built-in themes it keeps its hue. On a mid-tone
    /// custom background readability wins: 4.5:1 is only reachable near
    /// white or black, where the hue fades to gray, but it never turns into
    /// another hue.
    #[test]
    fn diagnostic_panel_follows_the_theme() {
        use crate::app::domain::settings::SyntaxTheme;
        let reference = DIAGNOSTIC_LEVEL_BASES;
        let mut failures = Vec::new();
        for (theme, editor_bg) in crate::ui::color::theme_test_backgrounds() {
            let built_in = SyntaxTheme::all()
                .iter()
                .any(|t| t.background() == editor_bg);
            let bg = diagnostic_panel_bg(editor_bg);
            if bg != editor_bg {
                failures.push(format!(
                    "{theme}: panel bg {bg:?} != editor bg {editor_bg:?}"
                ));
            }
            for (i, color) in diagnostic_level_colors(bg).into_iter().enumerate() {
                let ratio = wcag_contrast(color, bg);
                let same_hue = match (hue(color), hue(reference[i])) {
                    (Some(a), Some(b)) => {
                        let d = (a - b).abs();
                        d.min(360.0 - d) <= 12.0
                    }
                    (None, None) => true,
                    (None, Some(_)) => !built_in,
                    (Some(_), None) => false,
                };
                if ratio < 4.5 || !same_hue {
                    failures.push(format!(
                        "{theme}: level {i} {color:?} on {bg:?} = {ratio:.2}:1, hue kept: {same_hue}"
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    /// T0104, by the user's choice: the header bar keeps its pre-T0104 fills
    /// on every theme (its label carries the information; the band's own
    /// contrast against the panel is decorative). Its text is the old white
    /// wherever white reads (>= 4.5:1), and black otherwise (the orange bar,
    /// where white was 2.70:1).
    #[test]
    fn diagnostic_header_keeps_the_old_fills_with_readable_text() {
        // The function takes no background, so the bar is the same on every
        // theme by construction.
        let expected = [
            (HeaderState::Clean, (46, 125, 50), (255, 255, 255)),
            (HeaderState::Error, (198, 40, 40), (255, 255, 255)),
            (HeaderState::Warning, (245, 124, 0), (0, 0, 0)),
            (HeaderState::Info, (25, 118, 210), (255, 255, 255)),
        ];
        for (state, old_fill, old_or_readable_text) in expected {
            let (fill, text) = diagnostic_header_colors(state);
            let text = text.to_rgb();
            assert_eq!(fill, old_fill, "{state:?}: the pre-T0104 fill");
            assert_eq!(text, old_or_readable_text, "{state:?}: label color");
            let ratio = wcag_contrast(text, fill);
            assert!(ratio >= 4.5, "{state:?}: label {ratio:.2}:1 on the bar");
        }
    }

    /// Black must not become `@C0`, FLTK's foreground index.
    #[test]
    fn color_code_never_writes_the_foreground_index() {
        assert_eq!(color_code((0, 0, 0)), "@C56");
        assert_eq!(color_code((255, 0, 0)), format!("@C{}", 0xFF00_0000u32));
        assert_eq!(color_code((0, 0, 1)), "@C256");
    }

    /// T0104: every color of the panel is derived from the theme; none is
    /// written into the widget code by hand.
    #[test]
    fn diagnostic_panel_sets_no_absolute_colors() {
        let src = include_str!("diagnostic_panel.rs");
        let code = src.split("#[cfg(test)]\nmod tests").next().expect("source");
        let literal = regex_lite::Regex::new(
            r"(from_rgb|fltk_rgb)\(\s*\d|set_\w*color\(\s*Color::(White|Black|Red|Green|Blue|Yellow|Dark\w*|Light\w*|Gray\w*)\b",
        )
        .expect("regex");
        let hits: Vec<&str> = code
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter(|l| literal.is_match(l))
            .collect();
        assert!(hits.is_empty(), "absolute colors:\n{}", hits.join("\n"));
    }
}
