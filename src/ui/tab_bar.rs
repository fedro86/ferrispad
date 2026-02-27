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

use crate::app::{Document, DocumentId, GroupColor, GroupId, Message, TabGroup};

pub const TAB_BAR_HEIGHT: i32 = 32;

const MIN_TAB_WIDTH: i32 = 60;
const MAX_TAB_WIDTH: i32 = 200;
const CLOSE_BTN_SIZE: i32 = 14;
const CLOSE_BTN_MARGIN: i32 = 6;
const TAB_H_PADDING: i32 = 10;
const CORNER_RADIUS: i32 = 6;
const TAB_GAP: i32 = 1;
const PLUS_BTN_WIDTH: i32 = 28;
const PLUS_BTN_MARGIN: i32 = 4;

const SCROLL_ARROW_WIDTH: i32 = 24;
const SCROLL_ARROW_MARGIN: i32 = 6;

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
    ScrollLeft { x: i32, enabled: bool },
    ScrollRight { x: i32, enabled: bool },
}

enum HitResult {
    Tab { index: usize, is_close: bool },
    GroupLabel(GroupId),
    CollapsedChip(GroupId),
    PlusButton,
    ScrollLeft,
    ScrollRight,
    None,
}

#[derive(Clone, Copy, PartialEq)]
enum DragSource {
    Tab(usize),          // dragging a single tab
    Group(GroupId),      // dragging a collapsed group chip
}

#[derive(Clone, Copy, PartialEq)]
enum DragTarget {
    OnTab(usize),           // center zone → will group
    InsertAt(usize),        // edge zone → will reorder (index = insertion point)
    OnCollapsedGroup(GroupId), // dropping onto a collapsed group chip → join that group
}

/// RGB color tuple for theme colors
#[derive(Clone, Copy)]
pub struct ThemeRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ThemeRgb {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn from_tuple(rgb: (u8, u8, u8)) -> Self {
        Self { r: rgb.0, g: rgb.1, b: rgb.2 }
    }

    /// Calculate brightness (0-255)
    pub fn brightness(&self) -> u8 {
        ((self.r as u32 + self.g as u32 + self.b as u32) / 3) as u8
    }

    /// Darken color by a factor (0.0 = black, 1.0 = unchanged)
    pub fn darken(&self, factor: f32) -> Self {
        Self {
            r: (self.r as f32 * factor) as u8,
            g: (self.g as f32 * factor) as u8,
            b: (self.b as f32 * factor) as u8,
        }
    }

    /// Lighten color towards white by a factor (0.0 = unchanged, 1.0 = white)
    pub fn lighten(&self, factor: f32) -> Self {
        Self {
            r: self.r + ((255 - self.r) as f32 * factor) as u8,
            g: self.g + ((255 - self.g) as f32 * factor) as u8,
            b: self.b + ((255 - self.b) as f32 * factor) as u8,
        }
    }

    /// Blend towards another color
    pub fn blend(&self, other: &Self, factor: f32) -> Self {
        Self {
            r: (self.r as f32 + (other.r as f32 - self.r as f32) * factor) as u8,
            g: (self.g as f32 + (other.g as f32 - self.g as f32) * factor) as u8,
            b: (self.b as f32 + (other.b as f32 - self.b as f32) * factor) as u8,
        }
    }

    pub fn to_fltk(&self) -> Color {
        Color::from_rgb(self.r, self.g, self.b)
    }
}

struct TabBarState {
    tabs: Vec<TabInfo>,
    groups: Vec<GroupInfo>,
    layout: Vec<LayoutItem>,
    is_dark: bool,
    /// Editor background color from syntax theme
    theme_bg: ThemeRgb,
    hover_tab_index: Option<usize>,
    hover_close: bool,
    hover_plus: bool,
    hover_group_label: Option<GroupId>,
    hover_collapsed_chip: Option<GroupId>,
    hover_scroll_left: bool,
    hover_scroll_right: bool,
    drag_source: Option<DragSource>,
    drag_target: Option<DragTarget>,
    sender: Sender<Message>,
    widget_w: i32,
    /// Index of first visible layout item (for scroll)
    scroll_offset: usize,
    /// Total number of scrollable items (tabs + collapsed chips)
    total_items: usize,
    /// Number of items visible in current layout
    visible_items: usize,
    /// Raw pointer to a pre-created MenuButton for context menus (Wayland-friendly)
    ctx_menu_ptr: fltk::app::WidgetPtr,
}

