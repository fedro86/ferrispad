//! Start Page — landing page shown when no tabs are open.
//!
//! Below a centered title, two columns when the page is wide enough, one
//! column (left block above right block) when it is not (T0108):
//! - Left: 2x2 button grid + stacked session/files cards
//! - Right: wiki buttons + changelog card
//!
//! The content scrolls vertically when the page is too short, and while the
//! page is shown the window cannot shrink below one column.

use std::collections::HashSet;
use std::path::Path;

use fltk::{
    app::Sender,
    button::Button,
    draw,
    enums::{Align, Color, Event, Font, FrameType},
    frame::Frame,
    group::{Flex, Scroll, ScrollType},
    prelude::*,
};

use crate::app::domain::messages::Message;
use crate::app::services::session::{self, SessionRestore};
use crate::ui::dialogs::DialogTheme;

const WIKI_FERRISPAD: &str = "https://github.com/fedro86/ferrispad/wiki";
const WIKI_PLUGINS: &str = "https://github.com/fedro86/ferrispad-plugins/wiki";
const CHANGELOG_URL: &str = "https://github.com/fedro86/ferrispad/blob/master/CHANGELOG.md";

/// Recent changelog entries: (description, version_tag).
const CHANGELOG_ENTRIES: &[&str] = &[
    "Custom themes & theme manager",
    "Current-line highlight",
    "Window size remembered",
    "Blue, readable selection",
    "Save prompt on title-bar close",
    "Updated dependencies, smaller binary",
];

const BTN_W: i32 = 150;
const BTN_GAP: i32 = 12;
const BTN_H: i32 = 38;
const MAX_RECENT: usize = 4;
/// Label size of the page's buttons.
const BTN_LABEL_SIZE: i32 = 15;
/// Room around a button label.
const BTN_LABEL_PAD: i32 = 24;
/// Space between the two columns, and between the content and the page's
/// left and right edges.
const COL_GAP: i32 = 20;
/// Space above and below the content.
const PAGE_MARGIN: i32 = 24;
/// Title block ("FerrisPad" + version) and the gap below it.
const TITLE_H: i32 = 75;
const TITLE_GAP: i32 = 10;
/// The labels of the 2x2 button grid (they share one width).
const GRID_LABELS: [&str; 4] = ["New File", "Open File", "New Session", "Sessions..."];

/// One or two columns of content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Columns {
    One,
    Two,
}

/// Width of one column: two grid buttons and the gap between them.
pub fn column_width(button_w: i32) -> i32 {
    2 * button_w + BTN_GAP
}

/// The narrowest content that shows two columns side by side: the columns,
/// the gap between them and a gap at each edge.
pub fn two_columns_min_width(column_w: i32) -> i32 {
    2 * column_w + 3 * COL_GAP
}

/// The narrowest content that shows one column, with a gap at each edge.
pub fn one_column_min_width(column_w: i32) -> i32 {
    column_w + 2 * COL_GAP
}

/// The layout for content `content_w` px wide (T0108).
pub fn columns_for(content_w: i32, column_w: i32) -> Columns {
    if content_w >= two_columns_min_width(column_w) {
        Columns::Two
    } else {
        Columns::One
    }
}

/// The layout for a page of `page_w` × `page_h`, whose content is
/// `two_h` tall in two columns: two columns only if they fit beside the
/// vertical scrollbar that this height needs, if any.
pub fn page_columns(page_w: i32, page_h: i32, column_w: i32, two_h: i32, bar_w: i32) -> Columns {
    let bar = if two_h > page_h { bar_w } else { 0 };
    columns_for(page_w - bar, column_w)
}

/// The shortest page: the title and the first button rows; the rest
/// scrolls.
pub fn min_page_height() -> i32 {
    PAGE_MARGIN + TITLE_H + TITLE_GAP + 2 * BTN_H + BTN_GAP + PAGE_MARGIN
}

/// Width of a grid button: [`BTN_W`], or wider if a label needs it at the
/// current font metrics (measuring needs the display).
fn grid_button_width() -> i32 {
    let (old_font, old_size) = (draw::font(), draw::size());
    draw::set_font(Font::Helvetica, BTN_LABEL_SIZE);
    let widest = GRID_LABELS
        .iter()
        .map(|l| draw::width(l).ceil() as i32)
        .max()
        .unwrap_or(0);
    draw::set_font(old_font, old_size);
    BTN_W.max(widest + BTN_LABEL_PAD)
}

