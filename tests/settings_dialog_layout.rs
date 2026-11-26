//! T0083: the whole Settings dialog must lay out with the fonts actually
//! installed. Every label drawn inside its widget fits (also at +2 pt), and
//! no sibling widgets overlap. Run under Xvfb in CI; locally run it with a
//! wide desktop font too, e.g. DejaVu Sans via FONTCONFIG_FILE.
//!
//! One test function on purpose: fltk-rs pins its UI thread to the first
//! thread that creates a window.

mod support;

use ferris_pad::app::AppSettings;
use ferris_pad::ui::dialogs::settings_dialog::build_settings_dialog;
use fltk::prelude::*;
use support::ui_layout::*;

/// Save/Cancel must be fully inside the dialog (below the scrolling content)
/// and the dialog no taller than its screen's work area minus room for the
/// title bar. xvfb-run's default 640×480 screen forces the scrolling layout.
fn buttons_reachable(dialog: &fltk::window::Window, title: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let screen = fltk::app::screen_num(dialog.x() + dialog.w() / 2, dialog.y() + dialog.h() / 2);
    let (_, _, _, work_h) = fltk::app::screen_work_area(screen);
    if dialog.h() > work_h {
        failures.push(format!(
            "{title}: dialog {}px tall, work area {work_h}px",
            dialog.h()
        ));
    }
    for i in 0..dialog.children() {
        let Some(w) = dialog.child(i) else { continue };
        if ["Save", "Cancel"].contains(&w.label().as_str())
            && (w.y() < 0 || w.y() + w.h() > dialog.h() || w.x() + w.w() > dialog.w())
        {
            failures.push(format!(
                "{title}: {:?} at {:?} is outside the {}x{} dialog",
                w.label(),
                (w.x(), w.y(), w.w(), w.h()),
                dialog.w(),
                dialog.h()
            ));
        }
    }
    failures
}

#[test]
fn settings_dialog_labels_fit() {
    if !display_available() {
        return;
    }
    let mut failures = Vec::new();
    // Light and dark dialog backgrounds (Ocean Light / Ocean Dark).
    for bg in [(239, 241, 245), (43, 48, 59)] {
        let ui = build_settings_dialog(&AppSettings::default(), bg, &[]);
        let title = format!("settings{bg:?}");
        failures.extend(layout_failures(&ui.dialog, &title, everything));
        failures.extend(buttons_reachable(&ui.dialog, &title));
    }
    assert!(
        failures.is_empty(),
        "Settings dialog layout failures:\n{}",
        failures.join("\n")
    );
}
