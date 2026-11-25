//! UI construction tests (T0080): build the new dialogs' real windows in a
//! debug build, so fltk-rs debug assertions in widget setup fail here and
//! not in the user's hands (one aborted "Manage themes…").
//!
//! Building these windows opens the display (`center_screen`). They live in
//! their own test binary so a missing display can only fail this binary,
//! never the library's unit tests. Without a display they are skipped with a
//! note, unless `FERRISPAD_REQUIRE_UI_TESTS=1` (set in CI, which runs the
//! tests under `xvfb-run`): then a missing display is a failure.
//!
//! One test function on purpose: fltk-rs pins its "UI thread" to the first
//! thread that creates a window and asserts it for every later one.

use ferris_pad::app::controllers::theme_manager::ThemeManagerModel;
use ferris_pad::app::domain::settings::SyntaxTheme;
use ferris_pad::app::services::custom_themes::ThemeStore;
use ferris_pad::ui::dialogs::color_picker::build_color_picker;
use ferris_pad::ui::dialogs::theme_manager::build_theme_manager;
use fltk::prelude::*;

/// Whether a display is available; panics when one is required but absent.
fn display_available() -> bool {
    let has_display = ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|var| std::env::var_os(var).is_some_and(|v| !v.is_empty()));
    if !has_display {
        assert!(
            std::env::var("FERRISPAD_REQUIRE_UI_TESTS").as_deref() != Ok("1"),
            "UI tests required but no display (set DISPLAY, e.g. run under xvfb-run)"
        );
        eprintln!("skipping UI construction tests: no DISPLAY or WAYLAND_DISPLAY");
    }
    has_display
}

#[test]
fn new_dialog_windows_build() {
    if !display_available() {
        return;
    }

    // Theme manager, with a custom theme whose name has FLTK format codes.
    let dir = tempfile::tempdir().unwrap();
    let mut store = ThemeStore::empty(dir.path().to_path_buf());
    store
        .clone_from(&SyntaxTheme::Base16OceanDark.into(), "@b weird\tname")
        .unwrap();
    let model = ThemeManagerModel::new(
        store,
        SyntaxTheme::Base16OceanLight.into(),
        SyntaxTheme::Base16OceanDark.into(),
        true,
        SyntaxTheme::Base16OceanDark.into(),
    );
    let (manager, list) = build_theme_manager((43, 48, 59), model);
    assert_eq!(list.size() as usize, SyntaxTheme::all().len() + 1);
    assert_eq!(list.text(list.size()).as_deref(), Some("@.@b weird name"));
    assert!(!manager.shown());

    // Color picker, on a dark and a light dialog background.
    for bg in [(43, 48, 59), (239, 241, 245)] {
        let (picker, result) = build_color_picker(bg, "Selection Color", (0, 120, 215));
        assert!(!picker.shown());
        assert_eq!(*result.borrow(), None, "nothing chosen before OK");
    }
}
