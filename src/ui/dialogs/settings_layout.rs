//! Pure layout decisions for the Settings dialog (T0083): how tall the
//! dialog is and whether its content scrolls, and how wide its columns and
//! label cells are given the *measured* label widths. No FLTK calls here, so
//! the rules are unit-tested; `settings_dialog` measures and applies them.

/// Height of the fixed Save/Cancel row at the bottom (button + margins).
pub const BUTTON_ROW_H: i32 = 50;
/// Margin around the content area.
pub const CONTENT_MARGIN: i32 = 10;
/// Room left for the window decorations above the client area when the
/// dialog is fitted to the work area (the real title bar is not known
/// before the window is shown).
pub const DECORATION_ALLOWANCE: i32 = 40;

/// Vertical layout of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogHeight {
    /// Client height of the dialog window.
    pub dialog_h: i32,
    /// Height of the content viewport (above the button row).
    pub viewport_h: i32,
    /// The content is taller than the viewport: it scrolls.
    pub scrolls: bool,
}

/// Fit the dialog to the work area. `content_h` is the full height the
/// content needs, `work_h` the height of the screen's work area. The button
/// row is always included and never scrolls; when content + buttons do not
/// fit, the content area shrinks to what is left and scrolls.
pub fn dialog_height(content_h: i32, work_h: i32) -> DialogHeight {
    let content_h = content_h.max(0);
    let natural = content_h + 2 * CONTENT_MARGIN + BUTTON_ROW_H;
    let max_h = work_h
        .saturating_sub(DECORATION_ALLOWANCE)
        .max(BUTTON_ROW_H + 2 * CONTENT_MARGIN + 1);
    if natural <= max_h {
        DialogHeight {
            dialog_h: natural,
            viewport_h: content_h,
            scrolls: false,
        }
    } else {
        DialogHeight {
            dialog_h: max_h,
            viewport_h: max_h - 2 * CONTENT_MARGIN - BUTTON_ROW_H,
            scrolls: true,
        }
    }
}

/// The tallest of the columns, i.e. the height the content needs.
pub fn content_height(column_bottoms: &[i32]) -> i32 {
    column_bottoms.iter().copied().max().unwrap_or(0)
}

/// A column is as wide as its widest measured row, but never narrower than
/// the historical `min_w` (so a short-labelled dialog looks as before).
pub fn column_width(min_w: i32, measured_rows: &[i32]) -> i32 {
    measured_rows.iter().copied().fold(min_w, i32::max)
}

/// The dialog is widened by the vertical scrollbar when its content
/// scrolls, so the bar never covers the right column.
pub fn dialog_width(content_w: i32, scrolls: bool, scrollbar_w: i32) -> i32 {
    content_w + if scrolls { scrollbar_w } else { 0 }
}

/// Top of the dialog's client area in a work area starting at `work_y`, tall
/// `work_h`: vertically centred, leaving [`DECORATION_ALLOWANCE`] above for
/// the title bar, and never above the work area.
pub fn dialog_top(work_y: i32, work_h: i32, dialog_h: i32) -> i32 {
    let room = work_h
        .saturating_sub(dialog_h)
        .saturating_add(DECORATION_ALLOWANCE);
    work_y.saturating_add((room / 2).max(DECORATION_ALLOWANCE))
}

/// Gap kept between a focused field and the viewport edge when scrolling it
/// into view.
pub const SCROLL_MARGIN: i32 = 8;

/// The scroll position that brings a focused child (`child_y`, `child_h`, in
/// window coordinates as currently scrolled) into a viewport at `view_y`,
/// `view_h`, given the current scroll position `pos`; `None` when the child
/// is already fully visible.
pub fn scroll_to_show(
    child_y: i32,
    child_h: i32,
    view_y: i32,
    view_h: i32,
    pos: i32,
) -> Option<i32> {
    let (top, bottom) = (child_y, child_y + child_h);
    let (view_top, view_bottom) = (view_y, view_y + view_h);
    if top < view_top {
        Some((pos - (view_top - top) - SCROLL_MARGIN).max(0))
    } else if bottom > view_bottom {
        Some(pos + (bottom - view_bottom) + SCROLL_MARGIN)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_dialog_layout_scroll_follows_focus() {
        // Viewport 10..410, content scrolled by 100.
        assert_eq!(scroll_to_show(200, 22, 10, 400, 100), None, "visible");
        // Below the viewport: scroll down just enough (+ margin).
        assert_eq!(
            scroll_to_show(420, 22, 10, 400, 100),
            Some(100 + 32 + SCROLL_MARGIN)
        );
        // Above it: scroll up, never below 0.
        assert_eq!(
            scroll_to_show(-30, 22, 10, 400, 100),
            Some(100 - 40 - SCROLL_MARGIN)
        );
        assert_eq!(scroll_to_show(0, 22, 10, 400, 5), Some(0));
    }

    #[test]
    fn settings_dialog_layout_width_and_top() {
        assert_eq!(dialog_width(620, false, 16), 620);
        assert_eq!(dialog_width(620, true, 16), 636);
        // Plenty of room: centred below the decoration allowance.
        let top = dialog_top(0, 1050, 600);
        assert!(top >= DECORATION_ALLOWANCE && top + 600 <= 1050, "{top}");
        // Exactly fitting: the dialog ends at the work-area bottom.
        let h = 768 - DECORATION_ALLOWANCE;
        assert_eq!(dialog_top(0, 768, h) + h, 768);
        // A work area with a top panel.
        assert!(dialog_top(27, 741, 600) >= 27 + DECORATION_ALLOWANCE);
    }

    #[test]
    fn settings_dialog_layout_scrolls_when_taller_than_work_area() {
        // Content 900 on a 768 work area (1366×768 laptop).
        let h = dialog_height(900, 768);
        assert!(h.scrolls);
        assert!(h.dialog_h + DECORATION_ALLOWANCE <= 768, "{h:?}");
        // The buttons are inside the dialog, below the viewport.
        assert!(h.viewport_h + 2 * CONTENT_MARGIN + BUTTON_ROW_H <= h.dialog_h);
        assert!(h.viewport_h > 0);
    }

    #[test]
    fn settings_dialog_layout_no_scroll_when_it_fits() {
        let h = dialog_height(590, 1050);
        assert_eq!(
            h,
            DialogHeight {
                dialog_h: 590 + 2 * CONTENT_MARGIN + BUTTON_ROW_H,
                viewport_h: 590,
                scrolls: false,
            }
        );
        // Exactly fitting is not scrolling.
        let exact = 590 + 2 * CONTENT_MARGIN + BUTTON_ROW_H + DECORATION_ALLOWANCE;
        assert!(!dialog_height(590, exact).scrolls);
        assert!(dialog_height(590, exact - 1).scrolls);
    }

    #[test]
    fn settings_dialog_layout_degenerate_work_area() {
        // A nonsense work area never yields a negative or zero viewport.
        for work in [i32::MIN, -5, 0, 10] {
            let h = dialog_height(900, work);
            assert!(h.scrolls && h.viewport_h >= 1, "{work}: {h:?}");
        }
        assert_eq!(dialog_height(-50, 800).viewport_h, 0);
    }

    #[test]
    fn settings_dialog_layout_columns_grow_with_measured_labels() {
        assert_eq!(column_width(280, &[120, 200]), 280, "short labels keep 280");
        assert_eq!(
            column_width(280, &[120, 312]),
            312,
            "a long label widens it"
        );
        assert_eq!(column_width(280, &[]), 280);
        assert_eq!(content_height(&[580, 612]), 612);
        assert_eq!(content_height(&[]), 0);
    }
}
