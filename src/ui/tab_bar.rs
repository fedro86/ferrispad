use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    app::Sender,
    draw,
    enums::{Align, Color, Event, Font, Shortcut},
    menu::{MenuButton, MenuFlag},
    prelude::*,
    widget::Widget,
};

use crate::app::document::{Document, DocumentId};
use crate::app::messages::Message;
use crate::app::tab_manager::{GroupColor, GroupId, TabGroup};

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

const GROUP_LABEL_H_PAD: i32 = 8;
const GROUP_LABEL_GAP: i32 = 4;
const GROUP_UNDERLINE: i32 = 2;
const COLLAPSED_CHIP_MIN_W: i32 = 50;
const GROUP_DOT_RADIUS: i32 = 5;

struct TabInfo {
    id: DocumentId,
    display_name: String,
    is_dirty: bool,
    is_active: bool,
    group_id: Option<GroupId>,
}

#[derive(Clone)]
struct GroupInfo {
    id: GroupId,
    name: String,
    color: GroupColor,
    collapsed: bool,
}

#[derive(Clone)]
enum LayoutItem {
    Tab { index: usize, x: i32, width: i32 },
    GroupLabel { group_id: GroupId, x: i32, width: i32 },
    CollapsedChip { group_id: GroupId, x: i32, width: i32, count: usize, name: String, color: GroupColor },
    PlusButton { x: i32 },
}

enum HitResult {
    Tab { index: usize, is_close: bool },
    GroupLabel(GroupId),
    CollapsedChip(GroupId),
    PlusButton,
    None,
}

#[derive(Clone, Copy, PartialEq)]
enum DragSource {
    Tab(usize),          // dragging a single tab
    Group(GroupId),      // dragging a collapsed group chip
}

#[derive(Clone, Copy, PartialEq)]
enum DragTarget {
    OnTab(usize),    // center zone → will group
    InsertAt(usize), // edge zone → will reorder (index = insertion point)
}

struct TabBarState {
    tabs: Vec<TabInfo>,
    groups: Vec<GroupInfo>,
    layout: Vec<LayoutItem>,
    is_dark: bool,
    hover_tab_index: Option<usize>,
    hover_close: bool,
    hover_plus: bool,
    hover_group_label: Option<GroupId>,
    hover_collapsed_chip: Option<GroupId>,
    drag_source: Option<DragSource>,
    drag_target: Option<DragTarget>,
    sender: Sender<Message>,
    widget_w: i32,
}

pub struct TabBar {
    pub widget: Widget,
    state: Rc<RefCell<TabBarState>>,
}

impl TabBar {
    pub fn new(x: i32, y: i32, w: i32, sender: Sender<Message>) -> Self {
        let state = Rc::new(RefCell::new(TabBarState {
            tabs: Vec::new(),
            groups: Vec::new(),
            layout: Vec::new(),
            is_dark: false,
            hover_tab_index: None,
            hover_close: false,
            hover_plus: false,
            hover_group_label: None,
            hover_collapsed_chip: None,
            drag_source: None,
            drag_target: None,
            sender,
            widget_w: w,
        }));

        let mut widget = Widget::new(x, y, w, TAB_BAR_HEIGHT, None);

        let draw_state = state.clone();
        widget.draw(move |wid| {
            let st = draw_state.borrow();
            draw_tab_bar(wid, &st);
        });

        let handle_state = state.clone();
        widget.handle(move |wid, event| {
            handle_tab_bar(wid, event, &handle_state)
        });

        Self { widget, state }
    }

