use std::collections::HashMap;

use ferris_pad::app::domain::settings::ShortcutOverride;
use ferris_pad::app::services::shortcut_registry::{normalize_shortcut, ShortcutRegistry};

/// Simulates BUILTIN_SHORTCUTS from menu.rs
const TEST_DEFAULTS: &[(&str, &str)] = &[
    ("File/New", "Ctrl+T"),
    ("File/Open...", "Ctrl+O"),
    ("File/Save", "Ctrl+S"),
    ("File/Save As...", "Ctrl+Shift+S"),
    ("Edit/Undo", "Ctrl+Z"),
    ("Edit/Redo", "Ctrl+Shift+Z"),
];

#[test]
fn test_settings_to_registry_roundtrip() {
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
            shortcut: "Ctrl+Y".to_string(),
            enabled: true,
        },
    );

    // Settings -> Registry
    let reg = ShortcutRegistry::from_settings(&overrides);

    // Registry -> Settings
    let exported = reg.to_settings();

    // Rebuild registry from exported settings
    let reg2 = ShortcutRegistry::from_settings(&exported);

    // Verify effective shortcuts match
    for &(id, default) in TEST_DEFAULTS {
        assert_eq!(
            reg.effective_shortcut(id, default),
            reg2.effective_shortcut(id, default),
            "Mismatch for {}",
            id
        );
    }
}

#[test]
fn test_override_takes_precedence() {
    let mut reg = ShortcutRegistry::default();
    reg.set_override(
        "File/Save".to_string(),
        ShortcutOverride {
            shortcut: "Ctrl+Shift+S".to_string(),
            enabled: true,
        },
    );

    assert_eq!(
        reg.effective_shortcut("File/Save", "Ctrl+S"),
        "Ctrl+Shift+S"
    );

    // Disable override -> falls back to default
    reg.set_override(
        "File/Save".to_string(),
        ShortcutOverride {
            shortcut: "Ctrl+Shift+S".to_string(),
            enabled: false,
        },
    );
    assert_eq!(reg.effective_shortcut("File/Save", "Ctrl+S"), "Ctrl+S");
}

#[test]
fn test_conflict_detection() {
    let mut reg = ShortcutRegistry::default();
    // Override Edit/Undo to use Ctrl+S (same as File/Save default)
    reg.set_override(
        "Edit/Undo".to_string(),
        ShortcutOverride {
            shortcut: "Ctrl+S".to_string(),
            enabled: true,
        },
    );

    // Trying to assign Ctrl+S to File/Open should detect conflict
    let conflict = reg.find_conflict(
        &normalize_shortcut("Ctrl+S"),
        "File/Open...",
        TEST_DEFAULTS.iter().copied(),
    );
    assert!(conflict.is_some());

    let conflict_id = conflict.unwrap();
    // Could be either File/Save (default) or Edit/Undo (override)
    assert!(
        conflict_id == "File/Save" || conflict_id == "Edit/Undo",
        "Unexpected conflict source: {}",
        conflict_id
    );
}

#[test]
fn test_empty_overrides_use_defaults() {
    let reg = ShortcutRegistry::default();

    let effective = reg.effective_shortcuts(TEST_DEFAULTS.iter().copied());

    assert_eq!(effective.len(), TEST_DEFAULTS.len());
    for &(id, default) in TEST_DEFAULTS {
        assert_eq!(
            effective[id], default,
            "Default mismatch for {}",
            id
        );
    }
}
