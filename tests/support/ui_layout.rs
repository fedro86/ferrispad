//! Layout checks for built (not shown) FLTK windows, measured with the
//! fonts actually installed (T0080): labels drawn inside a widget must fit
//! it, also with a larger font, and sibling widgets must not overlap.
//!
//! Use from any integration test binary that builds windows:
//! `mod support; use support::ui_layout::*;`. Building windows opens the
//! display; gate the test with [`display_available`]. fltk-rs pins its UI
//! thread to the first thread that creates a window, so keep all window
//! building of a binary in one `#[test]` function.

use fltk::button::LightButton;
use fltk::prelude::*;
use fltk::widget::Widget;
use fltk::window::Window;

/// Room a label needs on each side inside its widget.
pub const LABEL_PAD: i32 = 4;

/// FLTK alignment bits (`Fl_Align`).
const ALIGN_TOP: i32 = 1;
const ALIGN_BOTTOM: i32 = 2;
const ALIGN_LEFT: i32 = 4;
const ALIGN_RIGHT: i32 = 8;
const ALIGN_INSIDE: i32 = 16;
const ALIGN_CLIP: i32 = 64;
const ALIGN_WRAP: i32 = 128;

/// Whether a display is available. Without one the caller should skip,
/// unless `FERRISPAD_REQUIRE_UI_TESTS=1` (CI), where this panics instead.
pub fn display_available() -> bool {
    let has_display = ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|var| std::env::var_os(var).is_some_and(|v| !v.is_empty()));
    if !has_display {
        assert!(
            std::env::var("FERRISPAD_REQUIRE_UI_TESTS").as_deref() != Ok("1"),
            "UI tests required but no display (set DISPLAY, e.g. run under xvfb-run)"
        );
        eprintln!("skipping UI construction tests: no DISPLAY or WAYLAND_DISPLAY");
    } else {
        // Opens the display, which measuring text needs.
        let _ = fltk::app::screen_size();
    }
    has_display
}

/// Select every widget (for the `only` filters).
pub fn everything(_: &Widget) -> bool {
    true
}

/// The visible children of `group`.
fn children(group: &fltk::group::Group) -> Vec<Widget> {
    (0..group.children())
        .filter_map(|i| group.child(i))
        .filter(|w| w.visible())
        .collect()
}

/// Every visible widget under `group`, with a path for messages, plus each
/// group's visible children as one sibling set. FLTK's own composite
/// widgets (`ColorChooser`) lay out their parts themselves and are not
/// entered.
fn walk(
    group: &fltk::group::Group,
    path: &str,
    widgets: &mut Vec<(String, Widget)>,
    siblings: &mut Vec<(String, Rect, Vec<Widget>)>,
) {
    let kids = children(group);
    // FLTK draws outside labels relative to the parent: a window's own
    // origin is 0,0 for its children (`Fl_Group::draw_outside_label`).
    let parent = if group.as_window().is_some() {
        Rect::new(0, 0, group.w(), group.h())
    } else {
        Rect::new(group.x(), group.y(), group.w(), group.h())
    };
    siblings.push((path.to_string(), parent, kids.clone()));
    for (i, kid) in kids.into_iter().enumerate() {
        let name = format!("{path}/{i}:{:?}", kid.label());
        let composite = fltk::group::ColorChooser::from_dyn_widget(&kid).is_some();
        if let Some(sub) = kid.as_group().filter(|_| !composite) {
            walk(&sub, &name, widgets, siblings);
        }
        widgets.push((name, kid));
    }
}

/// How many visible widgets of `window` pass `only` (to check a filter
/// selects what it should).
pub fn count_matching(window: &Window, only: impl Fn(&Widget) -> bool) -> usize {
    let mut widgets = Vec::new();
    walk(
        &window.as_group().unwrap(),
        "",
        &mut widgets,
        &mut Vec::new(),
    );
    widgets.iter().filter(|(_, w)| only(w)).count()
}

/// Lay out `window` as it would be shown: Flex groups place their children
/// only when laid out.
pub fn lay_out(window: &Window) {
    let mut widgets = Vec::new();
    walk(
        &window.as_group().unwrap(),
        "",
        &mut widgets,
        &mut Vec::new(),
    );
    for (_, w) in widgets {
        if let Some(flex) = fltk::group::Flex::from_dyn_widget(&w) {
            flex.layout();
        }
    }
}

