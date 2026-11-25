use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

use ferris_pad::app::domain::settings::{
    AppSettings, PluginApprovals, ShortcutOverride, SyntaxTheme, ThemeMode, ThemeRef,
    WindowGeometry,
};
use ferris_pad::app::services::session::SessionRestore;
use ferris_pad::app::services::updater::UpdateChannel;
use ferris_pad::app::services::window_geometry::{ScreenRect, restore_geometry};

#[test]
fn test_settings_roundtrip_all_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");

    let settings = AppSettings {
        line_numbers_enabled: false,
        word_wrap_enabled: false,
        highlighting_enabled: false,
        theme_mode: ThemeMode::Dark,
        font: "HelveticaMono".to_string(),
        font_size: 20,
        auto_check_updates: false,
        update_channel: UpdateChannel::Beta,
        last_update_check: 1700000000,
        skipped_versions: vec!["0.8.0".to_string(), "0.8.1".to_string()],
        tabs_enabled: true,
        session_restore: SessionRestore::Full,
        preview_enabled: true,
        syntax_theme_light: SyntaxTheme::InspiredGitHub.into(),
        syntax_theme_dark: SyntaxTheme::SolarizedDark.into(),
        tab_size: 2,
        use_spaces: true,
        plugins_enabled: false,
        disabled_plugins: vec!["python-lint".to_string()],
        plugin_approvals: HashMap::new(),
        auto_check_plugin_updates: false,
        last_plugin_update_check: 1700000001,
        run_all_checks_plugins: vec!["python-lint".to_string()],
        run_all_checks_shortcut: "Ctrl+Shift+R".to_string(),
        plugin_configs: HashMap::new(),
        shortcut_overrides: HashMap::new(),
        large_file_warning_mb: 100,
        max_editable_size_mb: 200,
        window: Some(WindowGeometry {
            x: -1200,
            y: 40,
            width: 1100,
            height: 750,
            maximized: true,
        }),
        highlight_current_line: false,
    };

    let json = serde_json::to_string_pretty(&settings).unwrap();
    fs::write(&path, &json).unwrap();

    let loaded_json = fs::read_to_string(&path).unwrap();
    let loaded: AppSettings = serde_json::from_str(&loaded_json).unwrap();

    assert_eq!(settings, loaded);
}

#[test]
fn test_settings_with_plugin_approvals() {
    let mut approvals = HashMap::new();
    approvals.insert(
        "python-lint".to_string(),
        PluginApprovals {
            approved_commands: vec!["ruff".to_string(), "mypy".to_string()],
            denied_commands: vec!["rm".to_string()],
        },
    );

    let settings = AppSettings {
        plugin_approvals: approvals,
        ..Default::default()
    };

    let json = serde_json::to_string(&settings).unwrap();
    let loaded: AppSettings = serde_json::from_str(&json).unwrap();

    let pl = &loaded.plugin_approvals["python-lint"];
    assert_eq!(pl.approved_commands, vec!["ruff", "mypy"]);
    assert_eq!(pl.denied_commands, vec!["rm"]);
}

#[test]
fn test_settings_with_shortcut_overrides() {
    let mut overrides = HashMap::new();
    overrides.insert(
        "File/Save".to_string(),
        ShortcutOverride {
            shortcut: "Ctrl+Shift+S".to_string(),
            enabled: true,
        },
    );
    overrides.insert(
        "Edit/Undo".to_string(),
        ShortcutOverride {
            shortcut: String::new(),
            enabled: true,
        },
    );
    overrides.insert(
        "Edit/Redo".to_string(),
        ShortcutOverride {
            shortcut: "Ctrl+Y".to_string(),
            enabled: false,
        },
    );

    let settings = AppSettings {
        shortcut_overrides: overrides,
        ..Default::default()
    };

    let json = serde_json::to_string(&settings).unwrap();
    let loaded: AppSettings = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.shortcut_overrides.len(), 3);
    assert_eq!(
        loaded.shortcut_overrides["File/Save"].shortcut,
        "Ctrl+Shift+S"
    );
    assert!(loaded.shortcut_overrides["File/Save"].enabled);
    assert!(loaded.shortcut_overrides["Edit/Undo"].shortcut.is_empty());
    assert!(!loaded.shortcut_overrides["Edit/Redo"].enabled);
}