pub struct StartPage {
    flex: Flex,
    is_visible: bool,
    last_theme_bg: Option<(u8, u8, u8)>,
    /// Minimum page size of the current build (T0108).
    min_page: (i32, i32),
}

impl Default for StartPage {
    fn default() -> Self {
        Self::new()
    }
}

impl StartPage {
    pub fn new() -> Self {
        let mut flex = Flex::default().column();
        flex.end();
        flex.hide();
        Self {
            flex,
            is_visible: false,
            last_theme_bg: None,
            min_page: (0, 0),
        }
    }

    pub fn widget(&self) -> &Flex {
        &self.flex
    }

    pub fn visible(&self) -> bool {
        self.is_visible
    }

    pub fn last_theme_bg(&self) -> Option<(u8, u8, u8)> {
        self.last_theme_bg
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
        self.flex.hide();
        self.flex.clear();
        // The editor has no minimum window size (T0108).
        if let Some(mut win) = self.flex.top_window() {
            win.size_range(1, 1, 0, 0);
        }
    }

    /// While the page is shown, keep the window at least one column wide and
    /// tall enough for the title and the first buttons (T0108): the page's
    /// minimum plus whatever else the window holds around it (menu, status
    /// bar, side panels). Grows the window if it is smaller. Call after the
    /// page has been laid out.
    pub fn enforce_min_window_size(&self) {
        if !self.is_visible || self.flex.w() <= 0 {
            return;
        }
        let Some(mut win) = self.flex.top_window() else {
            return;
        };
        let chrome_w = (win.w() - self.flex.w()).max(0);
        let chrome_h = (win.h() - self.flex.h()).max(0);
        let (min_w, min_h) = (self.min_page.0 + chrome_w, self.min_page.1 + chrome_h);
        win.size_range(min_w, min_h, 0, 0);
        if win.w() < min_w || win.h() < min_h {
            win.resize(win.x(), win.y(), win.w().max(min_w), win.h().max(min_h));
        }
    }