    pub fn rebuild(
        &mut self,
        documents: &[Document],
        groups: &[TabGroup],
        active_id: Option<DocumentId>,
        _sender: &Sender<Message>,
        is_dark: bool,
    ) {
        let mut st = self.state.borrow_mut();
        st.is_dark = is_dark;
        st.widget_w = self.widget.w();
        st.tabs.clear();
        st.groups.clear();
        for doc in documents {
            st.tabs.push(TabInfo {
                id: doc.id,
                display_name: doc.display_name.clone(),
                is_dirty: doc.is_dirty(),
                is_active: active_id == Some(doc.id),
                group_id: doc.group_id,
            });
        }
        for g in groups {
            st.groups.push(GroupInfo {
                id: g.id,
                name: g.name.clone(),
                color: g.color,
                collapsed: g.collapsed,
            });
        }
        st.hover_tab_index = None;
        st.hover_close = false;
        st.hover_group_label = None;
        st.hover_collapsed_chip = None;
        compute_layout(&mut st);
        drop(st);
        self.widget.redraw();
    }

    pub fn apply_theme(&mut self, is_dark: bool) {
        self.state.borrow_mut().is_dark = is_dark;
        self.widget.redraw();
    }
}

// --- Layout computation ---

fn group_label_width(name: &str) -> i32 {
    if name.is_empty() {
        // Just a colored dot
        GROUP_DOT_RADIUS * 2 + GROUP_LABEL_H_PAD * 2
    } else {
        draw::set_font(Font::Helvetica, 11);
        let (tw, _) = draw::measure(name, true);
        tw + GROUP_LABEL_H_PAD * 2
    }
}

fn collapsed_chip_width(name: &str, count: usize) -> i32 {
    let label = if name.is_empty() {
        format!("({count})")
    } else {
        format!("{name} ({count})")
    };
    draw::set_font(Font::Helvetica, 11);
    let (tw, _) = draw::measure(&label, true);
    (tw + GROUP_LABEL_H_PAD * 2 + GROUP_DOT_RADIUS * 2 + 4).max(COLLAPSED_CHIP_MIN_W)
}

fn compute_layout(st: &mut TabBarState) {
    st.layout.clear();
    let widget_w = st.widget_w;

    if st.tabs.is_empty() {
        st.layout.push(LayoutItem::PlusButton { x: PLUS_BTN_MARGIN });
        return;
    }

    // First pass: determine which items exist and their fixed-width contributions
    // Walk tabs in order, emitting group labels/chips as needed
    let mut items_draft: Vec<LayoutItem> = Vec::new();
    let mut visible_tab_count: i32 = 0;
    let mut fixed_width: i32 = 0; // group labels + collapsed chips + plus btn + gaps
    let mut current_group: Option<GroupId> = None;
    let mut i = 0;

    while i < st.tabs.len() {
        let tab_group = st.tabs[i].group_id;

        if tab_group != current_group {
            current_group = tab_group;

            if let Some(gid) = tab_group
                && let Some(ginfo) = st.groups.iter().find(|g| g.id == gid) {
                    if ginfo.collapsed {
                        // Count how many tabs are in this group
                        let count = st.tabs[i..].iter().take_while(|t| t.group_id == Some(gid)).count();
                        let cw = collapsed_chip_width(&ginfo.name, count);
                        items_draft.push(LayoutItem::CollapsedChip {
                            group_id: gid,
                            x: 0, // filled in later
                            width: cw,
                            count,
                            name: ginfo.name.clone(),
                            color: ginfo.color,
                        });
                        fixed_width += cw + TAB_GAP;
                        i += count;
                        continue;
                    } else {
                        // Emit group label
                        let lw = group_label_width(&ginfo.name);
                        items_draft.push(LayoutItem::GroupLabel {
                            group_id: gid,
                            x: 0,
                            width: lw,
                        });
                        fixed_width += lw + GROUP_LABEL_GAP;
                    }
                }
        }

        // Emit tab placeholder
        items_draft.push(LayoutItem::Tab {
            index: i,
            x: 0,
            width: 0, // filled in later
        });
        visible_tab_count += 1;
        i += 1;
    }

    // Plus button
    fixed_width += PLUS_BTN_WIDTH + PLUS_BTN_MARGIN;

    // Gaps between tabs
    if visible_tab_count > 1 {
        fixed_width += TAB_GAP * (visible_tab_count - 1);
    }

    // Compute tab width
    let available = widget_w - fixed_width;
    let tab_width = if visible_tab_count > 0 {
        (available / visible_tab_count).clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH)
    } else {
        MAX_TAB_WIDTH
    };

    // Second pass: assign x positions
    let mut cursor_x = 0i32;
    for item in &mut items_draft {
        match item {
            LayoutItem::Tab { x, width, .. } => {
                *x = cursor_x;
                *width = tab_width;
                cursor_x += tab_width + TAB_GAP;
            }
            LayoutItem::GroupLabel { x, width, .. } => {
                *x = cursor_x;
                cursor_x += *width + GROUP_LABEL_GAP;
            }
            LayoutItem::CollapsedChip { x, width, .. } => {
                *x = cursor_x;
                cursor_x += *width + TAB_GAP;
            }
            LayoutItem::PlusButton { .. } => {}
        }
    }

    // Plus button at the end
    items_draft.push(LayoutItem::PlusButton { x: cursor_x + PLUS_BTN_MARGIN });

    st.layout = items_draft;
}

