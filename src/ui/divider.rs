//! Draggable panel dividers (split view, tree panel, terminal panel).
//!
//! Every divider is a hidden flat `Frame` that shows a resize cursor while the
//! pointer is over it and, during a drag, reports the pointer position as a
//! `Message`. [`new_divider`] is the single implementation of that handler;
//! [`dragged_size`] is the matching pure pointer → panel-size math the
//! dispatch handlers apply.

use std::cell::Cell;
use std::rc::Rc;

use fltk::{
    app::Sender,
    enums::{Cursor, Event, FrameType},
    frame::Frame,
    prelude::*,
};

use crate::app::Message;

/// The pointer axis a divider is dragged along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragAxis {
    /// Left/right drag of a vertical divider (`Cursor::WE`, `event_x`).
    X,
    /// Up/down drag of a horizontal divider (`Cursor::NS`, `event_y`).
    Y,
}

impl DragAxis {
    fn cursor(self) -> Cursor {
        match self {
            DragAxis::X => Cursor::WE,
            DragAxis::Y => Cursor::NS,
        }
    }

    fn pointer(self) -> i32 {
        match self {
            DragAxis::X => fltk::app::event_x(),
            DragAxis::Y => fltk::app::event_y(),
        }
    }
}

/// Create a hidden divider `Frame` in the current group.
///
/// While the pointer is over it the window shows the `axis` resize cursor.
/// Between a `Push` and the next `Released`, each `Drag` sends
/// `on_drag(pointer)`, where `pointer` is the event coordinate along `axis`.
/// The `dragging` guard keeps the cursor from resetting when the pointer leaves
/// the thin divider mid-drag, and drops a `Drag` without a prior `Push`.
/// `window()` is `None` during teardown or before `show()`, so it is never
/// unwrapped (T0018).
pub fn new_divider(sender: Sender<Message>, axis: DragAxis, on_drag: fn(i32) -> Message) -> Frame {
    let mut divider = Frame::default();
    divider.set_frame(FrameType::FlatBox);
    divider.hide();

    let dragging = Rc::new(Cell::new(false));
    divider.handle(move |div, ev| match ev {
        Event::Enter => {
            if let Some(mut win) = div.window() {
                win.set_cursor(axis.cursor());
            }
            true
        }
        Event::Leave => {
            if !dragging.get()
                && let Some(mut win) = div.window()
            {
                win.set_cursor(Cursor::Default);
            }
            true
        }
        Event::Push => {
            dragging.set(true);
            true
        }
        Event::Drag => {
            if dragging.get() {
                sender.send(on_drag(axis.pointer()));
            }
            true
        }
        Event::Released => {
            dragging.set(false);
            if let Some(mut win) = div.window() {
                win.set_cursor(Cursor::Default);
            }
            true
        }
        _ => false,
    });

    divider
}

/// The end of its parent a dragged panel is docked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockEdge {
    /// Left / top: the panel spans from the parent's start to the pointer.
    Start,
    /// Right / bottom: the panel spans from the pointer to the parent's end.
    End,
}

/// New panel size for a divider dragged to `pointer` inside a parent spanning
/// `start..start + len` along the drag axis, with the panel docked at `edge`.
///
/// The size is clamped to `min..=max`. When the parent is so small that
/// `max < min`, the minimum wins instead of panicking (`i32::clamp` asserts
/// `min <= max`).
pub fn dragged_size(pointer: i32, start: i32, len: i32, edge: DockEdge, min: i32, max: i32) -> i32 {
    let size = match edge {
        DockEdge::Start => pointer.saturating_sub(start),
        DockEdge::End => start.saturating_add(len).saturating_sub(pointer),
    };
    size.clamp(min, max.max(min))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_docked_size_is_pointer_minus_parent_start() {
        // Tree on the left of a row spanning x = 50..1050, dragged to x = 350.
        assert_eq!(dragged_size(350, 50, 1000, DockEdge::Start, 100, 500), 300);
    }

    #[test]
    fn end_docked_size_is_parent_end_minus_pointer() {
        // Panel at the bottom of a column spanning y = 0..1000, dragged to 600.
        assert_eq!(dragged_size(600, 0, 1000, DockEdge::End, 100, 500), 400);
    }

    #[test]
    fn size_is_clamped_to_min_and_max() {
        assert_eq!(dragged_size(990, 0, 1000, DockEdge::End, 100, 500), 100);
        assert_eq!(dragged_size(10, 0, 1000, DockEdge::End, 100, 500), 500);
        assert_eq!(dragged_size(-40, 0, 1000, DockEdge::Start, 100, 500), 100);
        assert_eq!(dragged_size(2000, 0, 1000, DockEdge::Start, 100, 500), 500);
    }

    #[test]
    fn tiny_parent_where_max_is_below_min_returns_min_without_panicking() {
        // Terminal: 4/5 of a 200px row is 160 < its 200px minimum.
        let w = 200;
        assert_eq!(dragged_size(100, 0, w, DockEdge::End, 200, w * 4 / 5), 200);
        // Tree: half of a 150px row is 75 < its 100px minimum, on both edges.
        let w = 150;
        assert_eq!(dragged_size(60, 0, w, DockEdge::Start, 100, w / 2), 100);
        assert_eq!(dragged_size(60, 0, w, DockEdge::End, 100, w / 2), 100);
        // Fully collapsed parent.
        assert_eq!(dragged_size(0, 0, 0, DockEdge::End, 100, 0), 100);
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        assert_eq!(
            dragged_size(i32::MIN, i32::MAX, i32::MAX, DockEdge::End, 100, 500),
            500
        );
        assert_eq!(
            dragged_size(i32::MAX, i32::MIN, 10, DockEdge::Start, 100, 500),
            500
        );
    }
}