    pub fn show(&mut self, sender: Sender<Message>, theme_bg: (u8, u8, u8), active_session: &str) {
        let theme = DialogTheme::from_theme_bg(theme_bg);
        let bg = Color::from_rgb(theme_bg.0, theme_bg.1, theme_bg.2);
        let card_bg = theme.row_bg;
        let border_color = theme.text_dim;

        self.flex.clear();
        self.flex.set_color(bg);
        self.flex.set_frame(FrameType::FlatBox);
        self.flex.set_pad(0);
        self.flex.set_margin(0);
        self.flex.begin();

        // ── Sizes (T0108) ──
        let button_w = grid_button_width();
        let column_w = column_width(button_w);
        let bar_w = fltk::app::scrollbar_size();
        // A page that was hidden has no size of its own yet: its parent's
        // is the best estimate. If the layout then differs, the resize
        // callback below asks for a rebuild.
        let (page_w, viewport_h) = match self.flex.parent() {
            Some(parent) if self.flex.w() <= 0 || self.flex.h() <= 0 => (parent.w(), parent.h()),
            _ => (self.flex.w(), self.flex.h()),
        };
        // The page can scroll, so its minimum width leaves room for the bar.
        self.min_page = (one_column_min_width(column_w) + bar_w, min_page_height());

        let all_sessions = session::list_sessions();
        let sessions: Vec<&String> = all_sessions.iter().take(MAX_RECENT).collect();
        let recent = gather_recent_files();
        let max_card_items = sessions.len().max(recent.len()).max(1) as i32;
        let card_items_h = 30 + 1 + max_card_items * 28 + (2 + max_card_items - 1) * 2 + 20;
        // Left: 2 button rows, 2 gaps of 4, 2 cards, 5 pads between its 6 rows.
        let left_h = 2 * BTN_H + 2 * 4 + 2 * card_items_h + 5 * BTN_GAP;
        // Changelog: beside the cards, as tall as both; alone, as tall as
        // its own content. Right: 2 button rows, a gap of 4, the changelog,
        // 3 pads.
        let changelog_two = card_items_h * 2 + BTN_GAP + 4 + BTN_GAP;
        let changelog_one = 30 + 1 + CHANGELOG_ENTRIES.len() as i32 * 22 + 25 + 12;
        let right_h = |changelog_h: i32| 2 * BTN_H + 4 + changelog_h + 3 * BTN_GAP;
        let frame_h = PAGE_MARGIN + TITLE_H + TITLE_GAP + PAGE_MARGIN;
        let two_h = frame_h + left_h.max(right_h(changelog_two));
        let one_h = frame_h + left_h + COL_GAP + right_h(changelog_one);

        let columns = page_columns(page_w, viewport_h, column_w, two_h, bar_w);
        let (changelog_h, body_h, content_h) = match columns {
            Columns::Two => (changelog_two, two_h - frame_h, two_h),
            Columns::One => (changelog_one, one_h - frame_h, one_h),
        };

        // ── Scrolling viewport ──
        let mut scroll = Scroll::default();
        scroll.set_type(ScrollType::Vertical);
        scroll.set_color(bg);
        scroll.set_frame(FrameType::FlatBox);
        let mut bar = scroll.scrollbar();
        crate::ui::scrollbar::style_scrollbar(&mut bar, theme.scroll_track, theme.scroll_thumb);
        let scrollbar_w = if content_h > viewport_h { bar_w } else { 0 };
        // Taller than needed when the page is tall: the spacers centre it.
        let mut content = Flex::new(
            self.flex.x(),
            self.flex.y(),
            (page_w - scrollbar_w).max(0),
            content_h.max(viewport_h),
            None,
        )
        .column();
        content.set_color(bg);
        content.set_frame(FrameType::FlatBox);
        content.set_pad(0);

        spacer(bg); // top centering
        fspacer(&mut content, PAGE_MARGIN, bg);

        // ── Title (centered, full width) ──
        let mut title_col = Flex::default().column();
        title_col.set_pad(2);
        title_col.set_color(bg);
        let t = clabel("FerrisPad", 35, Font::HelveticaBold, theme.text, bg);
        title_col.fixed(&t, 45);
        let v = clabel(
            &format!("v{}", env!("CARGO_PKG_VERSION")),
            16,
            Font::Helvetica,
            theme.text_dim,
            bg,
        );
        title_col.fixed(&v, 25);
        title_col.end();
        content.fixed(&title_col, TITLE_H);

        fspacer(&mut content, TITLE_GAP, bg);

        // ── Body: one row of two columns, or one centred column ──
        let mut body = Flex::default().row();
        body.set_pad(COL_GAP);
        body.set_color(bg);
        spacer(bg); // left centering
        // One column: the left block above the right block, in `stack`.
        let mut stack = (columns == Columns::One).then(|| {
            let mut stack = Flex::default().column();
            stack.set_pad(COL_GAP);
            stack.set_color(bg);
            stack
        });

        // ── LEFT COLUMN ──
        let mut left = Flex::default().column();
        left.set_pad(BTN_GAP);
        left.set_color(bg);

        // Button row 1: [New File] [Open File]
        let mut row1 = Flex::default().row();
        row1.set_pad(BTN_GAP);
        row1.set_color(bg);
        let mut b1 = action_button("New File", &theme);
        let s = sender;
        b1.set_callback(move |_| s.send(Message::FileNew));
        let mut b2 = action_button("Open File", &theme);
        let s = sender;
        b2.set_callback(move |_| s.send(Message::FileOpen));
        row1.end();
        left.fixed(&row1, BTN_H);

        // Button row 2: [New Session] [Sessions...]
        let mut row2 = Flex::default().row();
        row2.set_pad(BTN_GAP);
        row2.set_color(bg);
        let mut b3 = action_button("New Session", &theme);
        let s = sender;
        b3.set_callback(move |_| s.send(Message::SessionNewWindow));
        let mut b4 = action_button("Sessions...", &theme);
        let s = sender;
        b4.set_callback(move |_| s.send(Message::SessionShowPicker));
        row2.end();
        left.fixed(&row2, BTN_H);

        fspacer(&mut left, 4, bg);

        // Sessions card
        let mut sess_box = card(&theme, card_bg, border_color);
        let h = box_header("Recent Sessions", &theme, card_bg);
        sess_box.fixed(&h, 30);
        let sep = separator(border_color);
        sess_box.fixed(&sep, 1);
        for name in &sessions {
            let active = *name == active_session;
            let txt = if active {
                format!("{} (active)", name)
            } else {
                (*name).clone()
            };
            let mut btn = hoverable_item(&txt, name, &theme, card_bg, active);
            sess_box.fixed(&btn, 28);
            if !active {
                let s = sender;
                let n = (*name).clone();
                btn.set_callback(move |_| s.send(Message::SessionSwitchTo(n.clone())));
            }
        }
        spacer(card_bg);
        sess_box.end();
        add_rounded_border(&mut sess_box, card_bg, border_color);
        left.fixed(&sess_box, card_items_h);

        fspacer(&mut left, 4, bg);

        // Files card
        let mut files_box = card(&theme, card_bg, border_color);
        let h = box_header("Recent Files", &theme, card_bg);
        files_box.fixed(&h, 30);
        let sep = separator(border_color);
        files_box.fixed(&sep, 1);
        for (display, path) in &recent {
            let short = shorten(path);
            let label = format!("{} {}", display, short);
            let mut btn = hoverable_item(&label, path, &theme, card_bg, false);
            files_box.fixed(&btn, 28);
            let s = sender;
            let p = path.clone();
            btn.set_callback(move |_| {
                if Path::new(&p).exists() {
                    s.send(Message::DeferredOpenFile(p.clone()));
                }
            });
        }
        spacer(card_bg);
        files_box.end();
        add_rounded_border(&mut files_box, card_bg, border_color);
        left.fixed(&files_box, card_items_h);

        left.end();

        // ── RIGHT COLUMN ──
        let mut right = Flex::default().column();
        right.set_pad(BTN_GAP);
        right.set_color(bg);

        // Donate button — height = 2 button rows + gap
        // Wiki buttons: accent-colored, 2 rows matching the left buttons
        let mut wiki_row1 = Flex::default().row();
        wiki_row1.set_color(bg);
        let mut wb1 = accent_button("Wiki FerrisPad", &theme);
        wb1.set_callback(|_| {
            let _ = open::that(WIKI_FERRISPAD);
        });
        wiki_row1.end();
        right.fixed(&wiki_row1, BTN_H);

        let mut wiki_row2 = Flex::default().row();
        wiki_row2.set_color(bg);
        let mut wb2 = accent_button("Wiki Plugins", &theme);
        wb2.set_callback(|_| {
            let _ = open::that(WIKI_PLUGINS);
        });
        wiki_row2.end();
        right.fixed(&wiki_row2, BTN_H);

        fspacer(&mut right, 4, bg);

        // Changelog / sponsor card — flexible, fills remaining right column to align with left cards
        let mut sponsor_box = card(&theme, card_bg, border_color);
        let h = box_header("Changelog", &theme, card_bg);
        sponsor_box.fixed(&h, 30);
        let sep = separator(border_color);
        sponsor_box.fixed(&sep, 1);

        for &entry in CHANGELOG_ENTRIES {
            let mut item = Frame::default();
            item.set_label(&format!("  \u{2022} {}", entry));
            item.set_label_size(14);
            item.set_label_color(theme.text);
            item.set_align(Align::Left | Align::Inside | Align::Clip);
            item.set_frame(FrameType::FlatBox);
            item.set_color(card_bg);
            item.set_tooltip(entry);
            sponsor_box.fixed(&item, 22);
        }

        spacer(card_bg);

        // "more..." link at the bottom
        let mut more_btn = hoverable_item("more...", CHANGELOG_URL, &theme, card_bg, true);
        more_btn.set_align(Align::Right | Align::Inside);
        sponsor_box.fixed(&more_btn, 25);
        more_btn.set_callback(|_| {
            let _ = open::that(CHANGELOG_URL);
        });

        sponsor_box.end();
        add_rounded_border(&mut sponsor_box, card_bg, border_color);
        right.fixed(&sponsor_box, changelog_h);

        right.end();

        match stack.as_mut() {
            None => {
                body.fixed(&left, column_w);
                body.fixed(&right, column_w);
            }
            Some(stack) => {
                stack.fixed(&left, left_h);
                stack.fixed(&right, right_h(changelog_h));
                stack.end();
                body.fixed(stack, column_w);
            }
        }

        spacer(bg); // right centering
        body.end();
        content.fixed(&body, body_h);

        fspacer(&mut content, PAGE_MARGIN, bg);
        spacer(bg); // bottom centering
        content.end();
        scroll.end();

        // Follow the page's size: within one layout, the content takes the
        // new width (and at least the new height); crossing the column
        // threshold rebuilds the page (T0108).
        let mut content_follow = content.clone();
        // One request per build: a window drag across the threshold fires a
        // resize per motion event, and each request rebuilds the page (and
        // re-reads the session and recent-file lists). The rebuild installs
        // a new callback with a fresh flag.
        let relayout_requested = std::cell::Cell::new(false);
        scroll.resize_callback(move |scroll, x, y, w, h| {
            if page_columns(w, h, column_w, two_h, bar_w) != columns
                && !relayout_requested.replace(true)
            {
                sender.send(Message::StartPageRelayout);
            }
            let bar = if content_h > h { bar_w } else { 0 };
            content_follow.resize(
                x,
                y - scroll.yposition(),
                (w - bar).max(0),
                content_h.max(h),
            );
        });

        self.flex.end();
        self.flex.show();
        self.is_visible = true;
        self.last_theme_bg = Some(theme_bg);
    }
}