// --- Hit-testing ---

fn hit_test_layout(items: &[LayoutItem], wy: i32, mx: i32, my: i32) -> HitResult {
    if my < wy || my >= wy + TAB_BAR_HEIGHT {
        return HitResult::None;
    }

    for item in items {
        match item {
            LayoutItem::Tab { index, x, width } => {
                if mx >= *x && mx < *x + *width {
                    // Check close button
                    let close_x = *x + *width - CLOSE_BTN_MARGIN - CLOSE_BTN_SIZE;
                    let close_y = wy + (TAB_BAR_HEIGHT - CLOSE_BTN_SIZE) / 2;
                    let is_close = mx >= close_x
                        && mx <= close_x + CLOSE_BTN_SIZE
                        && my >= close_y
                        && my <= close_y + CLOSE_BTN_SIZE;
                    return HitResult::Tab { index: *index, is_close };
                }
            }
            LayoutItem::GroupLabel { group_id, x, width } => {
                if mx >= *x && mx < *x + *width {
                    return HitResult::GroupLabel(*group_id);
                }
            }
            LayoutItem::CollapsedChip { group_id, x, width, .. } => {
                if mx >= *x && mx < *x + *width {
                    return HitResult::CollapsedChip(*group_id);
                }
            }
            LayoutItem::PlusButton { x } => {
                if mx >= *x && mx < *x + PLUS_BTN_WIDTH {
                    return HitResult::PlusButton;
                }
            }
        }
    }
    HitResult::None
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
        let full = format!("{candidate}{ellipsis}");
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
    draw::draw_rectf(x, y + r, w, h - r);
    draw::draw_rectf(x + r, y, w - 2 * r, r);
    draw::draw_pie(x, y, 2 * r, 2 * r, 90.0, 180.0);
    draw::draw_pie(x + w - 2 * r, y, 2 * r, 2 * r, 0.0, 90.0);
}

fn draw_rounded_rect(x: i32, y: i32, w: i32, h: i32, r: i32, color: Color) {
    draw::set_draw_color(color);
    // Center body
    draw::draw_rectf(x + r, y, w - 2 * r, h);
    // Left strip
    draw::draw_rectf(x, y + r, r, h - 2 * r);
    // Right strip
    draw::draw_rectf(x + w - r, y + r, r, h - 2 * r);
    // Four corners
    draw::draw_pie(x, y, 2 * r, 2 * r, 90.0, 180.0);
    draw::draw_pie(x + w - 2 * r, y, 2 * r, 2 * r, 0.0, 90.0);
    draw::draw_pie(x, y + h - 2 * r, 2 * r, 2 * r, 180.0, 270.0);
    draw::draw_pie(x + w - 2 * r, y + h - 2 * r, 2 * r, 2 * r, 270.0, 360.0);
}

