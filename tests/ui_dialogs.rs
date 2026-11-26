//! UI construction tests (T0080): build the new dialogs' real windows in a
//! debug build, so fltk-rs debug assertions in widget setup fail here and
//! not in the user's hands (one aborted "Manage themes…"), and check their
//! layout against the fonts actually installed: every label drawn inside a
//! widget fits it, also with the label font 2 pt larger (a wider desktop
//! font), and no two sibling widgets overlap.
//!
//! Building these windows opens the display. They live in their own test
//! binary so a missing display can only fail this binary, never the
//! library's unit tests. Without a display they are skipped with a note,
//! unless `FERRISPAD_REQUIRE_UI_TESTS=1` (set in CI, which runs the tests
//! under `xvfb-run`): then a missing display is a failure.
//!
//! One test function on purpose: fltk-rs pins its "UI thread" to the first
//! thread that creates a window and asserts it for every later one.

mod support;

use ferris_pad::app::controllers::theme_manager::ThemeManagerModel;
use ferris_pad::app::domain::settings::{AppSettings, SyntaxTheme, ThemeRef};
use ferris_pad::app::services::custom_themes::ThemeStore;
use ferris_pad::ui::dialogs::DialogTheme;
use ferris_pad::ui::dialogs::build_themed_input;
use ferris_pad::ui::dialogs::color_picker::build_color_picker;
use ferris_pad::ui::dialogs::settings_dialog::build_settings_dialog;
use ferris_pad::ui::dialogs::theme_manager::{SWATCH_TOOLTIP, build_theme_manager};
use fltk::prelude::*;
use fltk::widget::Widget;
use fltk::window::Window;
use support::ui_layout::{
    active_buttons_looking_disabled, count_matching, display_available, everything,
    flat_widgets_with_tooltip, layout_failures, unthemed_inputs,
};

/// The Settings rows T0079/T0080 added: the syntax-theme choices (and their
/// labels) and "Manage themes…". The rest of the dialog is T0083's.
fn settings_theme_rows(w: &Widget) -> bool {
    let label = w.label();
    label.starts_with("Syntax Theme") || label == "Manage themes…" || {
        // The two theme Choice widgets carry no label: find them by the
        // entries they list.
        fltk::menu::Choice::from_dyn_widget(w).is_some_and(|c| {
            c.text(0)
                .is_some_and(|t| t.starts_with("Base16 Ocean Dark"))
        })
    }
}

#[test]
fn new_dialog_windows_build() {
    if !display_available() {
        return;
    }
    let mut windows: Vec<(&str, Window)> = Vec::new();

    // Theme manager, with a custom theme whose name has FLTK format codes.
    let dir = tempfile::tempdir().unwrap();
    let mut store = ThemeStore::empty(dir.path().to_path_buf());
    let custom = store
        .clone_from(&SyntaxTheme::Base16OceanDark.into(), "@b weird\tname")
        .unwrap();
    let model = ThemeManagerModel::new(
        store,
        SyntaxTheme::Base16OceanLight.into(),
        SyntaxTheme::Base16OceanDark.into(),
        true,
        ThemeRef::Custom(custom),
    );
    let (manager, list) = build_theme_manager((43, 48, 59), model);
    assert_eq!(list.size() as usize, SyntaxTheme::all().len() + 1);
    assert_eq!(list.text(list.size()).as_deref(), Some("@.@b weird name"));
    // Opens on the theme the editor shows: the custom one, the last row.
    assert_eq!(list.value(), list.size());
    assert!(!manager.shown());
    // Every color swatch keeps a border, so a color close to the dialog
    // background stays visible (7 swatches).
    assert_eq!(
        count_matching(&manager, |w| w.tooltip().as_deref() == Some(SWATCH_TOOLTIP)),
        7
    );
    let flat_swatches = flat_widgets_with_tooltip(&manager, "theme manager", SWATCH_TOOLTIP);
    assert!(flat_swatches.is_empty(), "{flat_swatches:?}");
    windows.push(("theme manager", manager));

    // Color picker, on a dark and a light dialog background.
    for (name, bg) in [
        ("picker dark", (43, 48, 59)),
        ("picker light", (239, 241, 245)),
    ] {
        let (picker, result) = build_color_picker(bg, "Selection Color", (0, 120, 215));
        assert!(!picker.shown());
        assert_eq!(*result.borrow(), None, "nothing chosen before OK");
        windows.push((name, picker));
    }

    // The Duplicate/Rename name prompt.
    windows.push((
        "name prompt",
        build_themed_input(
            (43, 48, 59),
            "Duplicate Theme",
            "Name of the new theme:",
            "x",
        ),
    ));

    // Settings, with a custom theme in the lists.
    let settings = build_settings_dialog(
        &AppSettings::default(),
        (43, 48, 59),
        &[(ThemeRef::Custom("t-a".into()), "A custom theme".into())],
    );

    let mut failures = Vec::new();
    for (name, window) in &windows {
        failures.extend(layout_failures(window, name, everything));
    }
    // Enabled buttons must not look disabled (dimmed label text).
    let dark_dim = DialogTheme::from_theme_bg((43, 48, 59)).text_dim;
    let light_dim = DialogTheme::from_theme_bg((239, 241, 245)).text_dim;
    for (name, window) in &windows {
        let dim = if name.contains("light") {
            light_dim
        } else {
            dark_dim
        };
        failures.extend(active_buttons_looking_disabled(window, name, dim));
    }
    // Two labels, two choices and the button.
    assert_eq!(count_matching(&settings.dialog, settings_theme_rows), 5);
    failures.extend(layout_failures(
        &settings.dialog,
        "settings",
        settings_theme_rows,
    ));
    // Every text input uses the theme's input colors, incl. the shared
    // selection color (T0105), in the dark and the light dialogs.
    let dark = DialogTheme::from_theme_bg((43, 48, 59));
    let light = DialogTheme::from_theme_bg((239, 241, 245));
    let settings_light = build_settings_dialog(
        &AppSettings::default(),
        (239, 241, 245),
        &[(ThemeRef::Custom("t-a".into()), "A custom theme".into())],
    );
    for (name, window) in &windows {
        let theme = if name.contains("light") {
            &light
        } else {
            &dark
        };
        failures.extend(unthemed_inputs(window, name, theme));
    }
    failures.extend(unthemed_inputs(&settings.dialog, "settings dark", &dark));
    failures.extend(unthemed_inputs(
        &settings_light.dialog,
        "settings light",
        &light,
    ));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