pub struct TabBar {
    pub widget: Widget,
    state: Rc<RefCell<TabBarState>>,
}

impl TabBar {
    pub fn new(x: i32, y: i32, w: i32, sender: Sender<Message>) -> Self {
        // Pre-create a hidden MenuButton for context menus.
        // Parented to the current group (main window) so Wayland's
        // xdg_positioner can anchor popups to the correct surface.
        let mut ctx_menu = MenuButton::new(1, 1, 1, 1, None);
        ctx_menu.hide();
        let ctx_menu_ptr = ctx_menu.as_widget_ptr();

        let state = Rc::new(RefCell::new(TabBarState {
            tabs: Vec::new(),
            groups: Vec::new(),
            layout: Vec::new(),
            is_dark: false,
            theme_bg: ThemeRgb::new(255, 255, 255), // Default to white
            hover_tab_index: None,
            hover_close: false,
            hover_plus: false,
            hover_group_label: None,
            hover_collapsed_chip: None,
            hover_scroll_left: false,
            hover_scroll_right: false,
            drag_source: None,
            drag_target: None,
            sender,
            widget_w: w,
            scroll_offset: 0,
            total_items: 0,
            visible_items: 0,
            ctx_menu_ptr,
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
        theme_bg: (u8, u8, u8),
    ) {
        let mut st = self.state.borrow_mut();
        st.is_dark = is_dark;
        st.theme_bg = ThemeRgb::from_tuple(theme_bg);
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

    pub fn apply_theme(&mut self, is_dark: bool, theme_bg: (u8, u8, u8)) {
        let mut st = self.state.borrow_mut();
        st.is_dark = is_dark;
        st.theme_bg = ThemeRgb::from_tuple(theme_bg);
        drop(st);
        self.widget.redraw();
    }

    /// Ensure the active tab is visible by adjusting scroll offset.
    /// Call this after TabSwitch to auto-scroll to the active tab.
    pub fn ensure_active_visible(&mut self, active_id: Option<DocumentId>) {
        let Some(active_id) = active_id else { return };

        let mut st = self.state.borrow_mut();

        // Find the scrollable item index for this tab
        let mut scrollable_idx = 0;
        let mut i = 0;
        let mut current_group: Option<GroupId> = None;

        while i < st.tabs.len() {
            let tab_group = st.tabs[i].group_id;

            if tab_group != current_group {
                current_group = tab_group;

                if let Some(gid) = tab_group
                    && let Some(ginfo) = st.groups.iter().find(|g| g.id == gid)
                    && ginfo.collapsed {
                        // This is a collapsed group - count all tabs in it
                        let count = st.tabs[i..].iter().take_while(|t| t.group_id == Some(gid)).count();
                        // Check if active tab is in this collapsed group
                        let in_group = st.tabs[i..i+count].iter().any(|t| t.id == active_id);
                        if in_group {
                            // Scroll to show this collapsed chip
                            if scrollable_idx < st.scroll_offset {
                                st.scroll_offset = scrollable_idx;
                            } else if scrollable_idx >= st.scroll_offset + st.visible_items && st.visible_items > 0 {
                                st.scroll_offset = scrollable_idx.saturating_sub(st.visible_items - 1);
                            }
                            compute_layout(&mut st);
                            drop(st);
                            self.widget.redraw();
                            return;
                        }
                        scrollable_idx += 1;
                        i += count;
                        continue;
                    }
            }

            // Check if this is the active tab
            if st.tabs[i].id == active_id {
                // Scroll to show this tab
                if scrollable_idx < st.scroll_offset {
                    st.scroll_offset = scrollable_idx;
                } else if scrollable_idx >= st.scroll_offset + st.visible_items && st.visible_items > 0 {
                    st.scroll_offset = scrollable_idx.saturating_sub(st.visible_items - 1);
                }
                compute_layout(&mut st);
                drop(st);
                self.widget.redraw();
                return;
            }

            scrollable_idx += 1;
            i += 1;
        }
    }

    /// Handle window resize - recalculate layout
    pub fn handle_resize(&mut self) {
        let mut st = self.state.borrow_mut();
        let new_w = self.widget.w();
        if new_w != st.widget_w {
            st.widget_w = new_w;
            // Clamp scroll offset if window got larger
            let max_offset = st.total_items.saturating_sub(1);
            if st.scroll_offset > max_offset {
                st.scroll_offset = max_offset;
            }
            compute_layout(&mut st);
            drop(st);
            self.widget.redraw();
        }
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

/// Represents a scrollable item (tab or collapsed group chip) for overflow calculation
#[derive(Clone)]
struct ScrollableItem {
    layout_item: LayoutItem,
    /// Width needed for this item (tab width or chip width)
    width: i32,
    /// Whether this is a tab (true) or collapsed chip (false)
    is_tab: bool,
    /// Group label that precedes this item (if any)
    group_label: Option<LayoutItem>,
    /// Width of the group label (if any)
    group_label_width: i32,
}

fn compute_layout(st: &mut TabBarState) {
    st.layout.clear();
    let widget_w = st.widget_w;

    if st.tabs.is_empty() {
        st.layout.push(LayoutItem::PlusButton { x: PLUS_BTN_MARGIN });
        st.total_items = 0;
        st.visible_items = 0;
        st.scroll_offset = 0;
        return;
    }

    // First pass: build scrollable items (tabs + collapsed chips) with their group labels
    let mut scrollable_items: Vec<ScrollableItem> = Vec::new();
    let mut current_group: Option<GroupId> = None;
    let mut i = 0;

    while i < st.tabs.len() {
        let tab_group = st.tabs[i].group_id;

        let mut grp_label: Option<LayoutItem> = None;
        let mut grp_label_w: i32 = 0;

        if tab_group != current_group {
            current_group = tab_group;

            if let Some(gid) = tab_group
                && let Some(ginfo) = st.groups.iter().find(|g| g.id == gid) {
                    if ginfo.collapsed {
                        // Collapsed group is a single scrollable item
                        let count = st.tabs[i..].iter().take_while(|t| t.group_id == Some(gid)).count();
                        let cw = collapsed_chip_width(&ginfo.name, count);
                        scrollable_items.push(ScrollableItem {
                            layout_item: LayoutItem::CollapsedChip {
                                group_id: gid,
                                x: 0,
                                width: cw,
                                count,
                                name: ginfo.name.clone(),
                                color: ginfo.color,
                            },
                            width: cw,
                            is_tab: false,
                            group_label: None,
                            group_label_width: 0,
                        });
                        i += count;
                        continue;
                    } else {
                        // Group label precedes the first tab of this group
                        let lw = group_label_width(&ginfo.name);
                        grp_label = Some(LayoutItem::GroupLabel {
                            group_id: gid,
                            x: 0,
                            width: lw,
                        });
                        grp_label_w = lw + GROUP_LABEL_GAP;
                    }
                }
        }

        // Tab is a scrollable item
        scrollable_items.push(ScrollableItem {
            layout_item: LayoutItem::Tab {
                index: i,
                x: 0,
                width: 0,
            },
            width: 0, // Will be computed later
            is_tab: true,
            group_label: grp_label,
            group_label_width: grp_label_w,
        });
        i += 1;
    }

    st.total_items = scrollable_items.len();

    // Calculate space needed for plus button
    let plus_btn_space = PLUS_BTN_WIDTH + PLUS_BTN_MARGIN * 2;

    // Check if we need scroll arrows
    // First, compute minimum width needed for all items at MIN_TAB_WIDTH
    let min_total_width: i32 = scrollable_items.iter().map(|item| {
        if item.is_tab {
            item.group_label_width + MIN_TAB_WIDTH + TAB_GAP
        } else {
            item.width + TAB_GAP
        }
    }).sum::<i32>() + plus_btn_space;

    let needs_scroll = min_total_width > widget_w;

    // Reserve space for arrows if needed
    // Left arrow: margin + width + margin (gap to tabs)
    // Right arrow: margin + width + margin (gap to plus button)
    let arrow_space = if needs_scroll {
        (SCROLL_ARROW_MARGIN + SCROLL_ARROW_WIDTH + SCROLL_ARROW_MARGIN) * 2
    } else {
        0
    };

    // Clamp scroll_offset to valid range
    // If scrolling is no longer needed, reset to 0 to show all tabs
    if !needs_scroll {
        st.scroll_offset = 0;
    } else if st.scroll_offset >= scrollable_items.len() {
        st.scroll_offset = scrollable_items.len().saturating_sub(1);
    }

    // Calculate available width for tabs (excluding arrows and plus button)
    let available_for_tabs = widget_w - arrow_space - plus_btn_space;

    // Compute tab width and visible count
    let (tab_width, visible_count) = if !needs_scroll {
        // No scrolling needed - all items will be shown, calculate tab width to fit them all
        let total_tabs: i32 = scrollable_items.iter().filter(|i| i.is_tab).count() as i32;
        let fixed_width: i32 = scrollable_items.iter().map(|item| {
            if item.is_tab {
                item.group_label_width + TAB_GAP
            } else {
                item.width + TAB_GAP
            }
        }).sum();

        let tw = if total_tabs > 0 {
            let available_for_tab_bodies = available_for_tabs - fixed_width;
            (available_for_tab_bodies / total_tabs).clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH)
        } else {
            MAX_TAB_WIDTH
        };
        (tw, scrollable_items.len())
    } else {
        // Scrolling needed - calculate how many items fit starting from scroll_offset
        // First, estimate tab width based on visible range
        let visible_tabs_in_range: i32 = scrollable_items.iter().skip(st.scroll_offset).filter(|i| i.is_tab).count() as i32;
        let fixed_in_range: i32 = scrollable_items.iter().skip(st.scroll_offset).map(|item| {
            if item.is_tab {
                item.group_label_width
            } else {
                item.width + TAB_GAP
            }
        }).sum();

        let tw = if visible_tabs_in_range > 0 {
            let available_for_tab_bodies = available_for_tabs - fixed_in_range - (visible_tabs_in_range - 1).max(0) * TAB_GAP;
            (available_for_tab_bodies / visible_tabs_in_range).clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH)
        } else {
            MAX_TAB_WIDTH
        };

        // Now determine how many items actually fit
        let mut count = 0;
        let mut used_width = 0;
        for item in scrollable_items.iter().skip(st.scroll_offset) {
            let item_width = if item.is_tab {
                item.group_label_width + tw + TAB_GAP
            } else {
                item.width + TAB_GAP
            };

            if used_width + item_width > available_for_tabs && count > 0 {
                break;
            }
            used_width += item_width;
            count += 1;
        }
        (tw, count)
    };

    st.visible_items = visible_count;

    // Build final layout
    // When scrolling: left margin + arrow width + right margin (gap to first tab)
    let mut cursor_x = if needs_scroll {
        SCROLL_ARROW_MARGIN + SCROLL_ARROW_WIDTH + SCROLL_ARROW_MARGIN
    } else {
        0
    };

    // Add scroll left arrow if needed
    if needs_scroll {
        st.layout.push(LayoutItem::ScrollLeft {
            x: SCROLL_ARROW_MARGIN,
            enabled: st.scroll_offset > 0,
        });
    }

    // Add visible items
    for item in scrollable_items.iter().skip(st.scroll_offset).take(visible_count) {
        // Add group label if present
        if let Some(ref label) = item.group_label {
            if let LayoutItem::GroupLabel { group_id, width, .. } = label {
                st.layout.push(LayoutItem::GroupLabel {
                    group_id: *group_id,
                    x: cursor_x,
                    width: *width,
                });
                cursor_x += *width + GROUP_LABEL_GAP;
            }
        }

        // Add the item itself
        match &item.layout_item {
            LayoutItem::Tab { index, .. } => {
                st.layout.push(LayoutItem::Tab {
                    index: *index,
                    x: cursor_x,
                    width: tab_width,
                });
                cursor_x += tab_width + TAB_GAP;
            }
            LayoutItem::CollapsedChip { group_id, width, count, name, color, .. } => {
                st.layout.push(LayoutItem::CollapsedChip {
                    group_id: *group_id,
                    x: cursor_x,
                    width: *width,
                    count: *count,
                    name: name.clone(),
                    color: *color,
                });
                cursor_x += *width + TAB_GAP;
            }
            _ => {}
        }
    }

    // Add scroll right arrow if needed - anchored to the right side
    if needs_scroll {
        let can_scroll_right = st.scroll_offset + visible_count < scrollable_items.len();
        // Right arrow position: widget_w - plus_btn_space - arrow_width - margin
        let right_arrow_x = widget_w - plus_btn_space - SCROLL_ARROW_WIDTH - SCROLL_ARROW_MARGIN;
        st.layout.push(LayoutItem::ScrollRight {
            x: right_arrow_x,
            enabled: can_scroll_right,
        });
    }

    // Plus button anchored to the right edge
    let plus_x = widget_w - PLUS_BTN_WIDTH - PLUS_BTN_MARGIN;
    st.layout.push(LayoutItem::PlusButton { x: plus_x });
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
            LayoutItem::ScrollLeft { x, .. } => {
                if mx >= *x && mx < *x + SCROLL_ARROW_WIDTH {
                    return HitResult::ScrollLeft;
                }
            }
            LayoutItem::ScrollRight { x, .. } => {
                if mx >= *x && mx < *x + SCROLL_ARROW_WIDTH {
                    return HitResult::ScrollRight;
                }
            }
        }
    }
    HitResult::None
}

// --- Colors ---

/// Theme colors for tab bar and menu bar styling.
pub struct ThemeColors {
    pub bar_bg: Color,
    pub active_bg: Color,
    pub inactive_bg: Color,
    pub active_text: Color,
    pub inactive_text: Color,
    pub close_hover_bg: Color,
}

/// Calculate tab bar colors based on the editor's syntax theme background.
/// - Active tab: matches editor background (seamless transition)
/// - Bar background: slightly darker/lighter than editor
/// - Inactive tab: between active and bar background
///
/// Also used by menu bar styling to maintain visual consistency.
pub fn theme_colors_from_bg(theme_bg: &ThemeRgb) -> ThemeColors {
    let is_dark = theme_bg.brightness() < 128;

    // Active tab = editor background (creates seamless connection)
    let active_bg = theme_bg.to_fltk();

    // Bar background: darken for light themes, darken more for dark themes
    let bar_bg_rgb = if is_dark {
        theme_bg.darken(0.65) // Darker than editor
    } else {
        theme_bg.darken(0.85) // Slightly darker
    };
    let bar_bg = bar_bg_rgb.to_fltk();

    // Inactive tab: blend between active (editor) and bar background
    let inactive_bg_rgb = theme_bg.blend(&bar_bg_rgb, 0.5);
    let inactive_bg = inactive_bg_rgb.to_fltk();

    // Text colors based on brightness
    let (active_text, inactive_text) = if is_dark {
        (
            Color::from_rgb(230, 230, 230),
            Color::from_rgb(140, 140, 140),
        )
    } else {
        (
            Color::from_rgb(0, 0, 0),
            Color::from_rgb(80, 80, 80),
        )
    };

    // Close button hover: slightly different from inactive
    let close_hover_bg = if is_dark {
        theme_bg.lighten(0.15).to_fltk()
    } else {
        theme_bg.darken(0.78).to_fltk()
    };

    ThemeColors {
        bar_bg,
        active_bg,
        inactive_bg,
        active_text,
        inactive_text,
        close_hover_bg,
    }
}

#[allow(dead_code)] // Keep for reference/fallback
fn theme_colors(is_dark: bool) -> ThemeColors {
    if is_dark {
        ThemeColors {
            bar_bg: Color::from_rgb(20, 20, 20),
            active_bg: Color::from_rgb(30, 30, 30),
            inactive_bg: Color::from_rgb(25, 25, 25),
            active_text: Color::from_rgb(230, 230, 230),
            inactive_text: Color::from_rgb(140, 140, 140),
            close_hover_bg: Color::from_rgb(50, 50, 50),
        }
    } else {
        ThemeColors {
            bar_bg: Color::from_rgb(215, 215, 215),
            active_bg: Color::from_rgb(255, 255, 255),
            inactive_bg: Color::from_rgb(235, 235, 235),
            active_text: Color::from_rgb(0, 0, 0),
            inactive_text: Color::from_rgb(80, 80, 80),
            close_hover_bg: Color::from_rgb(200, 200, 200),
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
    let colors = theme_colors_from_bg(&st.theme_bg);

    // Background
    draw::set_draw_color(colors.bar_bg);
    draw::draw_rectf(wx, wy, ww, wh);

    for item in &st.layout {
        match item {
            LayoutItem::Tab { index, x, width } => {
                let tx = wx + *x;
                let tab_width = *width;
                let tab = &st.tabs[*index];

                // Tab background (2px gap at top for menu bar separation)
                if tab.is_active {
                    draw_rounded_top_rect(tx, wy + 2, tab_width, wh - 2, CORNER_RADIUS, colors.active_bg);
                } else {
                    let inactive_h = wh - 4;
                    draw_rounded_top_rect(tx, wy + 4, tab_width, inactive_h, CORNER_RADIUS, colors.inactive_bg);
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
                let circle_size = 22;
                let circle_x = px + (PLUS_BTN_WIDTH - circle_size) / 2;
                let circle_y = wy + (wh - circle_size) / 2;
                // Use active tab color as base, with slight adjustment for hover
                let bg = if st.hover_plus {
                    colors.close_hover_bg
                } else {
                    colors.active_bg
                };
                draw::set_draw_color(bg);
                draw::draw_pie(circle_x, circle_y, circle_size, circle_size, 0.0, 360.0);
                let text_color = if st.hover_plus {
                    colors.active_text
                } else {
                    colors.inactive_text
                };
                draw::set_draw_color(text_color);
                draw::set_font(Font::HelveticaBold, 16);
                draw::draw_text2("+", circle_x + 1, circle_y, circle_size, circle_size, Align::Center);
            }
            LayoutItem::ScrollLeft { x, enabled } => {
                let ax = wx + *x;
                let arrow_h = wh - 8;
                let arrow_y = wy + 4;
                let is_hover = st.hover_scroll_left && *enabled;

                // Background
                let bg = if is_hover {
                    colors.close_hover_bg
                } else {
                    colors.inactive_bg
                };
                draw_rounded_rect(ax, arrow_y, SCROLL_ARROW_WIDTH, arrow_h, 4, bg);

                // Arrow symbol
                let text_color = if *enabled {
                    if is_hover { colors.active_text } else { colors.inactive_text }
                } else {
                    // Disabled: very faded
                    Color::from_rgb(100, 100, 100)
                };
                draw::set_draw_color(text_color);
                draw::set_font(Font::HelveticaBold, 14);
                draw::draw_text2("\u{25c0}", ax, arrow_y, SCROLL_ARROW_WIDTH, arrow_h, Align::Center);
            }
            LayoutItem::ScrollRight { x, enabled } => {
                let ax = wx + *x;
                let arrow_h = wh - 8;
                let arrow_y = wy + 4;
                let is_hover = st.hover_scroll_right && *enabled;

                // Background
                let bg = if is_hover {
                    colors.close_hover_bg
                } else {
                    colors.inactive_bg
                };
                draw_rounded_rect(ax, arrow_y, SCROLL_ARROW_WIDTH, arrow_h, 4, bg);

                // Arrow symbol
                let text_color = if *enabled {
                    if is_hover { colors.active_text } else { colors.inactive_text }
                } else {
                    Color::from_rgb(100, 100, 100)
                };
                draw::set_draw_color(text_color);
                draw::set_font(Font::HelveticaBold, 14);
                draw::draw_text2("\u{25b6}", ax, arrow_y, SCROLL_ARROW_WIDTH, arrow_h, Align::Center);
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
            DragTarget::OnCollapsedGroup(gid) => {
                // Highlight the collapsed chip with a blue tint
                if let Some(item) = st.layout.iter().find(|it| matches!(it, LayoutItem::CollapsedChip { group_id, .. } if group_id == gid))
                    && let LayoutItem::CollapsedChip { x, width, .. } = item {
                        let bg = colors.inactive_bg;
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
                        draw_rounded_rect(wx + *x, wy + 4, *width, wh - 6, CORNER_RADIUS, blended);
                    }
            }
        }
    }
}

// --- Context menu ---

/// Build the context menu (adds items) but does NOT call popup().
/// The caller must drop any RefCell borrows before calling popup()
/// because popup() runs a nested event loop that may re-enter the handler.
fn build_context_menu(st: &TabBarState, tab_index: Option<usize>, group_id: Option<GroupId>) -> MenuButton {
    let sender = st.sender;
    let mx = fltk::app::event_x();
    let my = fltk::app::event_y();
    // Reuse the pre-created MenuButton (parented to main window) so Wayland's
    // xdg_positioner can anchor the popup to the correct surface.
    let mut menu = unsafe { MenuButton::from_widget_ptr(st.ctx_menu_ptr) };
    menu.clear();
    menu.resize(mx, my, 1, 1);
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

    menu
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
                        // Right-click context menu — build menu while borrowed,
                        // then drop borrow before popup() (nested event loop).
                        let group_id = st.tabs[index].group_id;
                        drop(st);
                        let st2 = state.borrow();
                        let mut menu = build_context_menu(&st2, Some(index), group_id);
                        drop(st2);
                        menu.popup();
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
                        let mut menu = build_context_menu(&st2, None, Some(gid));
                        drop(st2);
                        menu.popup();
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
                        let mut menu = build_context_menu(&st2, None, Some(gid));
                        drop(st2);
                        menu.popup();
                    }
                    true
                }
                HitResult::ScrollLeft if button == 1 => {
                    drop(st);
                    let mut st_mut = state.borrow_mut();
                    if st_mut.scroll_offset > 0 {
                        st_mut.scroll_offset -= 1;
                        compute_layout(&mut st_mut);
                        drop(st_mut);
                        wid.redraw();
                    }
                    true
                }
                HitResult::ScrollRight if button == 1 => {
                    drop(st);
                    let mut st_mut = state.borrow_mut();
                    let max_offset = st_mut.total_items.saturating_sub(st_mut.visible_items);
                    if st_mut.scroll_offset < max_offset {
                        st_mut.scroll_offset += 1;
                        compute_layout(&mut st_mut);
                        drop(st_mut);
                        wid.redraw();
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
                        HitResult::CollapsedChip(gid) => {
                            // Check if source tab is not already in this group
                            let src_group = st.tabs.get(src_idx).and_then(|t| t.group_id);
                            if src_group != Some(gid) {
                                Some(DragTarget::OnCollapsedGroup(gid))
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
                            // Check if we're inserting between tabs of the same group
                            // If so, the dragged tab should join that group
                            let source_tab = &st.tabs[from];
                            let source_id = source_tab.id;

                            // Get group of tab before and after insertion point (excluding source)
                            let group_before = if to > 0 && to - 1 != from {
                                st.tabs.get(to - 1).and_then(|t| t.group_id)
                            } else if to > 1 && to - 1 == from {
                                // Skip source, check one more before
                                st.tabs.get(to - 2).and_then(|t| t.group_id)
                            } else {
                                None
                            };

                            let group_after = if to < st.tabs.len() && to != from {
                                st.tabs.get(to).and_then(|t| t.group_id)
                            } else if to + 1 < st.tabs.len() && to == from {
                                // Skip source, check one more after
                                st.tabs.get(to + 1).and_then(|t| t.group_id)
                            } else {
                                None
                            };

                            // Determine target group:
                            // Only join a group if dropping BETWEEN two tabs of the SAME group
                            // Dropping at the edge of a group should NOT auto-join
                            let target_group = match (group_before, group_after) {
                                (Some(gb), Some(ga)) if gb == ga => Some(gb),
                                _ => None,
                            };

                            // Get source group for comparison
                            let source_group = source_tab.group_id;

                            drop(st);

                            if from != to {
                                // Use atomic move+group operation to avoid clamp issues
                                if let Some(target_gid) = target_group {
                                    if source_group != Some(target_gid) {
                                        // Moving to a different group - use atomic operation
                                        sender.send(Message::TabMoveToGroup(source_id, to, Some(target_gid)));
                                    } else {
                                        // Same group - just move
                                        sender.send(Message::TabMove(from, to));
                                    }
                                } else {
                                    // No target group
                                    if source_group.is_some() {
                                        // Leaving a group - use atomic operation to remove from group
                                        sender.send(Message::TabMoveToGroup(source_id, to, None));
                                    } else {
                                        // Not in a group, not joining one - simple move
                                        sender.send(Message::TabMove(from, to));
                                    }
                                }
                            }
                        }
                        (DragSource::Tab(from), DragTarget::OnCollapsedGroup(gid)) => {
                            // Dropping a tab onto a collapsed group → add to that group
                            let source_tab = &st.tabs[from];
                            let source_id = source_tab.id;
                            // Find the last tab in the target group to insert after it
                            let last_in_group = st.tabs.iter().rposition(|t| t.group_id == Some(gid));
                            let insert_pos = last_in_group.map_or(from, |i| i + 1);
                            drop(st);
                            // Use atomic operation to move and join group
                            sender.send(Message::TabMoveToGroup(source_id, insert_pos, Some(gid)));
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

            let (new_hover, new_close, new_hover_plus, new_hover_group, new_hover_chip, new_scroll_left, new_scroll_right) = match hit {
                HitResult::Tab { index, is_close } => (Some(index), is_close, false, None, None, false, false),
                HitResult::PlusButton => (None, false, true, None, None, false, false),
                HitResult::GroupLabel(gid) => (None, false, false, Some(gid), None, false, false),
                HitResult::CollapsedChip(gid) => (None, false, false, None, Some(gid), false, false),
                HitResult::ScrollLeft => (None, false, false, None, None, true, false),
                HitResult::ScrollRight => (None, false, false, None, None, false, true),
                HitResult::None => (None, false, false, None, None, false, false),
            };

            if new_hover != st.hover_tab_index
                || new_close != st.hover_close
                || new_hover_plus != st.hover_plus
                || new_hover_group != st.hover_group_label
                || new_hover_chip != st.hover_collapsed_chip
                || new_scroll_left != st.hover_scroll_left
                || new_scroll_right != st.hover_scroll_right
            {
                st.hover_tab_index = new_hover;
                st.hover_close = new_close;
                st.hover_plus = new_hover_plus;
                st.hover_group_label = new_hover_group;
                st.hover_collapsed_chip = new_hover_chip;
                st.hover_scroll_left = new_scroll_left;
                st.hover_scroll_right = new_scroll_right;
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
                || st.hover_scroll_left || st.hover_scroll_right
            {
                st.hover_tab_index = None;
                st.hover_close = false;
                st.hover_plus = false;
                st.hover_group_label = None;
                st.hover_collapsed_chip = None;
                st.hover_scroll_left = false;
                st.hover_scroll_right = false;
                drop(st);
                wid.redraw();
            }
            false
        }
        _ => false,
    }
}