// ── Widget helpers ──

fn card(_theme: &DialogTheme, card_bg: Color, _border: Color) -> Flex {
    let mut f = Flex::default().column();
    f.set_pad(2);
    f.set_margin(8);
    f.set_frame(FrameType::FlatBox);
    f.set_color(card_bg);
    f
}

fn add_rounded_border(widget: &mut Flex, fill: Color, border: Color) {
    let radius = fltk::app::frame_border_radius_max();
    widget.set_frame(FrameType::NoBox);
    widget.draw(move |w| {
        let x = w.x();
        let y = w.y();
        let ww = w.w();
        let hh = w.h();
        draw::set_draw_color(fill);
        draw::draw_rounded_rectf(x, y, ww, hh, radius);
        draw::set_draw_color(border);
        draw::draw_rounded_rect(x, y, ww, hh, radius);
        w.draw_children();
    });
}

fn spacer(bg: Color) {
    let mut f = Frame::default();
    f.set_frame(FrameType::FlatBox);
    f.set_color(bg);
}

fn fspacer(parent: &mut Flex, h: i32, bg: Color) {
    let mut f = Frame::default();
    f.set_frame(FrameType::FlatBox);
    f.set_color(bg);
    parent.fixed(&f, h);
}

fn separator(color: Color) -> Frame {
    let mut f = Frame::default();
    f.set_frame(FrameType::FlatBox);
    f.set_color(color);
    f
}