#[test]
fn test_settings_backward_compat() {
    // Minimal old-format JSON
    let json = r#"{
        "line_numbers_enabled": false,
        "font_size": 14
    }"#;

    let loaded: AppSettings = serde_json::from_str(json).unwrap();

    assert!(!loaded.line_numbers_enabled);
    assert_eq!(loaded.font_size, 14);
    // All other fields should get defaults
    assert!(loaded.word_wrap_enabled);
    assert!(loaded.highlighting_enabled);
    assert_eq!(loaded.theme_mode, ThemeMode::SystemDefault);
    assert_eq!(loaded.font, "Courier");
    assert!(loaded.auto_check_updates);
    assert_eq!(loaded.update_channel, UpdateChannel::Stable);
    assert!(loaded.plugins_enabled);
    assert!(loaded.shortcut_overrides.is_empty());
    assert!(loaded.plugin_approvals.is_empty());
    assert_eq!(loaded.tab_size, 4);
    assert_eq!(loaded.large_file_warning_mb, 50);
    assert_eq!(loaded.max_editable_size_mb, 150);
}

// --- T0079: syntax-theme slots reference a built-in or a custom theme ---

#[test]
fn test_theme_ref_backward_compat() {
    let json =
        r#"{ "syntax_theme_dark": "SolarizedDark", "syntax_theme_light": "InspiredGitHub" }"#;
    let loaded = AppSettings::from_json_or_default(json);
    assert_eq!(
        loaded.syntax_theme_dark,
        ThemeRef::Builtin(SyntaxTheme::SolarizedDark)
    );
    assert_eq!(
        loaded.syntax_theme_light,
        ThemeRef::Builtin(SyntaxTheme::InspiredGitHub)
    );
    // Built-ins are still written in the pre-T0079 form.
    let json = serde_json::to_string(&loaded).unwrap();
    assert!(
        json.contains(r#""syntax_theme_dark":"SolarizedDark""#),
        "{json}"
    );
}

#[test]
fn test_theme_ref_custom_roundtrip() {
    let settings = AppSettings {
        syntax_theme_dark: ThemeRef::Custom("t-my-theme".to_string()),
        ..Default::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    assert!(
        json.contains(r#""syntax_theme_dark":"custom:t-my-theme""#),
        "{json}"
    );
    let loaded = AppSettings::from_json_or_default(&json);
    assert_eq!(
        loaded.syntax_theme_dark,
        ThemeRef::Custom("t-my-theme".to_string())
    );
    assert_eq!(loaded, settings);
}

/// A malformed slot value falls back to that slot's default on its own: the
/// rest of `settings.json` (here a non-default font size) is kept.
#[test]
fn test_theme_ref_malformed_keeps_other_settings() {
    let bad_values = [
        "42",
        r#""custom:../../x""#,
        r#""custom:""#,
        r#""custom:T-UPPER""#,
        r#""NoSuchTheme""#,
        "null",
        r#"{"Builtin": "SolarizedDark"}"#,
    ];
    for bad in bad_values {
        let json = format!(
            r#"{{ "font_size": 23, "syntax_theme_dark": {bad}, "syntax_theme_light": {bad} }}"#
        );
        let loaded = AppSettings::from_json_or_default(&json);
        assert_eq!(loaded.font_size, 23, "font_size lost for slot value {bad}");
        assert_eq!(
            loaded.syntax_theme_dark,
            ThemeRef::Builtin(SyntaxTheme::Base16OceanDark),
            "dark slot for {bad}"
        );
        assert_eq!(
            loaded.syntax_theme_light,
            ThemeRef::Builtin(SyntaxTheme::Base16OceanLight),
            "light slot for {bad}"
        );
    }
}

#[test]
fn test_current_line_default_on() {
    // An old settings.json without the field: on.
    let old: AppSettings = serde_json::from_str(r#"{ "font_size": 14 }"#).unwrap();
    assert!(old.highlight_current_line);
    assert!(AppSettings::default().highlight_current_line);

    // An explicit false roundtrips.
    let off = AppSettings {
        highlight_current_line: false,
        ..Default::default()
    };
    let json = serde_json::to_string(&off).unwrap();
    let loaded: AppSettings = serde_json::from_str(&json).unwrap();
    assert!(!loaded.highlight_current_line);
}

// ---------------------------------------------------------------------------
// Window geometry (T0077)
// ---------------------------------------------------------------------------

fn sample_geometry() -> WindowGeometry {
    WindowGeometry {
        x: -1500,
        y: -20,
        width: 1100,
        height: 750,
        maximized: true,
    }
}

#[test]
fn test_window_geometry_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");

    let settings = AppSettings {
        window: Some(sample_geometry()),
        ..Default::default()
    };
    settings.save_to(&path).unwrap();

    let loaded = AppSettings::load_from(&path);
    assert_eq!(loaded.window, Some(sample_geometry()));
    assert_eq!(loaded, settings);
}

#[test]
fn test_window_geometry_backward_compat() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(
        &path,
        r#"{ "font_size": 18, "line_numbers_enabled": false }"#,
    )
    .unwrap();

    let loaded = AppSettings::load_from(&path);
    assert_eq!(loaded.window, None);
    assert_eq!(loaded.font_size, 18);
    assert!(!loaded.line_numbers_enabled);

    // `None` restores the 640×480, not-maximized default.
    let (rect, maximized) =
        restore_geometry(loaded.window, &[ScreenRect::new(0, 0, 1920, 1050)], 30);
    assert_eq!((rect.w, rect.h), (640, 480));
    assert!(!maximized);
}

#[test]
fn test_window_geometry_malformed_keeps_other_settings() {
    let malformed = [
        r#""window": {"x": 10, "y": 10, "width": "abc", "height": 600, "maximized": false}"#,
        r#""window": {"x": 10, "y": 10, "width": 1e20, "height": 600, "maximized": false}"#,
        r#""window": {"x": 10, "y": 10, "width": 800, "height": 600, "maximized": "yes"}"#,
        r#""window": {"x": 10, "y": 10, "width": 800}"#,
        r#""window": null"#,
        r#""window": [1, 2, 3, 4]"#,
        r#""window": "1100x750""#,
    ];
    for field in malformed {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let json = format!(r#"{{ "font_size": 22, "theme_mode": "Dark", {field} }}"#);
        fs::write(&path, &json).unwrap();

        let loaded = AppSettings::load_from(&path);
        assert_eq!(loaded.window, None, "geometry must be ignored for {field}");
        assert_eq!(loaded.font_size, 22, "font_size lost for {field}");
        assert_eq!(loaded.theme_mode, ThemeMode::Dark, "theme lost for {field}");
    }
}

#[test]
fn test_window_geometry_save_patches_only_geometry() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");

    // Another instance changed the font to 20 after this one started.
    let on_disk = AppSettings {
        font_size: 20,
        ..Default::default()
    };
    on_disk.save_to(&path).unwrap();

    // This instance still holds font_size = 12 in memory.
    let in_memory = AppSettings {
        font_size: 12,
        ..Default::default()
    };
    in_memory
        .save_window_geometry_to(&path, sample_geometry())
        .unwrap();

    let loaded = AppSettings::load_from(&path);
    assert_eq!(loaded.window, Some(sample_geometry()));
    assert_eq!(
        loaded.font_size, 20,
        "close-time save reverted another instance's setting"
    );
}

#[test]
fn test_window_geometry_save_skips_unparseable_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let broken = b"{ \"font_size\": 20, oops this is not json";
    fs::write(&path, broken).unwrap();

    let _ = AppSettings::default().save_window_geometry_to(&path, sample_geometry());

    assert_eq!(fs::read(&path).unwrap(), broken.to_vec());
    // No stray temp file left next to it either.
    let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1);
}

