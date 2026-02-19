use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    app::Sender,
    draw,
    enums::{Align, Color, Event, Font},
    prelude::*,
    widget::Widget,
};

use crate::app::document::{Document, DocumentId};
use crate::app::messages::Message;

pub const TAB_BAR_HEIGHT: i32 = 30;

const MIN_TAB_WIDTH: i32 = 60;
const MAX_TAB_WIDTH: i32 = 200;
const CLOSE_BTN_SIZE: i32 = 14;
const CLOSE_BTN_MARGIN: i32 = 6;
const TAB_H_PADDING: i32 = 10;
const CORNER_RADIUS: i32 = 6;
const TAB_GAP: i32 = 1;
const PLUS_BTN_WIDTH: i32 = 28;
const PLUS_BTN_MARGIN: i32 = 4;

struct TabInfo {
    id: DocumentId,
    display_name: String,
    is_dirty: bool,
    is_active: bool,
}

struct TabBarState {
    tabs: Vec<TabInfo>,
    is_dark: bool,
    hover_tab_index: Option<usize>,
    hover_close: bool,
    hover_plus: bool,
    drag_source: Option<usize>,
    drag_target: Option<usize>,
    sender: Sender<Message>,
}

pub struct TabBar {
    pub widget: Widget,
    state: Rc<RefCell<TabBarState>>,
}

impl TabBar {
    pub fn new(x: i32, y: i32, w: i32, sender: Sender<Message>) -> Self {
        let state = Rc::new(RefCell::new(TabBarState {
            tabs: Vec::new(),
            is_dark: false,
            hover_tab_index: None,
            hover_close: false,
            hover_plus: false,
            drag_source: None,
            drag_target: None,
            sender,
        }));

        let mut widget = Widget::new(x, y, w, TAB_BAR_HEIGHT, None);

        // Draw callback
        let draw_state = state.clone();
        widget.draw(move |wid| {
            let st = draw_state.borrow();
            draw_tab_bar(wid, &st);
        });

        // Handle callback
        let handle_state = state.clone();
        widget.handle(move |wid, event| {
            handle_tab_bar(wid, event, &handle_state)
        });

        Self { widget, state }
    }

    pub fn rebuild(
        &mut self,
        documents: &[Document],
        active_id: Option<DocumentId>,
        _sender: &Sender<Message>,
        is_dark: bool,
    ) {
        let mut st = self.state.borrow_mut();
        st.is_dark = is_dark;
        st.tabs.clear();
        for doc in documents {
            st.tabs.push(TabInfo {
                id: doc.id,
                display_name: doc.display_name.clone(),
                is_dirty: doc.is_dirty(),
                is_active: active_id == Some(doc.id),
            });
        }
        st.hover_tab_index = None;
        st.hover_close = false;
        drop(st);
        self.widget.redraw();
    }

    pub fn apply_theme(&mut self, is_dark: bool) {
        self.state.borrow_mut().is_dark = is_dark;
        self.widget.redraw();
    }

}

// --- Colors ---

struct ThemeColors {
    bar_bg: Color,
    active_bg: Color,
    inactive_bg: Color,
    active_text: Color,
    inactive_text: Color,
    close_hover_bg: Color,
}

fn theme_colors(is_dark: bool) -> ThemeColors {
    if is_dark {
        ThemeColors {
            bar_bg: Color::from_rgb(25, 25, 25),
            active_bg: Color::from_rgb(50, 50, 50),
            inactive_bg: Color::from_rgb(35, 35, 35),
            active_text: Color::from_rgb(230, 230, 230),
            inactive_text: Color::from_rgb(140, 140, 140),
            close_hover_bg: Color::from_rgb(70, 70, 70),
        }
    } else {
        ThemeColors {
            bar_bg: Color::from_rgb(200, 200, 200),
            active_bg: Color::from_rgb(255, 255, 255),
            inactive_bg: Color::from_rgb(220, 220, 220),
            active_text: Color::from_rgb(0, 0, 0),
            inactive_text: Color::from_rgb(80, 80, 80),
            close_hover_bg: Color::from_rgb(190, 190, 190),
        }
    }
}

// --- Layout helpers ---