fn clabel(text: &str, size: i32, font: Font, color: Color, bg: Color) -> Frame {
    let mut f = Frame::default();
    f.set_label(text);
    f.set_label_size(size);
    f.set_label_font(font);
    f.set_label_color(color);
    f.set_frame(FrameType::FlatBox);
    f.set_color(bg);
    f
}

fn box_header(text: &str, theme: &DialogTheme, bg: Color) -> Frame {
    let mut f = Frame::default();
    f.set_label(text);
    f.set_label_size(16);
    f.set_label_font(Font::HelveticaBold);
    f.set_label_color(theme.text);
    f.set_frame(FrameType::FlatBox);
    f.set_color(bg);
    f
}

/// Button with accent color background and white/dark text.
fn accent_button(label: &str, theme: &DialogTheme) -> Button {
    let mut btn = Button::default();
    btn.set_label(label);
    btn.set_frame(FrameType::RFlatBox);
    btn.set_color(theme.accent);
    // White text on accent for readability
    btn.set_label_color(Color::from_rgb(255, 255, 255));
    btn.set_label_size(15);
    let normal = theme.accent;
    let hover = theme.accent_hover;
    btn.handle(move |btn, ev| match ev {
        Event::Enter => {
            btn.set_color(hover);
            btn.redraw();
            true
        }
        Event::Leave => {
            btn.set_color(normal);
            btn.redraw();
            true
        }
        _ => false,
    });
    btn
}

