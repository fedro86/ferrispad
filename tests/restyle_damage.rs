//! T0095: a re-highlight repaints only what it restyled. A keystroke's
//! re-highlight step and every chunk of the chunked highlight must leave
//! range damage on the editor, never `FL_DAMAGE_ALL`: FLTK answers that with
//! two `recalc_display()` calls, each re-measuring the whole buffer.
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
use fltk::enums::Damage;
use fltk::prelude::*;
use support::ui_layout::display_available;

/// `lines` lines of Rust, each with a keyword, a number and a comment.
fn rust_lines(lines: usize) -> String {
    (0..lines)
        .map(|i| format!("fn f{i}() {{ let x = {i}; }} // note {i}\n"))
        .collect()
}

/// Replace the active document's text and highlight it from the top as a
/// Rust file.
fn load_rust(state: &mut AppState, text: &str) {
    let id = state.tab_manager.active_id().unwrap();
    state
        .tab_manager
        .active_doc_mut()
        .unwrap()
        .buffer
        .set_text(text);
    state
        .highlight
        .detect_and_highlight(id, "t0095.rs", &mut state.tab_manager, &state.sender);
    state.bind_active_buffer();
}

#[test]
fn restyle_repaints_only_its_range() {
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

    let settings = AppSettings {
        tabs_enabled: false,
        highlighting_enabled: true,
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

    // Keystroke: a typed `"` opens a string, so the rest of the line (and
    // on) is restyled by the re-highlight step.
    load_rust(&mut state, &rust_lines(300));
    let id = state.tab_manager.active_id().unwrap();
    let pos = rust_lines(10).len() as i32 + 3;
    state
        .tab_manager
        .active_doc_mut()
        .unwrap()
        .buffer
        .insert(pos, "\"");
    state.schedule_rehighlight(id, pos);
    state.editor.clear_damage();
    state.do_pending_rehighlight();
    let mut failures = Vec::new();
    let damage = state.editor.damage_type();
    if damage.contains(Damage::All) {
        failures.push(format!(
            "a keystroke's re-highlight repainted the whole editor: {damage:?}"
        ));
    }
    if !damage.contains(Damage::Scroll) {
        failures.push(format!(
            "a keystroke's re-highlight repainted no range: {damage:?}"
        ));
    }

    // Chunks: more than the synchronous limit (1 MiB) is highlighted in
    // chunks; the first step starts the pass, the others each style one
    // chunk.
    load_rust(&mut state, &rust_lines(40_000));
    let mut full = Vec::new();
    let mut steps = 0;
    state.continue_chunked_highlight();
    assert!(
        state.highlight.highlighter().chunked_doc_id().is_some(),
        "the document is not highlighted in chunks"
    );
    while state.highlight.highlighter().chunked_doc_id().is_some() {
        state.editor.clear_damage();
        state.continue_chunked_highlight();
        steps += 1;
        let last = state.highlight.highlighter().chunked_doc_id().is_none();
        if !last && state.editor.damage_type().contains(Damage::All) {
            full.push(steps);
        }
        assert!(steps < 10_000, "the chunked highlight does not finish");
    }
    assert!(steps > 2, "only {steps} chunk steps");
    if !full.is_empty() {
        failures.push(format!(
            "{} of {steps} chunk steps repainted the whole editor",
            full.len()
        ));
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
