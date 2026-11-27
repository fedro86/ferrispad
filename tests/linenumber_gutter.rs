//! T0107: the line-number gutter fits the widest line number of the active
//! document, follows its line count, and fits at every line-number size.
//! Builds the real main window and `AppState` (as `main.rs` does), so it
//! needs a display: like `tests/ui_dialogs.rs` it is skipped without one
//! unless `FERRISPAD_REQUIRE_UI_TESTS=1` (CI runs it under `xvfb-run`).
//!
//! One test function on purpose: fltk-rs pins its "UI thread" to the first
//! thread that creates a window.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use ferris_pad::app::domain::messages::Message;
use ferris_pad::app::domain::settings::{AppSettings, TreePanelPosition};
use ferris_pad::app::services::shortcut_registry::ShortcutRegistry;
use ferris_pad::app::services::window_geometry::ScreenRect;
use ferris_pad::app::state::AppState;
use ferris_pad::ui::main_window::build_main_window;
use ferris_pad::ui::menu::build_menu;
use fltk::prelude::*;
use support::ui_layout::display_available;

/// FLTK draws a line number inside the gutter minus 3 px on each side
/// (`Fl_Text_Display::draw_line_numbers`).
const FLTK_INSET: i32 = 3;

/// The width a gutter needs for line numbers up to `largest`, measured here
/// independently of the code under test: the widest `digits`-long run of
/// any one digit, in the editor's line-number font and size, plus the insets.
fn needed_width(state: &AppState, largest: usize) -> i32 {
    let digits = largest.to_string().len();
    fltk::draw::set_font(
        state.editor.linenumber_font(),
        state.editor.linenumber_size(),
    );
    let text_w = (0..10)
        .map(|d| fltk::draw::width(&d.to_string().repeat(digits)).ceil() as i32)
        .max()
        .unwrap();
    text_w + 2 * FLTK_INSET
}

/// Load `lines` lines into the active document the way the tabs-off open
/// does (`set_text` on the existing document), then run the
/// `BufferModified` bookkeeping.
fn load_lines(state: &mut AppState, lines: usize) {
    let text: String = (1..=lines).map(|i| format!("line {i}\n")).collect();
    let doc = state.tab_manager.active_doc_mut().unwrap();
    doc.buffer.set_text(&text);
    let id = doc.id;
    state.on_buffer_modified(id);
}

#[test]
fn line_number_gutter_fits_the_widest_number() {
    let root = tempfile::tempdir().unwrap();
    // SAFETY: set before this test creates any thread or reads the
    // environment elsewhere; this binary has a single test function. Keeps
    // AppState (themes, plugins, session) away from the user's real dirs.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
        std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    }
    if !display_available() {
        return;
    }

    // Tabs off: opening a file reuses the active document.
    let settings = AppSettings {
        tabs_enabled: false,
        line_numbers_enabled: true,
        ..AppSettings::default()
    };
    let (sender, _receiver) = fltk::app::channel::<Message>();
    let mut w = build_main_window(
        false,
        &sender,
        TreePanelPosition::default(),
        ScreenRect::new(0, 0, 800, 600),
    );
    let registry = ShortcutRegistry::from_settings(&settings.shortcut_overrides);
    build_menu(&mut w.menu, &sender, &settings, false, false, &registry);
    let mut state = AppState::new(
        w.editor_container,
        w.wind.clone(),
        w.menu.clone(),
        w.flex.clone(),
        w.update_banner_frame.clone(),
        sender,
        Rc::new(RefCell::new(settings.clone())),
        false,
        settings.line_numbers_enabled,
        settings.word_wrap_enabled,
        false,
        w.tab_bar,
        "default".to_string(),
    );
    state.update_linenumber_width();

    // FLTK shows one more line than there are newlines (the empty last line).
    for lines in [9_999, 100_000, 1_000_000] {
        load_lines(&mut state, lines);
        let (have, need) = (
            state.editor.linenumber_width(),
            needed_width(&state, lines + 1),
        );
        assert!(
            have >= need,
            "{lines} lines: gutter {have} px, the number {} needs {need} px",
            lines + 1
        );
    }

    // Every line-number size: the gutter is recomputed and still fits.
    for size in 8..=36 {
        state.editor.set_linenumber_size(size);
        state.update_linenumber_width();
        let (have, need) = (
            state.editor.linenumber_width(),
            needed_width(&state, 1_000_001),
        );
        assert!(
            have >= need,
            "size {size}: gutter {have} px, needs {need} px"
        );
    }
}