/// Labels drawn inside their widget that do not fit, measured with the
/// widget's label font at its size + `bump_pt` points (+2 approximates a
/// wider desktop font). Only widgets passing `only` are checked. Labels
/// drawn outside the widget, wrapped (`Align::Wrap`) or deliberately
/// clipped (`Align::Clip`) are skipped.
pub fn label_overflows(
    window: &Window,
    title: &str,
    bump_pt: i32,
    only: impl Fn(&Widget) -> bool,
) -> Vec<String> {
    let mut widgets = Vec::new();
    walk(
        &window.as_group().unwrap(),
        title,
        &mut widgets,
        &mut Vec::new(),
    );
    let mut failures = Vec::new();
    for (path, w) in widgets.into_iter().filter(|(_, w)| only(w)) {
        let label = w.label();
        let align = w.align().bits();
        let outside = align & (ALIGN_TOP | ALIGN_BOTTOM | ALIGN_LEFT | ALIGN_RIGHT) != 0
            && align & ALIGN_INSIDE == 0;
        if label.is_empty() || outside || align & (ALIGN_WRAP | ALIGN_CLIP) != 0 {
            continue;
        }
        fltk::draw::set_font(w.label_font(), w.label_size() + bump_pt);
        let (text_w, _) = fltk::draw::measure(&label, false);
        // Check and radio buttons draw their box left of the label.
        let box_w = if LightButton::from_dyn_widget(&w).is_some() {
            w.label_size() + 8
        } else {
            0
        };
        let needed = text_w + 2 * LABEL_PAD + box_w;
        if needed > w.w() {
            failures.push(format!(
                "{path}: label needs {needed}px at +{bump_pt}pt, widget is {}px",
                w.w()
            ));
        }
    }
    failures
}

/// Pairs of visible sibling widgets whose rectangles overlap, where at
/// least one of the two passes `only`.
pub fn sibling_overlaps(
    window: &Window,
    title: &str,
    only: impl Fn(&Widget) -> bool,
) -> Vec<String> {
    let mut siblings = Vec::new();
    walk(
        &window.as_group().unwrap(),
        title,
        &mut Vec::new(),
        &mut siblings,
    );
    let mut failures = Vec::new();
    for (path, _, kids) in siblings {
        for (i, a) in kids.iter().enumerate() {
            for b in &kids[i + 1..] {
                if !(only(a) || only(b)) {
                    continue;
                }
                let x = a.x().max(b.x()) < (a.x() + a.w()).min(b.x() + b.w());
                let y = a.y().max(b.y()) < (a.y() + a.h()).min(b.y() + b.h());
                if x && y && a.w() > 0 && b.w() > 0 && a.h() > 0 && b.h() > 0 {
                    failures.push(format!(
                        "{path}: {:?} {:?} overlaps {:?} {:?}",
                        a.label(),
                        (a.x(), a.y(), a.w(), a.h()),
                        b.label(),
                        (b.x(), b.y(), b.w(), b.h())
                    ));
                }
            }
        }
    }
    failures
}

/// A widget or text rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    fn of(w: &Widget) -> Self {
        Self::new(w.x(), w.y(), w.w(), w.h())
    }

    pub fn intersects(&self, o: &Rect) -> bool {
        self.w > 0
            && self.h > 0
            && o.w > 0
            && o.h > 0
            && self.x.max(o.x) < (self.x + self.w).min(o.x + o.w)
            && self.y.max(o.y) < (self.y + self.h).min(o.y + o.h)
    }

    pub fn contains(&self, o: &Rect) -> bool {
        o.x >= self.x
            && o.y >= self.y
            && o.x + o.w <= self.x + self.w
            && o.y + o.h <= self.y + self.h
    }
}

/// FLTK's gap between a widget and its left/right outside label.
const OUTSIDE_GAP: i32 = 3;
const ALIGN_POSITION_MASK: i32 = 15;
const ALIGN_LEFT_TOP: i32 = 7;
const ALIGN_RIGHT_TOP: i32 = 11;
const ALIGN_LEFT_BOTTOM: i32 = 13;
const ALIGN_RIGHT_BOTTOM: i32 = 14;