fn group_color_to_fltk(gc: GroupColor, is_dark: bool) -> Color {
    let (r, g, b) = if is_dark { gc.to_rgb_dark() } else { gc.to_rgb() };
    Color::from_rgb(r, g, b)
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

    for item in &st.layout {
        match item {
            LayoutItem::Tab { index, x, width } => {
                let tx = wx + *x;
                let tab_width = *width;
                let tab = &st.tabs[*index];

                // Tab background
                if tab.is_active {
                    draw_rounded_top_rect(tx, wy, tab_width, wh, CORNER_RADIUS, colors.active_bg);
                } else {
                    let inactive_h = wh - 2;
                    draw_rounded_top_rect(tx, wy + 2, tab_width, inactive_h, CORNER_RADIUS, colors.inactive_bg);
                }

                // Group color underline
                if let Some(gid) = tab.group_id
                    && let Some(ginfo) = st.groups.iter().find(|g| g.id == gid) {
                        let gc = group_color_to_fltk(ginfo.color, st.is_dark);
                        draw::set_draw_color(gc);
                        draw::draw_rectf(tx, wy + wh - GROUP_UNDERLINE, tab_width, GROUP_UNDERLINE);
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

                let text_area_width = tab_width - TAB_H_PADDING - CLOSE_BTN_MARGIN - CLOSE_BTN_SIZE - TAB_H_PADDING;
                let display_text = truncate_to_fit(&label, text_area_width);

                draw::set_draw_color(text_color);
                draw::set_font(Font::Helvetica, 12);
                let text_x = tx + TAB_H_PADDING;
                let text_y = wy + (wh + 12) / 2;
                draw::draw_text(&display_text, text_x, text_y);

                // Close button
                let close_x = tx + tab_width - CLOSE_BTN_MARGIN - CLOSE_BTN_SIZE;
                let close_y = wy + (wh - CLOSE_BTN_SIZE) / 2;

                let is_hovered_tab = st.hover_tab_index == Some(*index);
                if is_hovered_tab && st.hover_close {
                    draw::set_draw_color(colors.close_hover_bg);
                    draw::draw_rectf(close_x - 2, close_y - 2, CLOSE_BTN_SIZE + 4, CLOSE_BTN_SIZE + 4);
                }

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
            LayoutItem::GroupLabel { group_id, x, width } => {
                let lx = wx + *x;
                let is_hover = st.hover_group_label == Some(*group_id);
                if let Some(ginfo) = st.groups.iter().find(|g| g.id == *group_id) {
                    let gc = group_color_to_fltk(ginfo.color, st.is_dark);
                    let pill_h = 18;
                    let pill_y = wy + (wh - pill_h) / 2;

                    // Pill background (slightly brighter on hover)
                    let bg = if is_hover {
                        let (r, g, b) = if st.is_dark { ginfo.color.to_rgb_dark() } else { ginfo.color.to_rgb() };
                        Color::from_rgb(r.saturating_add(30), g.saturating_add(30), b.saturating_add(30))
                    } else {
                        gc
                    };
                    draw_rounded_rect(lx, pill_y, *width, pill_h, 4, bg);

                    if ginfo.name.is_empty() {
                        // Draw a white dot in the center
                        let dot_x = lx + *width / 2;
                        let dot_y = pill_y + pill_h / 2;
                        draw::set_draw_color(Color::from_rgb(255, 255, 255));
                        draw::draw_pie(
                            dot_x - GROUP_DOT_RADIUS,
                            dot_y - GROUP_DOT_RADIUS,
                            GROUP_DOT_RADIUS * 2,
                            GROUP_DOT_RADIUS * 2,
                            0.0,
                            360.0,
                        );
                    } else {
                        // Draw group name
                        draw::set_draw_color(Color::from_rgb(255, 255, 255));
                        draw::set_font(Font::Helvetica, 11);
                        draw::draw_text2(&ginfo.name, lx, pill_y, *width, pill_h, Align::Center);
                    }
                }
            }
            LayoutItem::CollapsedChip { group_id, x, width, count, name, color } => {
                let cx = wx + *x;
                let chip_h = wh - 4;
                let chip_y = wy + 2;
                let is_hover = st.hover_collapsed_chip == Some(*group_id);

                let gc = group_color_to_fltk(*color, st.is_dark);
                let bg = if is_hover {
                    let (r, g, b) = if st.is_dark { color.to_rgb_dark() } else { color.to_rgb() };
                    Color::from_rgb(r.saturating_add(30), g.saturating_add(30), b.saturating_add(30))
                } else {
                    gc
                };
                draw_rounded_top_rect(cx, chip_y, *width, chip_h, CORNER_RADIUS, bg);

                // Label
                let label = if name.is_empty() {
                    format!("({count})")
                } else {
                    format!("{name} ({count})")
                };
                draw::set_draw_color(Color::from_rgb(255, 255, 255));
                draw::set_font(Font::Helvetica, 11);

                if name.is_empty() {
                    // Draw dot + count
                    let dot_x = cx + GROUP_LABEL_H_PAD;
                    let dot_y = chip_y + (chip_h - GROUP_DOT_RADIUS * 2) / 2;
                    draw::draw_pie(dot_x, dot_y, GROUP_DOT_RADIUS * 2, GROUP_DOT_RADIUS * 2, 0.0, 360.0);
                    let text_x = dot_x + GROUP_DOT_RADIUS * 2 + 4;
                    draw::draw_text2(
                        &format!("({count})"),
                        text_x,
                        chip_y,
                        *width - (text_x - cx) - GROUP_LABEL_H_PAD,
                        chip_h,
                        Align::Left | Align::Inside,
                    );
                } else {
                    draw::draw_text2(&label, cx, chip_y, *width, chip_h, Align::Center);
                }
            }
            LayoutItem::PlusButton { x } => {
                let px = wx + *x;
                let btn_h = wh - 4;
                let btn_y = wy + 2;
                let bg = if st.hover_plus {
                    if st.is_dark {
                        Color::from_rgb(60, 60, 60)
                    } else {
                        Color::from_rgb(210, 210, 210)
                    }
                } else if st.is_dark {
                    Color::from_rgb(35, 35, 35)
                } else {
                    Color::from_rgb(220, 220, 220)
                };
                draw_rounded_top_rect(px, btn_y, PLUS_BTN_WIDTH, btn_h, CORNER_RADIUS, bg);
                let text_color = if st.hover_plus {
                    if st.is_dark { Color::from_rgb(230, 230, 230) } else { Color::from_rgb(0, 0, 0) }
                } else if st.is_dark { Color::from_rgb(140, 140, 140) } else { Color::from_rgb(80, 80, 80) };
                draw::set_draw_color(text_color);
                draw::set_font(Font::HelveticaBold, 16);
                draw::draw_text2("+", px, btn_y, PLUS_BTN_WIDTH, btn_h, Align::Center);
            }
        }
    }

    // Draw drop indicator during drag
    if let (Some(_source), Some(ref drag_target)) = (st.drag_source, st.drag_target) {
        match drag_target {
            DragTarget::OnTab(target_idx) => {
                // Highlight the target tab — 50% blend of blue tint over tab background
                if let Some(item) = st.layout.iter().find(|it| matches!(it, LayoutItem::Tab { index, .. } if index == target_idx))
                    && let LayoutItem::Tab { x, width, .. } = item {
                        let is_active = st.tabs.get(*target_idx).is_some_and(|t| t.is_active);
                        let bg = if is_active { colors.active_bg } else { colors.inactive_bg };
                        let (br, bg_g, bb) = bg.to_rgb();
                        let (tr, tg, tb) = if st.is_dark {
                            (60u8, 120u8, 220u8)
                        } else {
                            (80u8, 160u8, 255u8)
                        };
                        // 50% blend: (bg + tint) / 2
                        let blended = Color::from_rgb(
                            ((br as u16 + tr as u16) / 2) as u8,
                            ((bg_g as u16 + tg as u16) / 2) as u8,
                            ((bb as u16 + tb as u16) / 2) as u8,
                        );
                        draw_rounded_top_rect(wx + *x, wy + 2, *width, wh - 2, CORNER_RADIUS, blended);
                    }
            }
            DragTarget::InsertAt(pos) => {
                // Draw vertical insertion line at the position
                let indicator_x = if let Some(item) = st.layout.iter().find(|it| matches!(it, LayoutItem::Tab { index, .. } if index == pos)) {
                    // Insert before this visible tab
                    if let LayoutItem::Tab { x, .. } = item { Some(wx + *x) } else { None }
                } else if *pos > 0 {
                    // Check if previous tab is visible
                    if let Some(item) = st.layout.iter().find(|it| matches!(it, LayoutItem::Tab { index, .. } if *index == pos - 1)) {
                        if let LayoutItem::Tab { x, width, .. } = item { Some(wx + *x + *width) } else { None }
                    } else {
                        // Target tab is inside a collapsed group — find the chip
                        let target_group = st.tabs.get(*pos).or_else(|| st.tabs.get(pos - 1)).and_then(|t| t.group_id);
                        target_group.and_then(|gid| {
                            st.layout.iter().find(|it| matches!(it, LayoutItem::CollapsedChip { group_id, .. } if *group_id == gid))
                                .map(|it| if let LayoutItem::CollapsedChip { x, width, .. } = it { wx + *x + *width } else { 0 })
                        })
                    }
                } else {
                    Some(wx)
                };
                if let Some(ix) = indicator_x {
                    let indicator_color = if st.is_dark {
                        Color::from_rgb(100, 160, 255)
                    } else {
                        Color::from_rgb(30, 100, 220)
                    };
                    draw::set_draw_color(indicator_color);
                    draw::draw_rectf(ix - 1, wy + 2, 3, wh - 4);
                }
            }
        }
    }
}

// --- Context menu ---

fn show_context_menu(st: &TabBarState, tab_index: Option<usize>, group_id: Option<GroupId>) {
    let sender = st.sender;
    // Position at mouse cursor with a 1x1 anchor so Wayland has a valid rectangle
    let mx = fltk::app::event_x();
    let my = fltk::app::event_y();
    let mut menu = MenuButton::new(mx, my, 1, 1, None);
    let sc = Shortcut::None;
    let fl = MenuFlag::Normal;

    if let Some(idx) = tab_index {
        let tab = &st.tabs[idx];
        let tab_id = tab.id;

        if let Some(gid) = tab.group_id {
            menu.add_emit("Remove from group", sc, fl, sender, Message::TabGroupRemoveTab(tab_id));
            menu.add_emit("Rename group", sc, fl, sender, Message::TabGroupRename(gid));

            for color in GroupColor::ALL {
                let label = format!("Change color/{}", color.as_str());
                menu.add_emit(&label, sc, fl, sender, Message::TabGroupRecolor(gid, color));
            }

            menu.add_emit("Ungroup all", sc, fl, sender, Message::TabGroupDelete(gid));
            menu.add_emit("Close group", sc, fl, sender, Message::TabGroupClose(gid));
        } else {
            menu.add_emit("Add to new group", sc, fl, sender, Message::TabGroupCreate(tab_id));

            for ginfo in &st.groups {
                let label = if ginfo.name.is_empty() {
                    format!("Move to group/{}", ginfo.color.as_str())
                } else {
                    format!("Move to group/{}", ginfo.name)
                };
                menu.add_emit(&label, sc, fl, sender, Message::TabGroupAddTab(tab_id, ginfo.id));
            }
        }
    } else if let Some(gid) = group_id {
        menu.add_emit("Rename group", sc, fl, sender, Message::TabGroupRename(gid));

        for color in GroupColor::ALL {
            let label = format!("Change color/{}", color.as_str());
            menu.add_emit(&label, sc, fl, sender, Message::TabGroupRecolor(gid, color));
        }

        menu.add_emit("Ungroup all", sc, fl, sender, Message::TabGroupDelete(gid));
        menu.add_emit("Close group", sc, fl, sender, Message::TabGroupClose(gid));
    }

    menu.popup();
}

// --- Event handling ---

fn handle_tab_bar(wid: &mut Widget, event: Event, state: &Rc<RefCell<TabBarState>>) -> bool {
    match event {
        Event::Push => {
            let st = state.borrow();
            let mx = fltk::app::event_x() - wid.x();
            let my = fltk::app::event_y();
            let button = fltk::app::event_button();

            let hit = hit_test_layout(&st.layout, wid.y(), mx, my);

            match hit {
                HitResult::PlusButton if button == 1 => {
                    let sender = st.sender;
                    drop(st);
                    sender.send(Message::FileNew);
                    true
                }
                HitResult::Tab { index, is_close } => {
                    let tab_id = st.tabs[index].id;
                    let sender = st.sender;

                    if button == 3 {
                        // Right-click context menu
                        let group_id = st.tabs[index].group_id;
                        drop(st);
                        let st2 = state.borrow();
                        show_context_menu(&st2, Some(index), group_id);
                        return true;
                    }

                    if button == 2 {
                        drop(st);
                        sender.send(Message::TabClose(tab_id));
                    } else if button == 1 {
                        if is_close {
                            drop(st);
                            sender.send(Message::TabClose(tab_id));
                        } else {
                            let mut st_mut = {
                                drop(st);
                                state.borrow_mut()
                            };
                            st_mut.drag_source = Some(DragSource::Tab(index));
                            st_mut.drag_target = None;
                            let sender = st_mut.sender;
                            drop(st_mut);
                            sender.send(Message::TabSwitch(tab_id));
                        }
                    }
                    true
                }
                HitResult::GroupLabel(gid) => {
                    let sender = st.sender;
                    if button == 1 {
                        drop(st);
                        sender.send(Message::TabGroupToggle(gid));
                    } else if button == 3 {
                        drop(st);
                        let st2 = state.borrow();
                        show_context_menu(&st2, None, Some(gid));
                    }
                    true
                }
                HitResult::CollapsedChip(gid) => {
                    if button == 1 {
                        let mut st_mut = {
                            drop(st);
                            state.borrow_mut()
                        };
                        st_mut.drag_source = Some(DragSource::Group(gid));
                        st_mut.drag_target = None;
                    } else if button == 3 {
                        drop(st);
                        let st2 = state.borrow();
                        show_context_menu(&st2, None, Some(gid));
                    }
                    true
                }
                _ => {
                    false
                }
            }
        }
        Event::Drag => {
            let mut st = state.borrow_mut();
            let source = match st.drag_source {
                Some(s) => s,
                None => return false,
            };
            let mx = fltk::app::event_x() - wid.x();
            let my = fltk::app::event_y();

            let new_target = match source {
                DragSource::Tab(src_idx) => {
                    match hit_test_layout(&st.layout, wid.y(), mx, my) {
                        HitResult::Tab { index, .. } if index != src_idx => {
                            if let Some(LayoutItem::Tab { x, width, .. }) = st.layout.iter().find(|it| matches!(it, LayoutItem::Tab { index: i, .. } if *i == index)) {
                                let relative_x = mx - *x;
                                let left_zone = *width * 25 / 100;
                                let right_zone = *width * 75 / 100;
                                if relative_x < left_zone {
                                    Some(DragTarget::InsertAt(index))
                                } else if relative_x > right_zone {
                                    Some(DragTarget::InsertAt(index + 1))
                                } else {
                                    Some(DragTarget::OnTab(index))
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                }
                DragSource::Group(src_gid) => {
                    // Groups can only be reordered (InsertAt), not grouped into other tabs
                    match hit_test_layout(&st.layout, wid.y(), mx, my) {
                        HitResult::Tab { index, .. } => {
                            // Don't allow dropping inside own group
                            let is_own = st.tabs.get(index).is_some_and(|t| t.group_id == Some(src_gid));
                            if is_own {
                                None
                            } else if let Some(LayoutItem::Tab { x, width, .. }) = st.layout.iter().find(|it| matches!(it, LayoutItem::Tab { index: i, .. } if *i == index)) {
                                let half = *width / 2;
                                if (mx - *x) < half {
                                    Some(DragTarget::InsertAt(index))
                                } else {
                                    Some(DragTarget::InsertAt(index + 1))
                                }
                            } else {
                                None
                            }
                        }
                        HitResult::CollapsedChip(gid) if gid != src_gid => {
                            // Find the first tab index of this collapsed group to use as insert point
                            if let Some(idx) = st.tabs.iter().position(|t| t.group_id == Some(gid)) {
                                if let Some(LayoutItem::CollapsedChip { x, width, .. }) = st.layout.iter().find(|it| matches!(it, LayoutItem::CollapsedChip { group_id, .. } if *group_id == gid)) {
                                    let half = *width / 2;
                                    if (mx - *x) < half {
                                        Some(DragTarget::InsertAt(idx))
                                    } else {
                                        // After the last tab of this group
                                        let last = st.tabs.iter().rposition(|t| t.group_id == Some(gid)).unwrap_or(idx);
                                        Some(DragTarget::InsertAt(last + 1))
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                }
            };

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

            match (source, target) {
                (Some(src), Some(drag_target)) => {
                    let st = state.borrow();
                    let sender = st.sender;
                    match (src, drag_target) {
                        (DragSource::Tab(from), DragTarget::OnTab(target_idx)) => {
                            let source_id = st.tabs[from].id;
                            let target_id = st.tabs[target_idx].id;
                            drop(st);
                            sender.send(Message::TabGroupByDrag(source_id, target_id));
                        }
                        (DragSource::Tab(from), DragTarget::InsertAt(to)) => {
                            drop(st);
                            if from != to {
                                sender.send(Message::TabMove(from, to));
                            }
                        }
                        (DragSource::Group(gid), DragTarget::InsertAt(to)) => {
                            drop(st);
                            sender.send(Message::TabGroupMove(gid, to));
                        }
                        _ => {}
                    }
                }
                (Some(DragSource::Group(gid)), None) => {
                    // Click without drag on collapsed chip → toggle expand
                    let st = state.borrow();
                    let sender = st.sender;
                    drop(st);
                    sender.send(Message::TabGroupToggle(gid));
                }
                _ => {}
            }
            false
        }
        Event::Move => {
            let mut st = state.borrow_mut();
            let mx = fltk::app::event_x() - wid.x();
            let my = fltk::app::event_y();

            let hit = hit_test_layout(&st.layout, wid.y(), mx, my);

            let (new_hover, new_close, new_hover_plus, new_hover_group, new_hover_chip) = match hit {
                HitResult::Tab { index, is_close } => (Some(index), is_close, false, None, None),
                HitResult::PlusButton => (None, false, true, None, None),
                HitResult::GroupLabel(gid) => (None, false, false, Some(gid), None),
                HitResult::CollapsedChip(gid) => (None, false, false, None, Some(gid)),
                HitResult::None => (None, false, false, None, None),
            };

            if new_hover != st.hover_tab_index
                || new_close != st.hover_close
                || new_hover_plus != st.hover_plus
                || new_hover_group != st.hover_group_label
                || new_hover_chip != st.hover_collapsed_chip
            {
                st.hover_tab_index = new_hover;
                st.hover_close = new_close;
                st.hover_plus = new_hover_plus;
                st.hover_group_label = new_hover_group;
                st.hover_collapsed_chip = new_hover_chip;
                drop(st);
                wid.redraw();
            }
            true
        }
        Event::Leave => {
            let mut st = state.borrow_mut();
            st.drag_source = None;
            st.drag_target = None;
            if st.hover_tab_index.is_some() || st.hover_close || st.hover_plus
                || st.hover_group_label.is_some() || st.hover_collapsed_chip.is_some()
            {
                st.hover_tab_index = None;
                st.hover_close = false;
                st.hover_plus = false;
                st.hover_group_label = None;
                st.hover_collapsed_chip = None;
                drop(st);
                wid.redraw();
            }
            false
        }
        _ => false,
    }
}