fn compute_tab_width(widget_w: i32, tab_count: usize) -> i32 {
    if tab_count == 0 {
        return MAX_TAB_WIDTH;
    }
    let reserved = PLUS_BTN_WIDTH + PLUS_BTN_MARGIN;
    let total_gaps = TAB_GAP * (tab_count as i32 - 1);
    let w = (widget_w - total_gaps - reserved) / tab_count as i32;
    w.clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH)
}

fn tab_x_offset(widget_x: i32, index: usize, tab_width: i32) -> i32 {
    widget_x + (index as i32) * (tab_width + TAB_GAP)
}

/// Hit-test: returns (tab_index, is_close_btn)
fn hit_test(
    widget_x: i32,
    widget_y: i32,
    tab_count: usize,
    tab_width: i32,
    mx: i32,
    my: i32,
) -> Option<(usize, bool)> {
    if tab_count == 0 {
        return None;
    }
    let tab_h = TAB_BAR_HEIGHT;
    if my < widget_y || my >= widget_y + tab_h {
        return None;
    }

    let relative_x = mx - widget_x;
    if relative_x < 0 {
        return None;
    }

    let stride = tab_width + TAB_GAP;
    let idx = (relative_x / stride) as usize;
    if idx >= tab_count {
        return None;
    }

    // Click landed in the gap between tabs
    let pos_in_cell = relative_x % stride;
    if pos_in_cell >= tab_width {
        return None;
    }

    // Check close button area
    let tx = tab_x_offset(widget_x, idx, tab_width);
    let close_x = tx + tab_width - CLOSE_BTN_MARGIN - CLOSE_BTN_SIZE;
    let close_y = widget_y + (tab_h - CLOSE_BTN_SIZE) / 2;
    let is_close = mx >= close_x
        && mx <= close_x + CLOSE_BTN_SIZE
        && my >= close_y
        && my <= close_y + CLOSE_BTN_SIZE;

    Some((idx, is_close))
}

fn plus_btn_x(widget_x: i32, tab_count: usize, tab_width: i32) -> i32 {
    if tab_count == 0 {
        widget_x + PLUS_BTN_MARGIN
    } else {
        tab_x_offset(widget_x, tab_count, tab_width) + PLUS_BTN_MARGIN
    }
}

fn hit_test_plus_btn(
    widget_x: i32,
    widget_y: i32,
    tab_count: usize,
    tab_width: i32,
    mx: i32,
    my: i32,
) -> bool {
    let px = plus_btn_x(widget_x, tab_count, tab_width);
    mx >= px && mx < px + PLUS_BTN_WIDTH && my >= widget_y && my < widget_y + TAB_BAR_HEIGHT
}

// --- Truncation ---