#[test]
fn test_window_geometry_save_creates_missing_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");

    let in_memory = AppSettings {
        font_size: 14,
        ..Default::default()
    };
    in_memory
        .save_window_geometry_to(&path, sample_geometry())
        .unwrap();

    let loaded = AppSettings::load_from(&path);
    assert_eq!(loaded.window, Some(sample_geometry()));
    assert_eq!(loaded.font_size, 14);
}

#[cfg(unix)]
#[test]
fn test_settings_save_keeps_symlink_and_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let dir = tempdir().unwrap();
    let real_dir = dir.path().join("dotfiles");
    fs::create_dir(&real_dir).unwrap();
    let real = real_dir.join("settings.json");
    let link = dir.path().join("settings.json");

    AppSettings::default().save_to(&real).unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&real, &link).unwrap();

    // Both writers go through the atomic write.
    let settings = AppSettings {
        font_size: 21,
        ..Default::default()
    };
    settings.save_to(&link).unwrap();
    settings
        .save_window_geometry_to(&link, sample_geometry())
        .unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the symlink was replaced by a regular file"
    );
    let loaded = AppSettings::load_from(&real);
    assert_eq!(loaded.font_size, 21);
    assert_eq!(loaded.window, Some(sample_geometry()));
    let mode = fs::metadata(&real).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "permissions were not preserved");
    // No temp files left in either directory.
    assert_eq!(fs::read_dir(&real_dir).unwrap().count(), 1);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}
