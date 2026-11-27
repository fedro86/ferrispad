//! Start Page — landing page shown when no tabs are open.
//!
//! Two-column layout below a centered title:
//! - Left: 2x2 button grid + stacked session/files cards
//! - Right: donate button + changelog/sponsor card (aligned with left cards)

use crate::ui::color::fltk_rgb;
use std::path::Path;

use fltk::{
    app::Sender,
    button::Button,
    draw,
    enums::{Align, Color, Event, Font, FrameType},
    frame::Frame,
    group::Flex,
    prelude::*,
};

use crate::app::domain::messages::Message;
use crate::app::services::recent_files;
use crate::app::services::session;
use crate::ui::dialogs::DialogTheme;

const WIKI_FERRISPAD: &str = "https://github.com/fedro86/ferrispad/wiki";
const WIKI_PLUGINS: &str = "https://github.com/fedro86/ferrispad-plugins/wiki";
const CHANGELOG_URL: &str = "https://github.com/fedro86/ferrispad/blob/master/CHANGELOG.md";

/// Recent changelog entries: (description, version_tag).
const CHANGELOG_ENTRIES: &[&str] = &[
    "Plugin sandbox hardening",
    "No stalls on long lines",
    "Custom themes & theme manager",
    "Current-line highlight",
    "Window size remembered",
    "Blue, readable selection",
];

const BTN_W: i32 = 150;
const BTN_GAP: i32 = 12;
const BTN_H: i32 = 38;
const MAX_RECENT: usize = 4;
/// Left column width: 2 buttons + gap.
const LEFT_W: i32 = BTN_W * 2 + BTN_GAP;
/// Right column width: same as left for balance.
const RIGHT_W: i32 = LEFT_W;

pub struct StartPage {
    flex: Flex,
    is_visible: bool,
    last_theme_bg: Option<(u8, u8, u8)>,
    /// The recent-files list the page was last drawn with (T0116).
    last_recent: Vec<String>,
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
            last_recent: Vec::new(),
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

    /// The recent-files list the page was last drawn with.
    pub fn last_recent(&self) -> &[String] {
        &self.last_recent
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
        self.flex.hide();
        self.flex.clear();
    }

    pub fn show(
        &mut self,
        sender: Sender<Message>,
        theme_bg: (u8, u8, u8),
        active_session: &str,
        recent_list: &[String],
    ) {
        let theme = DialogTheme::from_theme_bg(theme_bg);
        let bg = fltk_rgb(theme_bg.0, theme_bg.1, theme_bg.2);
        let card_bg = theme.row_bg;
        let border_color = theme.text_dim;

        self.flex.clear();
        self.flex.set_color(bg);
        self.flex.set_frame(FrameType::FlatBox);
        self.flex.set_pad(0);
        self.flex.set_margin(0);
        self.flex.begin();

        spacer(bg); // top centering

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
        self.flex.fixed(&title_col, 75);

        fspacer(&mut self.flex, 10, bg);

        // ── Two-column body ──
        // Gather data
        let all_sessions = session::list_sessions();
        let sessions: Vec<&String> = all_sessions.iter().take(MAX_RECENT).collect();
        // The same list as File → Open Recent (T0116).
        let recent = recent_files::start_page_entries(recent_list);
        self.last_recent = recent_list.to_vec();
        let max_card_items = sessions.len().max(recent.len()).max(1) as i32;
        let card_items_h = left_card_h(max_card_items, CHANGELOG_ENTRIES.len());

        let mut body = Flex::default().row();
        body.set_pad(20);
        body.set_color(bg);

        spacer(bg); // left centering

        // ── LEFT COLUMN ──
        let mut left = Flex::default().column();
        left.set_pad(BTN_GAP);
        left.set_color(bg);
        body.fixed(&left, LEFT_W);

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
            // Like File → Open Recent: a missing file is reported and dropped.
            btn.set_callback(move |_| s.send(Message::OpenRecentFile(p.clone())));
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
        body.fixed(&right, RIGHT_W);

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
        right.fixed(&sponsor_box, changelog_card_h(card_items_h));

        right.end();

        spacer(bg); // right centering

        body.end();

        // Body height: buttons(2 rows) + gap + 2 cards + gaps
        let body_h = BTN_H * 2 + BTN_GAP + 4 + card_items_h * 2 + 4 + left.pad() * 5;
        self.flex.fixed(&body, body_h);

        // Ensure window is large enough for the start page content.
        // Force resize if current size is too small (e.g., after closing a small file tab).
        let min_h = 75 + 10 + body_h + 80;
        let min_w = LEFT_W + RIGHT_W + 20 + 80;
        if let Some(mut win) = self.flex.top_window() {
            let cur_w = win.w();
            let cur_h = win.h();
            if cur_w < min_w || cur_h < min_h {
                let new_w = cur_w.max(min_w);
                let new_h = cur_h.max(min_h);
                // Use pixel_w/pixel_h to handle HiDPI correctly
                win.resize(win.x(), win.y(), new_w, new_h);
            }
        }

        spacer(bg); // bottom centering

        self.flex.end();
        self.flex.show();
        self.is_visible = true;
        self.last_theme_bg = Some(theme_bg);
    }
}

// ── Widget helpers ──

/// Height of a recent-sessions / recent-files card listing `items` rows.
fn recent_card_h(items: i32) -> i32 {
    30 + 1 + items * 28 + (2 + items - 1) * 2 + 20
}

/// Height the changelog card needs to show `entries` rows: the card margins,
/// the header, the separator, one 22-px row per entry, the "more..." link,
/// and the pad between those children and the flexible spacer.
fn changelog_content_h(entries: usize) -> i32 {
    let rows = i32::try_from(entries).unwrap_or(i32::MAX / 64);
    let children = 2 + rows + 2;
    2 * 8 + 30 + 1 + rows * 22 + 25 + (children - 1) * 2
}

/// The changelog card spans both left cards, from the top of the sessions
/// card to the bottom of the files card: sess_card + pad + fspacer(4) + pad
/// + files_card.
fn changelog_card_h(left_card_h: i32) -> i32 {
    left_card_h * 2 + BTN_GAP + 4 + BTN_GAP
}

/// Height of each left card for `items` recent rows. With few rows the cards
/// grow, so the changelog card, which spans both, shows every one of its
/// `changelog_entries` and the columns stay aligned.
fn left_card_h(items: i32, changelog_entries: usize) -> i32 {
    let spanned = changelog_content_h(changelog_entries) - (BTN_GAP + 4 + BTN_GAP);
    recent_card_h(items).max((spanned + 1) / 2)
}

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
    btn.set_label_color(fltk_rgb(255, 255, 255));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// T0114: with few recent items (a fresh install), the changelog card,
    /// which is as tall as the two left cards, clipped its last entry.
    #[test]
    fn changelog_card_shows_every_entry() {
        let needed = changelog_content_h(CHANGELOG_ENTRIES.len());
        for items in 1..=MAX_RECENT as i32 {
            let card = changelog_card_h(left_card_h(items, CHANGELOG_ENTRIES.len()));
            assert!(
                card >= needed,
                "{items} recent item(s): changelog card {card} px < {needed} px needed"
            );
        }
    }

    #[test]
    fn left_cards_keep_their_recent_rows() {
        for items in 1..=MAX_RECENT as i32 {
            assert!(left_card_h(items, CHANGELOG_ENTRIES.len()) >= recent_card_h(items));
        }
    }
}