fn truncate_to_fit(text: &str, max_width: i32) -> String {
    if max_width <= 0 {
        return String::new();
    }
    draw::set_font(Font::Helvetica, 12);
    let (tw, _) = draw::measure(text, true);
    if tw <= max_width {
        return text.to_string();
    }

    let ellipsis = "...";
    let (ew, _) = draw::measure(ellipsis, true);
    if ew >= max_width {
        return ellipsis.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    for len in (1..chars.len()).rev() {
        let candidate: String = chars[..len].iter().collect();
        let full = format!("{}{}", candidate, ellipsis);
        let (fw, _) = draw::measure(&full, true);
        if fw <= max_width {
            return full;
        }
    }
    ellipsis.to_string()
}

// --- Drawing ---

fn draw_rounded_top_rect(x: i32, y: i32, w: i32, h: i32, r: i32, color: Color) {
    draw::set_draw_color(color);
    // Main body below the rounded corners
    draw::draw_rectf(x, y + r, w, h - r);
    // Top strip between corners
    draw::draw_rectf(x + r, y, w - 2 * r, r);
    // Top-left corner (quarter arc)
    draw::draw_pie(x, y, 2 * r, 2 * r, 90.0, 180.0);
    // Top-right corner (quarter arc)
    draw::draw_pie(x + w - 2 * r, y, 2 * r, 2 * r, 0.0, 90.0);
}

fn draw_tab_bar(wid: &Widget, st: &TabBarState) {
    let wx = wid.x();
    let wy = wid.y();
    let ww = wid.w();
    let wh = wid.h();
    let colors = theme_colors(st.is_dark);

    // Background
    draw::set_draw_color(colors.bar_bg);
    draw::draw_rectf(wx, wy, ww, wh);

    let tab_count = st.tabs.len();
    let tab_width = compute_tab_width(ww, tab_count);
    let tab_h = wh;

    // Draw "+" button
    {
        let px = plus_btn_x(wx, tab_count, tab_width);
        let btn_h = wh - 4;
        let btn_y = wy + 2;
        let bg = if st.hover_plus {
            if st.is_dark {
                Color::from_rgb(60, 60, 60)
            } else {
                Color::from_rgb(210, 210, 210)
            }
        } else {
            colors.inactive_bg
        };
        draw_rounded_top_rect(px, btn_y, PLUS_BTN_WIDTH, btn_h, CORNER_RADIUS, bg);
        let text_color = if st.hover_plus {
            colors.active_text
        } else {
            colors.inactive_text
        };
        draw::set_draw_color(text_color);
        draw::set_font(Font::HelveticaBold, 16);
        draw::draw_text2("+", px, btn_y, PLUS_BTN_WIDTH, btn_h, Align::Center);
    }

    for (i, tab) in st.tabs.iter().enumerate() {
        let tx = tab_x_offset(wx, i, tab_width);

        // Tab background
        if tab.is_active {
            draw_rounded_top_rect(tx, wy, tab_width, tab_h, CORNER_RADIUS, colors.active_bg);
        } else {
            // Inactive tabs are slightly shorter — 2px gap at bottom
            let inactive_h = tab_h - 2;
            draw_rounded_top_rect(tx, wy + 2, tab_width, inactive_h, CORNER_RADIUS, colors.inactive_bg);
        }

        // Text label
        let text_color = if tab.is_active {
            colors.active_text
        } else {
            colors.inactive_text
        };

        let label = if tab.is_dirty {
            format!("\u{25cf} {}", tab.display_name)
        } else {
            tab.display_name.clone()
        };

        // Available text width: tab_width minus padding on left, minus close button area on right
        let text_area_width = tab_width - TAB_H_PADDING - CLOSE_BTN_MARGIN - CLOSE_BTN_SIZE - TAB_H_PADDING;
        let display_text = truncate_to_fit(&label, text_area_width);

        draw::set_draw_color(text_color);
        draw::set_font(Font::Helvetica, 12);
        let text_x = tx + TAB_H_PADDING;
        let text_y = wy + (tab_h + 12) / 2; // baseline approx centered
        draw::draw_text(&display_text, text_x, text_y);

        // Close button
        let close_x = tx + tab_width - CLOSE_BTN_MARGIN - CLOSE_BTN_SIZE;
        let close_y = wy + (tab_h - CLOSE_BTN_SIZE) / 2;

        // Hover highlight on close button
        let is_hovered_tab = st.hover_tab_index == Some(i);
        if is_hovered_tab && st.hover_close {
            draw::set_draw_color(colors.close_hover_bg);
            draw::draw_rectf(close_x - 2, close_y - 2, CLOSE_BTN_SIZE + 4, CLOSE_BTN_SIZE + 4);
        }

        // Draw "x" on all tabs (dimmer on inactive, brighter on active/hovered)
        let close_color = if tab.is_active || is_hovered_tab {
            text_color
        } else {
            colors.inactive_text
        };
        draw::set_draw_color(close_color);
        draw::set_font(Font::HelveticaBold, 20);
        draw::draw_text2(
            "\u{00d7}",
            close_x,
            close_y,
            CLOSE_BTN_SIZE,
            CLOSE_BTN_SIZE,
            Align::Center,
        );
    }

    // Draw drop indicator during drag
    if let (Some(source), Some(target)) = (st.drag_source, st.drag_target) {
        if source != target {
            let indicator_x = if target <= source {
                tab_x_offset(wx, target, tab_width)
            } else {
                tab_x_offset(wx, target, tab_width) + tab_width
            };
            let indicator_color = if st.is_dark {
                Color::from_rgb(100, 160, 255)
            } else {
                Color::from_rgb(30, 100, 220)
            };
            draw::set_draw_color(indicator_color);
            draw::draw_rectf(indicator_x - 1, wy + 2, 3, tab_h - 4);
        }
    }
}

// --- Event handling ---

fn handle_tab_bar(wid: &mut Widget, event: Event, state: &Rc<RefCell<TabBarState>>) -> bool {
    match event {
        Event::Push => {
            let mut st = state.borrow_mut();
            let tab_count = st.tabs.len();
            let tab_width = compute_tab_width(wid.w(), tab_count);
            let mx = fltk::app::event_x();
            let my = fltk::app::event_y();
            let button = fltk::app::event_button();

            // Check "+" button
            if button == 1
                && hit_test_plus_btn(wid.x(), wid.y(), tab_count, tab_width, mx, my)
            {
                let sender = st.sender.clone();
                drop(st);
                sender.send(Message::FileNew);
                return true;
            }

            if tab_count == 0 {
                return false;
            }

            if let Some((idx, is_close)) = hit_test(wid.x(), wid.y(), tab_count, tab_width, mx, my)
            {
                let tab_id = st.tabs[idx].id;
                let sender = st.sender.clone();

                if button == 2 {
                    drop(st);
                    sender.send(Message::TabClose(tab_id));
                } else if button == 1 {
                    if is_close {
                        drop(st);
                        sender.send(Message::TabClose(tab_id));
                    } else {
                        // Start potential drag
                        st.drag_source = Some(idx);
                        st.drag_target = None;
                        drop(st);
                        sender.send(Message::TabSwitch(tab_id));
                    }
                }
                return true;
            }
            false
        }
        Event::Drag => {
            let mut st = state.borrow_mut();
            if st.drag_source.is_none() {
                return false;
            }
            let tab_count = st.tabs.len();
            if tab_count < 2 {
                return true;
            }
            let tab_width = compute_tab_width(wid.w(), tab_count);
            let mx = fltk::app::event_x();
            let my = fltk::app::event_y();

            let new_target = hit_test(wid.x(), wid.y(), tab_count, tab_width, mx, my)
                .map(|(idx, _)| idx);

            if new_target != st.drag_target {
                st.drag_target = new_target;
                drop(st);
                wid.redraw();
            }
            true
        }
        Event::Released => {
            let mut st = state.borrow_mut();
            let source = st.drag_source.take();
            let target = st.drag_target.take();
            drop(st);
            wid.redraw();

            if let (Some(from), Some(to)) = (source, target) {
                if from != to {
                    let st = state.borrow();
                    let sender = st.sender.clone();
                    drop(st);
                    sender.send(Message::TabMove(from, to));
                }
            }
            false
        }
        Event::Move => {
            let mut st = state.borrow_mut();
            let tab_count = st.tabs.len();
            let tab_width = compute_tab_width(wid.w(), tab_count);
            let mx = fltk::app::event_x();
            let my = fltk::app::event_y();

            let new_hover_plus =
                hit_test_plus_btn(wid.x(), wid.y(), tab_count, tab_width, mx, my);

            let (new_hover, new_close) = if tab_count > 0 {
                if let Some((idx, is_close)) =
                    hit_test(wid.x(), wid.y(), tab_count, tab_width, mx, my)
                {
                    (Some(idx), is_close)
                } else {
                    (None, false)
                }
            } else {
                (None, false)
            };

            if new_hover != st.hover_tab_index
                || new_close != st.hover_close
                || new_hover_plus != st.hover_plus
            {
                st.hover_tab_index = new_hover;
                st.hover_close = new_close;
                st.hover_plus = new_hover_plus;
                drop(st);
                wid.redraw();
            }
            true
        }
        Event::Leave => {
            let mut st = state.borrow_mut();
            st.drag_source = None;
            st.drag_target = None;
            if st.hover_tab_index.is_some() || st.hover_close || st.hover_plus {
                st.hover_tab_index = None;
                st.hover_close = false;
                st.hover_plus = false;
                drop(st);
                wid.redraw();
            }
            false
        }
        _ => false,
    }
}
