//! Start Page — landing page shown when no tabs are open.
//!
//! Layout: column Flex inside editor_col. Uses nested Flex for responsive sizing.
//! See docs/temp/lesson_learned/fltk-flex-layout.md for patterns.

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

const BTN_W: i32 = 115;
const BTN_GAP: i32 = 10;
const BOX_W: i32 = BTN_W * 2 + BTN_GAP;
/// Max recent sessions shown on start page.
const MAX_RECENT_SESSIONS: usize = 3;

pub struct StartPage {
    flex: Flex,
    is_visible: bool,
    /// Last theme_bg used for rendering — used to detect theme changes.
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

    /// Returns the theme_bg the start page was last rendered with.
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

        self.flex.clear();
        self.flex.set_color(bg);
        self.flex.set_frame(FrameType::FlatBox);
        self.flex.set_pad(0);
        self.flex.set_margin(0);
        self.flex.begin();

        // Vertical centering: [spacer] [content] [spacer]
        spacer(bg);

        let mut content = Flex::default().column();
        content.set_pad(12);
        content.set_color(bg);
        content.set_frame(FrameType::FlatBox);
        content.begin();

        // ── Title ──
        let t = clabel("FerrisPad", 28, Font::HelveticaBold, theme.text, bg);
        content.fixed(&t, 36);
        let v = clabel(
            &format!("v{}", env!("CARGO_PKG_VERSION")),
            13,
            Font::Helvetica,
            theme.text_dim,
            bg,
        );
        content.fixed(&v, 22);

        fspacer(&mut content, 8, bg);

        // ── Buttons row: [spacer] btn btn btn btn [spacer] ──
        let mut btn_row = Flex::default().row();
        btn_row.set_pad(BTN_GAP);
        btn_row.set_color(bg);
        spacer(bg);
        for (label, msg) in [
            ("New File", Message::FileNew),
            ("Open File", Message::FileOpen),
            ("New Session", Message::SessionNewWindow),
            ("Sessions...", Message::SessionShowPicker),
        ] {
            let mut btn = action_button(label, &theme);
            btn_row.fixed(&btn, BTN_W);
            let s = sender;
            btn.set_callback(move |_| s.send(msg.clone()));
        }
        spacer(bg);
        btn_row.end();
        content.fixed(&btn_row, 32);

        fspacer(&mut content, 12, bg);

        // ── Two boxes row: [spacer] [sessions box] [gap] [files box] [spacer] ──
        let all_sessions = session::list_sessions();
        // Show only 3 most recent sessions
        let sessions: Vec<&String> = all_sessions.iter().take(MAX_RECENT_SESSIONS).collect();
        let recent = gather_recent_files();
        let max_items = sessions.len().max(recent.len()).max(1) as i32;
        let box_inner_h = 28 + max_items * 24 + 8;

        let mut boxes_row = Flex::default().row();
        boxes_row.set_pad(BTN_GAP);
        boxes_row.set_color(bg);

        spacer(bg);

        let card_bg = theme.row_bg;
        let border_color = theme.text_dim;

        // Sessions box
        let mut sess_box = Flex::default().column();
        sess_box.set_pad(2);
        sess_box.set_margin(8);
        sess_box.set_frame(FrameType::FlatBox);
        sess_box.set_color(card_bg);

        let h = box_header("Recent Sessions", &theme, card_bg);
        sess_box.fixed(&h, 24);
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
        // Draw rounded border
        add_rounded_border(&mut sess_box, card_bg, border_color);
        boxes_row.fixed(&sess_box, BOX_W);

        // Recent Files box
        let mut files_box = Flex::default().column();
        files_box.set_pad(2);
        files_box.set_margin(8);
        files_box.set_frame(FrameType::FlatBox);
        files_box.set_color(card_bg);

        let h = box_header("Recent Files", &theme, card_bg);
        files_box.fixed(&h, 24);
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
        boxes_row.fixed(&files_box, BOX_W);

        spacer(bg);

        boxes_row.end();
        content.fixed(&boxes_row, box_inner_h);

        // ── Blog ──
        #[allow(clippy::const_is_empty)]
        if !BLOG_POSTS.is_empty() {
            fspacer(&mut content, 8, bg);
            let h = box_header("From the Blog", &theme, bg);
            content.fixed(&h, 24);
            for &(text, url) in BLOG_POSTS {
                let mut btn = hoverable_item(&format!("  {}", text), text, &theme, bg, false);
                content.fixed(&btn, 22);
                let u = url.to_string();
                btn.set_callback(move |_| {
                    let _ = open::that(&u);
                });
            }
        }

        fspacer(&mut content, 12, bg);

        // ── Donate: [spacer] btn [spacer] ──
        let mut donate_row = Flex::default().row();
        donate_row.set_color(bg);
        spacer(bg);
        let mut dbtn = action_button("Support FerrisPad", &theme);
        donate_row.fixed(&dbtn, 180);
        dbtn.set_callback(|_| {
            let _ = open::that(DONATE_URL);
        });
        spacer(bg);
        donate_row.end();
        content.fixed(&donate_row, 32);

        content.end();

        let n = content.children();
        let fixed_h = 36 + 22 + 8 + 32 + 12 + box_inner_h + 12 + 32;
        let pads = content.pad() * (n - 1).max(0);
        self.flex.fixed(&content, fixed_h + pads);

        spacer(bg);

        self.flex.end();
        self.flex.show();
        self.is_visible = true;
        self.last_theme_bg = Some(theme_bg);
    }
}

// ── Widget helpers ──

/// Draw a rounded border on a Flex widget using theme colors.
/// Draws fill + border BEFORE children, then lets FLTK draw children normally.
fn add_rounded_border(widget: &mut Flex, fill: Color, border: Color) {
    let radius = fltk::app::frame_border_radius_max();
    // Set frame to NoBox so FLTK doesn't draw its own background
    widget.set_frame(FrameType::NoBox);
    widget.draw(move |w| {
        let x = w.x();
        let y = w.y();
        let ww = w.w();
        let hh = w.h();
        // Fill
        draw::set_draw_color(fill);
        draw::draw_rounded_rectf(x, y, ww, hh, radius);
        // Border
        draw::set_draw_color(border);
        draw::draw_rounded_rect(x, y, ww, hh, radius);
        // Draw children on top
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
    f.set_align(Align::Left | Align::Inside);
    f.set_frame(FrameType::FlatBox);
    f.set_color(bg);
    f
}

/// Action button with hover highlight (color swap on Enter/Leave).
fn action_button(label: &str, theme: &DialogTheme) -> Button {
    let mut btn = Button::default();
    btn.set_label(label);
    btn.set_frame(FrameType::RFlatBox);
    btn.set_color(theme.button_bg);
    btn.set_label_color(theme.text);
    btn.set_label_size(12);
    // Hover: swap bg and label color
    let normal_bg = theme.button_bg;
    let normal_fg = theme.text;
    let hover_bg = theme.tab_active_bg;
    btn.handle(move |btn, ev| {
        match ev {
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
        }
    });
    btn
}

/// Hoverable list item — shows tooltip on truncated text, bold + color change on hover.
/// `full_text` is shown as tooltip if different from the visible label.
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

    // Tooltip with full text (useful when label is clipped)
    btn.set_tooltip(full_text);

    // Hover: bold font + brighter text (no bg change — avoids conflicts with parent custom draw)
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
    files.truncate(MAX_RECENT_SESSIONS);
    files
}
