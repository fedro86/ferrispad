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
    siblings: &mut Vec<(String, Vec<Widget>)>,
) {
    let kids = children(group);
    siblings.push((path.to_string(), kids.clone()));
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
    for (path, kids) in siblings {
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
/// labels at the normal size and at +2 pt, and sibling overlaps.
pub fn layout_failures(
    window: &Window,
    title: &str,
    only: impl Fn(&Widget) -> bool + Copy,
) -> Vec<String> {
    lay_out(window);
    let mut failures = label_overflows(window, title, 0, only);
    failures.extend(label_overflows(window, title, 2, only));
    failures.extend(sibling_overlaps(window, title, only));
    failures
}
