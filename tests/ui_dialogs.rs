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
use ferris_pad::ui::dialogs::session_picker::build_session_picker;
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

    // Sessions picker (T0105): layout, themed inputs, and its button row
    // must not run past the list's right edge (it ended 5 px past it).
    let sessions = vec![
        "default".to_string(),
        "notes".to_string(),
        "work".to_string(),
    ];
    for (name, theme) in [("sessions dark", &dark), ("sessions light", &light)] {
        let ui = build_session_picker(theme, "default", &sessions);
        failures.extend(layout_failures(&ui.dialog_win, name, everything));
        failures.extend(unthemed_inputs(&ui.dialog_win, name, theme));
        let list_right = ui.browser.x() + ui.browser.w();
        for b in [
            &ui.switch_btn,
            &ui.new_win_btn,
            &ui.delete_btn,
            &ui.cancel_btn,
        ] {
            if b.x() + b.w() > list_right {
                failures.push(format!(
                    "{name}: button {:?} ends at {} past the list edge {list_right}",
                    b.label(),
                    b.x() + b.w()
                ));
            }
        }
        if ui.cancel_btn.x() + ui.cancel_btn.w() != list_right {
            failures.push(format!(
                "{name}: button row not aligned to the list's right edge"
            ));
        }
    }
    // Every other dialog, through its build seam (T0106), in dark and light:
    // layout, and every input, list and button built as a themed component.
    for (mode, bg, theme) in [
        ("dark", (43, 48, 59), &dark),
        ("light", (239, 241, 245), &light),
    ] {
        for (name, window) in run_only_dialogs(bg, theme) {
            let name = format!("{name} {mode}");
            failures.extend(layout_failures(&window, &name, everything));
            failures.extend(unthemed_inputs(&window, &name, theme));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The dialogs that build and run in one `show_*` call, built through their
/// `build_*` seams (T0106) on the theme background `bg`.
fn run_only_dialogs(bg: (u8, u8, u8), theme: &DialogTheme) -> Vec<(&'static str, Window)> {
    use ferris_pad::ui::dialogs::{
        about::build_about_dialog,
        build_themed_choice, build_themed_message,
        find::{build_find_dialog, build_replace_dialog},
        font_picker::build_font_picker,
        goto_line::build_goto_line_dialog,
        session_picker::build_new_session_dialog,
    };
    let mut dialogs = vec![
        ("find", build_find_dialog(theme).dialog_win),
        ("replace", build_replace_dialog(theme).dialog_win),
        ("go to line", build_goto_line_dialog(theme).dialog_win),
        (
            "font picker",
            build_font_picker(theme, "Courier", 14).dialog_win,
        ),
        ("new session", build_new_session_dialog(theme).dialog_win),
        ("about", build_about_dialog(theme, bg).dialog),
        (
            "message",
            build_themed_message(bg, "Go To Line", "Please enter a valid line number"),
        ),
        (
            "large-file warning",
            build_themed_choice(
                bg,
                "Large File Warning",
                "\"big.log\" is 120.0 MB which may take a while to load and use \
                 significant memory.\n\nDo you want to continue?",
                "Open",
                "Cancel",
                "",
            ),
        ),
    ];
    dialogs.extend(app_data_dialogs(bg, theme));
    dialogs.extend(plugin_dialogs(theme));
    dialogs
}

/// The run-only dialogs that show app data (shortcuts, an update, a large
/// file), built from small fakes.
fn app_data_dialogs(bg: (u8, u8, u8), theme: &DialogTheme) -> Vec<(&'static str, Window)> {
    use ferris_pad::app::plugins::PluginMenuItem;
    use ferris_pad::app::services::shortcut_registry::ShortcutRegistry;
    use ferris_pad::app::services::updater::ReleaseInfo;
    use ferris_pad::ui::dialogs::{
        large_file::build_file_too_large_dialog,
        readonly_viewer::build_readonly_viewer,
        shortcut_dialog::{PluginShortcuts, build_shortcut_dialog},
        update::build_update_available_dialog,
    };
    let items = vec![PluginMenuItem {
        label: "Lint".into(),
        action: "lint".into(),
        shortcut: Some("Ctrl+Shift+L".into()),
    }];
    let plugins = [PluginShortcuts {
        name: "python-lint",
        menu_items: &items,
    }];
    let registry = ShortcutRegistry::default();
    let release = ReleaseInfo {
        tag_name: "9.9.9".into(),
        name: "9.9.9".into(),
        body: "- Fix A\n- Add B".into(),
        html_url: "https://example.invalid".into(),
        published_at: "2026-01-01T00:00:00Z".into(),
        prerelease: false,
        assets: vec![],
    };
    vec![
        (
            "shortcuts",
            build_shortcut_dialog(theme, &registry, &plugins, true).dialog,
        ),
        (
            "shortcuts, no plugins",
            build_shortcut_dialog(theme, &registry, &[], false).dialog,
        ),
        (
            "update",
            build_update_available_dialog(theme, bg, &release, true).dialog,
        ),
        (
            "update, manual only",
            build_update_available_dialog(theme, bg, &release, false).dialog,
        ),
        (
            "file too large",
            build_file_too_large_dialog(theme, "huge.log", 200 * 1024 * 1024, 150).dialog,
        ),
        (
            "read-only viewer",
            build_readonly_viewer(
                theme,
                "huge.log",
                3 * 1024 * 1024,
                fltk::enums::Font::Courier,
                14,
            )
            .window,
        ),
    ]
}

/// The plugin dialogs, built from small fakes of plugin metadata.
fn plugin_dialogs(theme: &DialogTheme) -> Vec<(&'static str, Window)> {
    use ferris_pad::app::domain::settings::PluginConfig;
    use ferris_pad::app::plugins::ConfigParamDef;
    use ferris_pad::app::services::plugin_registry::{AvailablePluginInfo, PluginTier};
    use ferris_pad::ui::dialogs::{
        community_install::{CommunityInstallReview, build_community_install_dialog},
        plugin_config::build_plugin_config_dialog,
        plugin_manager::{InstalledPluginEntry, OfficialPluginEntry, build_plugin_manager_dialog},
        plugin_permissions::{PermissionRequest, build_permission_dialog},
        plugin_settings::build_plugin_settings_dialog,
    };
    let defs = vec![
        ConfigParamDef {
            key: "len".into(),
            label: "Max length".into(),
            param_type: "number".into(),
            default: "80".into(),
            ..Default::default()
        },
        ConfigParamDef {
            key: "on".into(),
            label: "Strict".into(),
            param_type: "boolean".into(),
            ..Default::default()
        },
        ConfigParamDef {
            key: "mode".into(),
            label: "Mode".into(),
            param_type: "choice".into(),
            options: vec!["a|Alpha".into(), "b".into()],
            ..Default::default()
        },
        ConfigParamDef {
            key: "args".into(),
            label: "Args".into(),
            param_type: "string".into(),
            placeholder: Some("--flag".into()),
            ..Default::default()
        },
    ];
    let request = PermissionRequest {
        plugin_name: "Python Lint".into(),
        description: "Lints Python".into(),
        commands: vec!["ruff".into(), "mypy".into()],
    };
    // A plugin's description is free text of any length (T0110, found by
    // ferrispad-3b with dialogs-permission.sh).
    let long_request = PermissionRequest {
        plugin_name: "Python Lint".into(),
        description:
            "Screenshot fixture: a plugin with a menu item, settings and a command permission"
                .into(),
        commands: vec!["ruff".into()],
    };
    let review = |manual: bool| CommunityInstallReview {
        plugin_name: "demo".into(),
        version: "1.0.0".into(),
        author: if manual { String::new() } else { "ann".into() },
        source_url: "https://github.com/u/demo".into(),
        permissions: if manual { vec!["git".into()] } else { vec![] },
        is_manual: manual,
        has_text_change_hook: manual,
        scan_warnings: if manual {
            vec!["uses os.execute".into()]
        } else {
            vec![]
        },
    };
    let installed = vec![InstalledPluginEntry {
        name: "Python Lint".into(),
        version: "1.2.0".into(),
        description: "Lints Python".into(),
        enabled: true,
        tier: PluginTier::Official,
    }];
    let official = vec![OfficialPluginEntry {
        info: AvailablePluginInfo {
            name: "python-lint".into(),
            version: "1.3.0".into(),
            description: "Lints".into(),
            path: "python-lint/".into(),
            author: "ann".into(),
            _license: "MIT".into(),
            tags: vec!["lint".into()],
            _min_ferrispad_version: "0.9.0".into(),
            checksums: None,
            signature: None,
            readme_url: Some("https://example.invalid".into()),
        },
        already_installed: true,
        update_available: true,
    }];
    vec![
        (
            "plugin config",
            build_plugin_config_dialog(theme, "Demo", &defs, &PluginConfig::default()).dialog,
        ),
        (
            "plugin settings",
            build_plugin_settings_dialog(
                theme,
                &[],
                "Ctrl+Shift+L",
                &[("Python Lint".into(), true), ("Off".into(), false)],
            )
            .dialog,
        ),
        (
            "permissions",
            build_permission_dialog(theme, &request).dialog,
        ),
        (
            "permissions, long description",
            build_permission_dialog(theme, &long_request).dialog,
        ),
        (
            "community install, manual",
            build_community_install_dialog(theme, &review(true)).dialog,
        ),
        (
            "community install, indexed",
            build_community_install_dialog(theme, &review(false)).dialog,
        ),
        (
            "plugin manager",
            build_plugin_manager_dialog(theme, &installed, Ok(&official)).dialog,
        ),
        (
            "plugin manager, offline",
            build_plugin_manager_dialog(theme, &installed, Err("Network error: offline")).dialog,
        ),
    ]
}
