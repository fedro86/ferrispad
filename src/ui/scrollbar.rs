//! Access to the built-in scrollbars of FLTK composite widgets.
//!
//! `TextDisplay`/`TextEditor` (C++ `Fl_Text_Display`) and `Tree` (`Fl_Tree`)
//! own their scrollbars as plain `Fl_Group` children, but fltk-rs exposes no
//! accessor for them. This module is the single place that reaches them.
//!
//! It needs no `unsafe`: the widget is cast to a `Group` and each child to a
//! `Scrollbar` through fltk-rs's checked `from_dyn_widget*` casts (C++
//! `dynamic_cast`), so a widget or child of the wrong type yields `None`
//! instead of a mistyped handle.
//!
//! # Child-index invariant
//!
//! The children are in construction order (FLTK 1.4, as bundled by fltk-sys):
//!
//! | Widget            | child 0              | child 1              |
//! |-------------------|----------------------|----------------------|
//! | `Fl_Text_Display` | horizontal scrollbar | vertical scrollbar   |
//! | `Fl_Tree`         | vertical scrollbar   | horizontal scrollbar |
//!
//! (`Fl_Text_Display::Fl_Text_Display` creates `mHScrollBar` before
//! `mVScrollBar`; `Fl_Tree::Fl_Tree` creates `_vscroll` before `_hscroll`.)
//! The order differs per widget, so only the text-display lookup is
//! orientation-specific, and it is bounded on [`DisplayExt`] so it cannot be
//! called on a `Tree`. The unit tests below pin both rows of the table.

use fltk::{
    enums::{Color, FrameType},
    group::Group,
    prelude::*,
    valuator::Scrollbar,
};

/// Index of the vertical scrollbar among an `Fl_Text_Display`'s children.
const TEXT_DISPLAY_VSCROLLBAR: i32 = 1;

/// Every child of `widget` that is a scrollbar, in child order. Empty when
/// `widget` is not an `Fl_Group` subclass.
fn scrollbar_children<W: WidgetExt>(widget: &W) -> Vec<Scrollbar> {
    let Some(group) = Group::from_dyn_widget_ptr(widget.as_widget_ptr()) else {
        return Vec::new();
    };
    (0..group.children())
        .filter_map(|i| group.child(i))
        .filter_map(|child| Scrollbar::from_dyn_widget(&child))
        .collect()
}

/// The vertical scrollbar of a text display (`TextDisplay`, `TextEditor`, …).
fn text_display_vscrollbar<D: DisplayExt>(display: &D) -> Option<Scrollbar> {
    let group = Group::from_dyn_widget_ptr(display.as_widget_ptr())?;
    let child = group.child(TEXT_DISPLAY_VSCROLLBAR)?;
    Scrollbar::from_dyn_widget(&child)
}

/// Current value of a text display's vertical scrollbar — the first visible
/// line, as FLTK keeps it. `0.0` when the scrollbar cannot be found.
pub fn text_display_vscroll_value<D: DisplayExt>(display: &D) -> f64 {
    text_display_vscrollbar(display).map_or(0.0, |sb| sb.value())
}

/// Give one scrollbar the flat themed look: a `track` background and a
/// `thumb` slider, both without bevels.
pub fn style_scrollbar(scrollbar: &mut Scrollbar, track: Color, thumb: Color) {
    scrollbar.set_frame(FrameType::FlatBox);
    scrollbar.set_color(track);
    scrollbar.set_slider_frame(FrameType::FlatBox);
    scrollbar.set_selection_color(thumb);
}

/// Apply [`style_scrollbar`] to every built-in scrollbar of `widget`
/// (a text display, a tree, or any other `Fl_Group` with scrollbar children).
pub fn style_scrollbars<W: WidgetExt>(widget: &W, track: Color, thumb: Color) {
    for mut scrollbar in scrollbar_children(widget) {
        style_scrollbar(&mut scrollbar, track, thumb);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fltk::{
        text::{TextDisplay, TextEditor},
        tree::Tree,
        valuator::ScrollbarType,
    };

    fn orientations<W: WidgetExt>(widget: &W) -> Vec<ScrollbarType> {
        scrollbar_children(widget)
            .iter()
            .map(|sb| sb.get_type::<ScrollbarType>())
            .collect()
    }

    // All widget construction lives in this one test: FLTK's "current group"
    // is process-global, so building widgets from parallel test threads races.
    #[test]
    fn child_index_invariant_matches_bundled_fltk() {
        let _fltk = crate::ui::fltk_widget_test_lock();
        let display = TextDisplay::default();
        let editor = TextEditor::default();
        let tree = Tree::default();

        // Fl_Text_Display: child 0 = horizontal, child 1 = vertical.
        let text_order = vec![ScrollbarType::Horizontal, ScrollbarType::Vertical];
        assert_eq!(orientations(&display), text_order);
        assert_eq!(orientations(&editor), text_order);
        for vsb in [
            text_display_vscrollbar(&display),
            text_display_vscrollbar(&editor),
        ] {
            let vsb = vsb.expect("text display has a vertical scrollbar");
            assert_eq!(vsb.get_type::<ScrollbarType>(), ScrollbarType::Vertical);
        }
        assert_eq!(text_display_vscroll_value(&display), 0.0);

        // Fl_Tree: the opposite order — child 0 = vertical, 1 = horizontal.
        assert_eq!(
            orientations(&tree),
            vec![ScrollbarType::Vertical, ScrollbarType::Horizontal]
        );

        // A widget that is not an Fl_Group has no scrollbar children.
        let lone = Scrollbar::default();
        assert!(scrollbar_children(&lone).is_empty());
    }
}