/// Where FLTK draws a label that is *outside* its widget, as a text
/// rectangle of `text` (w, h). Mirrors `Fl_Group::draw_outside_label`:
/// FLTK picks a box between the parent's edge and the widget, then aligns
/// the text in it toward the widget, without clipping, so a long label
/// spills out of that box. `None` for an inside or centered label.
pub fn outside_label_rect(
    widget: Rect,
    parent: Rect,
    align: i32,
    text: (i32, i32),
) -> Option<Rect> {
    if align & ALIGN_POSITION_MASK == 0 || align & ALIGN_INSIDE != 0 {
        return None;
    }
    let (tw, th) = text;
    let (wx, wy, ww, wh) = (widget.x, widget.y, widget.w, widget.h);
    // Text placed against the widget, given its box and the alignment
    // FLTK draws it with inside that box.
    let left_of = wx - OUTSIDE_GAP - tw; // right-aligned left of the widget
    let right_of = wx + ww + OUTSIDE_GAP; // left-aligned right of it
    let centered_y = wy + (wh - th) / 2;
    let pos = align & ALIGN_POSITION_MASK;
    let (x, y) = match pos {
        ALIGN_LEFT_TOP => (left_of, wy),
        ALIGN_LEFT_BOTTOM => (left_of, wy + wh - th),
        ALIGN_RIGHT_TOP => (right_of, wy),
        ALIGN_RIGHT_BOTTOM => (right_of, wy + wh - th),
        _ if pos & ALIGN_TOP != 0 || pos & ALIGN_BOTTOM != 0 => {
            let y = if pos & ALIGN_TOP != 0 {
                wy - th
            } else {
                wy + wh
            };
            let x = if pos & ALIGN_LEFT != 0 {
                wx
            } else if pos & ALIGN_RIGHT != 0 {
                wx + ww - tw
            } else {
                wx + (ww - tw) / 2
            };
            (x, y)
        }
        _ if pos & ALIGN_LEFT != 0 => (left_of, centered_y),
        _ => (right_of, centered_y),
    };
    let _ = parent; // the box edge only matters for clipping, which FLTK doesn't do
    Some(Rect::new(x, y, tw, th))
}

/// Labels drawn *outside* their widget (e.g. an `Input`'s default left
/// label) must stay inside the parent group, and must not run into a
/// sibling widget or another outside label. Measured at the label size +
/// `bump_pt`. A pair is checked when either widget passes `only`.
pub fn outside_label_failures(
    window: &Window,
    title: &str,
    bump_pt: i32,
    only: impl Fn(&Widget) -> bool,
) -> Vec<String> {
    let mut siblings = Vec::new();
    walk(
        &window.as_group().unwrap(),
        title,
        &mut Vec::new(),
        &mut siblings,
    );
    let mut failures = Vec::new();
    for (path, parent, kids) in siblings {
        let labels: Vec<(usize, Rect)> = kids
            .iter()
            .enumerate()
            .filter_map(|(i, w)| {
                let label = w.label();
                if label.is_empty() {
                    return None;
                }
                fltk::draw::set_font(w.label_font(), w.label_size() + bump_pt);
                let text = fltk::draw::measure(&label, false);
                outside_label_rect(Rect::of(w), parent, w.align().bits(), text).map(|r| (i, r))
            })
            .collect();
        for &(i, rect) in &labels {
            let owner = &kids[i];
            let name = format!(
                "{path}: outside label {:?} at +{bump_pt}pt {:?}",
                owner.label(),
                rect
            );
            if only(owner) && !parent.contains(&rect) {
                failures.push(format!("{name} leaves its group {parent:?}"));
            }
            for (j, other) in kids.iter().enumerate() {
                if j != i && (only(owner) || only(other)) && rect.intersects(&Rect::of(other)) {
                    failures.push(format!(
                        "{name} runs into {:?} {:?}",
                        other.label(),
                        Rect::of(other)
                    ));
                }
            }
            for &(j, other) in &labels {
                if j > i && (only(owner) || only(&kids[j])) && rect.intersects(&other) {
                    failures.push(format!(
                        "{name} overlaps the label of {:?}",
                        kids[j].label()
                    ));
                }
            }
        }
    }
    failures
}

/// Active buttons whose label is drawn in `dim` (the dialog's dimmed text
/// color): they look disabled while they are not. FLTK dims inactive
/// widgets' labels by itself.
pub fn active_buttons_looking_disabled(
    window: &Window,
    title: &str,
    dim: fltk::enums::Color,
) -> Vec<String> {
    let mut widgets = Vec::new();
    walk(
        &window.as_group().unwrap(),
        title,
        &mut widgets,
        &mut Vec::new(),
    );
    widgets
        .into_iter()
        .filter(|(_, w)| fltk::button::Button::from_dyn_widget(w).is_some())
        .filter(|(_, w)| w.active() && !w.label().is_empty() && w.label_color() == dim)
        .map(|(path, _)| format!("{path}: active button drawn in the dimmed text color"))
        .collect()
}

