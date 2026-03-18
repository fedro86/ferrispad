use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

use ferris_pad::app::domain::settings::{
    AppSettings, FontChoice, PluginApprovals, ShortcutOverride, SyntaxTheme, ThemeMode,
};
use ferris_pad::app::services::session::SessionRestore;
use ferris_pad::app::services::updater::UpdateChannel;

#[test]
fn test_settings_roundtrip_all_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");

    let settings = AppSettings {
        line_numbers_enabled: false,
        word_wrap_enabled: false,
        highlighting_enabled: false,
        theme_mode: ThemeMode::Dark,
        font: FontChoice::HelveticaMono,
        font_size: 20,
        auto_check_updates: false,
        update_channel: UpdateChannel::Beta,
        last_update_check: 1700000000,
        skipped_versions: vec!["0.8.0".to_string(), "0.8.1".to_string()],
        tabs_enabled: true,
        session_restore: SessionRestore::Full,
        preview_enabled: true,
        syntax_theme_light: SyntaxTheme::InspiredGitHub,
        syntax_theme_dark: SyntaxTheme::SolarizedDark,
        tab_size: 2,
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
    assert_eq!(loaded.font, FontChoice::Courier);
    assert!(loaded.auto_check_updates);
    assert_eq!(loaded.update_channel, UpdateChannel::Stable);
    assert!(loaded.plugins_enabled);
    assert!(loaded.shortcut_overrides.is_empty());
    assert!(loaded.plugin_approvals.is_empty());
    assert_eq!(loaded.tab_size, 4);
    assert_eq!(loaded.large_file_warning_mb, 50);
    assert_eq!(loaded.max_editable_size_mb, 150);
}
