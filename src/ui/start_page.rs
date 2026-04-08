//! Start Page — landing page shown when no tabs are open.
//!
//! Two-column layout below a centered title:
//! - Left: 2x2 button grid + stacked session/files cards
//! - Right: donate button + changelog/sponsor card (aligned with left cards)

use std::collections::HashSet;
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
use crate::app::services::session::{self, SessionRestore};
use crate::ui::dialogs::DialogTheme;

const BLOG_POSTS: &[(&str, &str)] = &[
    // ("What's new in 0.9.5", "https://ferrispad.com/blog/0.9.5"),
];
const DONATE_URL: &str = "https://buymeacoffee.com/ferrispad";

const BTN_W: i32 = 120;
const BTN_GAP: i32 = 10;
const BTN_H: i32 = 30;
const MAX_RECENT: usize = 4;
/// Left column width: 2 buttons + gap.
const LEFT_W: i32 = BTN_W * 2 + BTN_GAP;
/// Right column width: same as left for balance.
const RIGHT_W: i32 = LEFT_W;

pub struct StartPage {
    flex: Flex,
    is_visible: bool,
    last_theme_bg: Option<(u8, u8, u8)>,
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
    }

    pub fn show(
        &mut self,
        sender: Sender<Message>,
        theme_bg: (u8, u8, u8),
        active_session: &str,
    ) {
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

        spacer(bg); // top centering

        // ── Title (centered, full width) ──
        let mut title_col = Flex::default().column();
        title_col.set_pad(2);
        title_col.set_color(bg);
        let t = clabel("FerrisPad", 28, Font::HelveticaBold, theme.text, bg);
        title_col.fixed(&t, 36);
        let v = clabel(
            &format!("v{}", env!("CARGO_PKG_VERSION")),
            13,
            Font::Helvetica,
            theme.text_dim,
            bg,
        );
        title_col.fixed(&v, 20);
        title_col.end();
        self.flex.fixed(&title_col, 60);

        fspacer(&mut self.flex, 10, bg);

        // ── Two-column body ──
        // Gather data
        let all_sessions = session::list_sessions();
        let sessions: Vec<&String> = all_sessions.iter().take(MAX_RECENT).collect();
        let recent = gather_recent_files();
        let max_card_items = sessions.len().max(recent.len()).max(1) as i32;
        let card_items_h = 24 + 1 + max_card_items * 22 + (2 + max_card_items - 1) * 2 + 16;

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
        sess_box.fixed(&h, 24);
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
            sess_box.fixed(&btn, 22);
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
        files_box.fixed(&h, 24);
        let sep = separator(border_color);
        files_box.fixed(&sep, 1);
        for (display, path) in &recent {
            let short = shorten(path);
            let label = format!("{} {}", display, short);
            let mut btn = hoverable_item(&label, path, &theme, card_bg, false);
            files_box.fixed(&btn, 22);
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
        body.fixed(&right, RIGHT_W);

        // Donate button — height = 2 button rows + gap
        let donate_h = BTN_H * 2 + BTN_GAP;
        let mut dbtn = Button::default();
        dbtn.set_label("Support FerrisPad");
        dbtn.set_frame(FrameType::RFlatBox);
        dbtn.set_color(theme.button_bg);
        dbtn.set_label_color(theme.text);
        dbtn.set_label_size(14);
        right.fixed(&dbtn, donate_h);
        let normal_bg = theme.button_bg;
        let hover_bg = theme.tab_active_bg;
        let normal_fg = theme.text;
        dbtn.handle(move |btn, ev| match ev {
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
        dbtn.set_callback(|_| {
            let _ = open::that(DONATE_URL);
        });

        fspacer(&mut right, 4, bg);

        // Changelog / sponsor card — flexible, fills remaining right column to align with left cards
        let mut sponsor_box = card(&theme, card_bg, border_color);
        let h = box_header("Changelog", &theme, card_bg);
        sponsor_box.fixed(&h, 24);
        let sep = separator(border_color);
        sponsor_box.fixed(&sep, 1);

        #[allow(clippy::const_is_empty)]
        if !BLOG_POSTS.is_empty() {
            for &(text, url) in BLOG_POSTS {
                let mut btn = hoverable_item(text, url, &theme, card_bg, false);
                sponsor_box.fixed(&btn, 22);
                let u = url.to_string();
                btn.set_callback(move |_| {
                    let _ = open::that(&u);
                });
            }
        } else {
            let mut ph = Frame::default();
            ph.set_label("Coming soon...");
            ph.set_label_size(11);
            ph.set_label_color(theme.text_dim);
            ph.set_frame(FrameType::FlatBox);
            ph.set_color(card_bg);
            sponsor_box.fixed(&ph, 22);
        }

        spacer(card_bg);
        sponsor_box.end();
        add_rounded_border(&mut sponsor_box, card_bg, border_color);
        // Match from top of sessions card to bottom of files card:
        // sess_card + pad + fspacer(4) + pad + files_card
        let changelog_h = card_items_h * 2 + BTN_GAP + 4 + BTN_GAP;
        right.fixed(&sponsor_box, changelog_h);

        right.end();

        spacer(bg); // right centering

        body.end();

        // Body height: buttons(2 rows) + gap + 2 cards + gaps
        let body_h = BTN_H * 2 + BTN_GAP + 4 + card_items_h * 2 + 4 + left.pad() * 5;
        self.flex.fixed(&body, body_h);

        // Ensure window is tall enough for the content
        // title(60) + gap(10) + body + margins for menu/status bar (~80)
        let min_h = 60 + 10 + body_h + 80;
        if let Some(mut win) = self.flex.top_window()
            && win.h() < min_h
        {
            win.set_size(win.w(), min_h);
        }

        spacer(bg); // bottom centering

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
    f.set_label_size(13);
    f.set_label_font(Font::HelveticaBold);
    f.set_label_color(theme.text);
    f.set_frame(FrameType::FlatBox);
    f.set_color(bg);
    f
}

fn action_button(label: &str, theme: &DialogTheme) -> Button {
    let mut btn = Button::default();
    btn.set_label(label);
    btn.set_frame(FrameType::RFlatBox);
    btn.set_color(theme.button_bg);
    btn.set_label_color(theme.text);
    btn.set_label_size(12);
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
    btn.set_label_size(12);
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
