//! T0103: the file-type menus follow the active document whenever it is
//! rebound, not only on a tab switch. Builds the real main window and
//! `AppState` (as `main.rs` does), so it needs a display: like
//! `tests/ui_dialogs.rs` it is skipped without one unless
//! `FERRISPAD_REQUIRE_UI_TESTS=1` (CI runs it under `xvfb-run`).
//!
//! One test function on purpose: fltk-rs pins its "UI thread" to the first
//! thread that creates a window.

mod support;

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use ferris_pad::app::domain::messages::Message;
use ferris_pad::app::domain::settings::{AppSettings, TreePanelPosition};
use ferris_pad::app::services::session::{
    DocumentSession, SessionData, SessionRestore, session_dir,
};
use ferris_pad::app::services::shortcut_registry::ShortcutRegistry;
use ferris_pad::app::services::window_geometry::ScreenRect;
use ferris_pad::app::state::AppState;
use ferris_pad::ui::main_window::build_main_window;
use ferris_pad::ui::menu::build_menu;
use fltk::prelude::*;
use support::ui_layout::display_available;

const PREVIEW: &str = "View/Preview in Browser";

fn preview_active(state: &AppState) -> bool {
    state
        .menu
        .find_item(PREVIEW)
        .expect("the Preview menu item exists")
        .active()
}

#[test]
fn file_type_menus_follow_the_active_document() {
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

    // A session whose active document is a Markdown file.
    let files = root.path().join("files");
    fs::create_dir_all(&files).unwrap();
    let md = files.join("notes.md");
    let txt = files.join("other.txt");
    fs::write(&md, "# Notes\n").unwrap();
    fs::write(&txt, "plain\n").unwrap();
    let doc = |path: &std::path::Path| DocumentSession {
        file_path: Some(path.to_string_lossy().into_owned()),
        display_name: path.file_name().unwrap().to_string_lossy().into_owned(),
        cursor_position: 0,
        temp_file: None,
        was_dirty: false,
        group_index: None,
    };
    let session = SessionData {
        version: 1,
        active_index: 1,
        documents: vec![doc(&txt), doc(&md)],
        last_open_directory: None,
        groups: vec![],
        instance_id: Some("1".to_string()),
    };
    let dir = session_dir("default");
    // Before touching anything: on a platform whose dirs ignore XDG_* this
    // would be the user's real session dir.
    assert!(dir.starts_with(root.path()), "session dir is the temp one");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("session.json"),
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .unwrap();

    // Build the window and the state as main.rs does.
    let settings = AppSettings {
        tabs_enabled: true,
        session_restore: SessionRestore::SavedFiles,
        ..AppSettings::default()
    };
    let (sender, _receiver) = fltk::app::channel::<Message>();
    let mut w = build_main_window(
        true,
        &sender,
        TreePanelPosition::default(),
        ScreenRect::new(0, 0, 800, 600),
    );
    let registry = ShortcutRegistry::from_settings(&settings.shortcut_overrides);
    build_menu(&mut w.menu, &sender, &settings, false, true, &registry);
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
        true,
        w.tab_bar,
        "default".to_string(),
    );
    // Startup: menus computed for the default untitled document.
    state.update_menus_for_file_type();
    assert!(!preview_active(&state), "no Markdown document yet");

    // The deferred session restore makes notes.md the active document.
    state.restore_session();
    let active = state.tab_manager.active_doc().unwrap();
    assert_eq!(active.display_name, "notes.md");
    assert!(
        preview_active(&state),
        "Preview in Browser must be enabled for the restored active .md"
    );

    // Closing every tab: no document, no Preview.
    while let Some(id) = state.tab_manager.active_id() {
        if state.close_tab(id) {
            break;
        }
    }
    assert_eq!(state.tab_manager.count(), 0);
    assert!(
        !preview_active(&state),
        "no document open: Preview disabled"
    );
}