fn action_button(label: &str, theme: &DialogTheme) -> Button {
    let mut btn = Button::default();
    btn.set_label(label);
    btn.set_frame(FrameType::RFlatBox);
    btn.set_color(theme.button_bg);
    btn.set_label_color(theme.text);
    btn.set_label_size(15);
    let normal_bg = theme.button_bg;
    let normal_fg = theme.text;
    let hover_bg = theme.tab_active_bg;
    btn.handle(move |btn, ev| match ev {
        Event::Enter => {
            btn.set_color(hover_bg);
            btn.redraw();
            true
        }
        Event::Leave => {
            btn.set_color(normal_bg);
            btn.set_label_color(normal_fg);
            btn.redraw();
            true
        }
        _ => false,
    });
    btn
}

fn hoverable_item(
    label: &str,
    full_text: &str,
    theme: &DialogTheme,
    bg: Color,
    dimmed: bool,
) -> Button {
    let mut btn = Button::default();
    btn.set_label(label);
    btn.set_frame(FrameType::FlatBox);
    btn.set_color(bg);
    let normal_color = if dimmed { theme.text_dim } else { theme.text };
    btn.set_label_color(normal_color);
    btn.set_label_size(14);
    btn.set_align(Align::Left | Align::Inside | Align::Clip);
    btn.set_tooltip(full_text);
    let hover_color = theme.text;
    btn.handle(move |btn, ev| match ev {
        Event::Enter => {
            btn.set_label_font(Font::HelveticaBold);
            btn.set_label_color(hover_color);
            btn.parent().unwrap().redraw();
            true
        }
        Event::Leave => {
            btn.set_label_font(Font::Helvetica);
            btn.set_label_color(normal_color);
            btn.parent().unwrap().redraw();
            true
        }
        _ => false,
    });
    btn
}

fn shorten(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| {
            let s = p.to_string_lossy();
            if let Some(home) = dirs::home_dir() {
                let h = home.to_string_lossy();
                if s.starts_with(h.as_ref()) {
                    return format!("~{}", &s[h.len()..]);
                }
            }
            s.to_string()
        })
        .unwrap_or_default()
}

fn gather_recent_files() -> Vec<(String, String)> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for name in session::list_sessions() {
        if let Some(data) = session::load_session(SessionRestore::SavedFiles, &name) {
            for doc in &data.documents {
                if let Some(ref p) = doc.file_path
                    && seen.insert(p.clone())
                {
                    let display = Path::new(p)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| p.clone());
                    files.push((display, p.clone()));
                }
            }
        }
    }
    files.truncate(MAX_RECENT);
    files
}

#[cfg(test)]
mod start_page_layout_tests {
    use super::*;

    const COL: i32 = 2 * BTN_W + BTN_GAP;

    /// T0108: one column below the two-column width, two at and above it.
    #[test]
    fn start_page_layout_columns_at_the_threshold() {
        assert_eq!(column_width(BTN_W), COL);
        let two = two_columns_min_width(COL);
        assert_eq!(two, 2 * COL + 3 * COL_GAP);
        assert_eq!(columns_for(two, COL), Columns::Two);
        assert_eq!(columns_for(two - 1, COL), Columns::One);
        assert_eq!(columns_for(1100, COL), Columns::Two);
        assert_eq!(columns_for(500, COL), Columns::One);
    }

    /// The vertical scrollbar a short page needs is taken from the width
    /// before deciding.
    #[test]
    fn start_page_layout_scrollbar_counts() {
        let two = two_columns_min_width(COL);
        // Tall enough: no bar, two columns at exactly the threshold.
        assert_eq!(page_columns(two, 900, COL, 600, 16), Columns::Two);
        // Too short for the two-column content: the bar leaves too little.
        assert_eq!(page_columns(two, 400, COL, 600, 16), Columns::One);
        assert_eq!(page_columns(two + 16, 400, COL, 600, 16), Columns::Two);
    }

    /// The minimum page follows the constants: one column with its edge
    /// gaps, the title and the first button rows.
    #[test]
    fn start_page_layout_minimum_size() {
        assert_eq!(one_column_min_width(COL), COL + 2 * COL_GAP);
        assert!(one_column_min_width(COL) < two_columns_min_width(COL));
        assert_eq!(
            min_page_height(),
            2 * PAGE_MARGIN + TITLE_H + TITLE_GAP + 2 * BTN_H + BTN_GAP
        );
    }
}