/// Widgets with tooltip `tooltip` drawn with a flat or no frame (a color
/// swatch must keep a visible border).
pub fn flat_widgets_with_tooltip(window: &Window, title: &str, tooltip: &str) -> Vec<String> {
    use fltk::enums::FrameType;
    let mut widgets = Vec::new();
    walk(
        &window.as_group().unwrap(),
        title,
        &mut widgets,
        &mut Vec::new(),
    );
    widgets
        .into_iter()
        .filter(|(_, w)| w.tooltip().as_deref() == Some(tooltip))
        .filter(|(_, w)| matches!(w.frame(), FrameType::FlatBox | FrameType::NoBox))
        .map(|(path, _)| format!("{path}: drawn without a border"))
        .collect()
}

/// Every layout failure of `window` for the widgets passing `only`:
/// labels at the normal size and at +2 pt (inside and outside labels), and
/// sibling overlaps.
pub fn layout_failures(
    window: &Window,
    title: &str,
    only: impl Fn(&Widget) -> bool + Copy,
) -> Vec<String> {
    lay_out(window);
    let mut failures = label_overflows(window, title, 0, only);
    failures.extend(label_overflows(window, title, 2, only));
    failures.extend(outside_label_failures(window, title, 0, only));
    failures.extend(outside_label_failures(window, title, 2, only));
    failures.extend(sibling_overlaps(window, title, only));
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Rect = Rect::new(100, 50, 60, 20); // the widget
    const P: Rect = Rect::new(0, 0, 400, 300); // its parent

    #[test]
    fn outside_label_rect_sides() {
        let t = (40, 14);
        // Left (Input's default): right-aligned 3 px left, centered.
        assert_eq!(
            outside_label_rect(W, P, ALIGN_LEFT, t),
            Some(Rect::new(57, 53, 40, 14))
        );
        // Right: 3 px right of the widget, centered.
        assert_eq!(
            outside_label_rect(W, P, ALIGN_RIGHT, t),
            Some(Rect::new(163, 53, 40, 14))
        );
        // Top / bottom: centered over / under the widget.
        assert_eq!(
            outside_label_rect(W, P, ALIGN_TOP, t),
            Some(Rect::new(110, 36, 40, 14))
        );
        assert_eq!(
            outside_label_rect(W, P, ALIGN_BOTTOM, t),
            Some(Rect::new(110, 70, 40, 14))
        );
        // Top-left / bottom-right: flush with that widget edge.
        assert_eq!(
            outside_label_rect(W, P, ALIGN_TOP | ALIGN_LEFT, t),
            Some(Rect::new(100, 36, 40, 14))
        );
        assert_eq!(
            outside_label_rect(W, P, ALIGN_BOTTOM | ALIGN_RIGHT, t),
            Some(Rect::new(120, 70, 40, 14))
        );
        // FLTK's LEFT_TOP etc.: beside the widget, at its top / bottom.
        assert_eq!(
            outside_label_rect(W, P, ALIGN_LEFT_TOP, t),
            Some(Rect::new(57, 50, 40, 14))
        );
        assert_eq!(
            outside_label_rect(W, P, ALIGN_RIGHT_BOTTOM, t),
            Some(Rect::new(163, 56, 40, 14))
        );
    }

    #[test]
    fn outside_label_rect_skips_inside_and_centered() {
        assert_eq!(
            outside_label_rect(W, P, ALIGN_LEFT | ALIGN_INSIDE, (40, 14)),
            None
        );
        assert_eq!(outside_label_rect(W, P, 0, (40, 14)), None);
    }

    #[test]
    fn outside_label_rect_spills_and_collides() {
        // A 120 px left label next to a widget at x=100 starts at -23:
        // outside the parent, and over a sibling sitting at x=0..50.
        let r = outside_label_rect(W, P, ALIGN_LEFT, (120, 14)).unwrap();
        assert_eq!(r.x, 100 - 3 - 120);
        assert!(!P.contains(&r));
        assert!(r.intersects(&Rect::new(0, 50, 50, 20)));
        assert!(
            !r.intersects(&Rect::new(0, 80, 50, 20)),
            "a row below is untouched"
        );
    }
}
