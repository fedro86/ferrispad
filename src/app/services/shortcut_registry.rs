//! Centralized shortcut registry.
//!
//! Stores user overrides for keyboard shortcuts. Defaults remain in code
//! (BUILTIN_SHORTCUTS in menu.rs, plugin manifests); this registry only
//! holds entries that differ from defaults.

use std::collections::HashMap;

use crate::app::domain::settings::ShortcutOverride;

/// Normalize a shortcut string for comparison (lowercase, sorted modifiers)
pub fn normalize_shortcut(s: &str) -> String {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let mut modifiers: Vec<String> = Vec::new();
    let mut key = String::new();

    for part in parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => modifiers.push("ctrl".to_string()),
            "shift" => modifiers.push("shift".to_string()),
            "alt" => modifiers.push("alt".to_string()),
            _ => key = lower,
        }
    }

    modifiers.sort();
    if !key.is_empty() {
        modifiers.push(key);
    }
    modifiers.join("+")
}

/// Runtime registry of shortcut overrides.
///
/// Keys are command IDs:
/// - Built-in: `"File/Save"`, `"Edit/Undo"`, etc. (menu path)
/// - Plugin:   `"plugin:<plugin_name>:<action>"` e.g. `"plugin:python-lint:lint"`
#[derive(Debug, Clone, Default)]
pub struct ShortcutRegistry {
    overrides: HashMap<String, ShortcutOverride>,
}

impl ShortcutRegistry {
    /// Create a registry from persisted settings.
    pub fn from_settings(overrides: &HashMap<String, ShortcutOverride>) -> Self {
        Self {
            overrides: overrides.clone(),
        }
    }

    /// Get the override for a command ID, if one exists.
    pub fn get_override(&self, id: &str) -> Option<&ShortcutOverride> {
        self.overrides.get(id)
    }

    /// Set an override for a command ID.
    #[allow(dead_code)] // Used in tests
    pub fn set_override(&mut self, id: String, ovr: ShortcutOverride) {
        self.overrides.insert(id, ovr);
    }

    /// Remove an override (revert to default).
    #[allow(dead_code)] // Used in tests
    pub fn remove_override(&mut self, id: &str) {
        self.overrides.remove(id);
    }

    /// Replace all overrides at once (e.g., after dialog save).
    pub fn replace_all(&mut self, overrides: HashMap<String, ShortcutOverride>) {
        self.overrides = overrides;
    }

    /// Return a snapshot of overrides for persistence.
    #[allow(dead_code)] // Used in tests
    pub fn to_settings(&self) -> HashMap<String, ShortcutOverride> {
        self.overrides.clone()
    }

    /// Compute the effective shortcut for a command given its default.
    /// Returns the override shortcut if one exists and is enabled,
    /// otherwise returns the default.
    pub fn effective_shortcut<'a>(&'a self, id: &str, default: &'a str) -> &'a str {
        if let Some(ovr) = self.overrides.get(id)
            && ovr.enabled
        {
            // Return owned string via leak-free approach: caller must handle lifetime
            // Actually we need to return &str, so we return from the stored override
            return &ovr.shortcut;
        }
        default
    }

    /// Build a map of all effective shortcuts: command_id -> normalized shortcut string.
    /// `defaults` is an iterator of (command_id, default_shortcut_string).
    #[allow(dead_code)] // Used in tests
    pub fn effective_shortcuts<'a>(
        &self,
        defaults: impl Iterator<Item = (&'a str, &'a str)>,
    ) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for (id, default) in defaults {
            let effective = self.effective_shortcut(id, default);
            if !effective.is_empty() {
                result.insert(id.to_string(), effective.to_string());
            }
        }
        result
    }

    /// Find a conflict: is `normalized_shortcut` already used by another command?
    /// Returns Some(conflicting_command_id) if conflict found.
    /// `exclude_id` is the command being edited (skip self-conflict).
    /// `defaults` provides (id, default_shortcut) for all commands.
    #[allow(dead_code)] // Used in tests
    pub fn find_conflict<'a>(
        &self,
        normalized: &str,
        exclude_id: &str,
        defaults: impl Iterator<Item = (&'a str, &'a str)>,
    ) -> Option<String> {
        if normalized.is_empty() {
            return None;
        }
        for (id, default) in defaults {
            if id == exclude_id {
                continue;
            }
            let effective = self.effective_shortcut(id, default);
            if !effective.is_empty() && normalize_shortcut(effective) == normalized {
                return Some(id.to_string());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry_returns_default() {
        let reg = ShortcutRegistry::default();
        assert_eq!(reg.effective_shortcut("File/Save", "Ctrl+S"), "Ctrl+S");
    }

    #[test]
    fn test_override_replaces_default() {
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
    }

    #[test]
    fn test_disabled_override_returns_default() {
        let mut reg = ShortcutRegistry::default();
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
    fn test_unbound_override() {
        let mut reg = ShortcutRegistry::default();
        reg.set_override(
            "File/Save".to_string(),
            ShortcutOverride {
                shortcut: String::new(),
                enabled: true,
            },
        );
        // Empty override means unbound
        assert_eq!(reg.effective_shortcut("File/Save", "Ctrl+S"), "");
    }

    #[test]
    fn test_remove_override() {
        let mut reg = ShortcutRegistry::default();
        reg.set_override(
            "File/Save".to_string(),
            ShortcutOverride {
                shortcut: "Ctrl+Shift+S".to_string(),
                enabled: true,
            },
        );
        reg.remove_override("File/Save");
        assert_eq!(reg.effective_shortcut("File/Save", "Ctrl+S"), "Ctrl+S");
    }

    #[test]
    fn test_conflict_detection() {
        let mut reg = ShortcutRegistry::default();
        reg.set_override(
            "Edit/Undo".to_string(),
            ShortcutOverride {
                shortcut: "Ctrl+S".to_string(),
                enabled: true,
            },
        );

        let defaults = [("File/Save", "Ctrl+S"), ("Edit/Undo", "Ctrl+Z")];

        // Check if Ctrl+S conflicts when trying to assign to File/Open
        let conflict = reg.find_conflict("ctrl+s", "File/Open", defaults.iter().copied());
        // Both File/Save (default) and Edit/Undo (override) have Ctrl+S,
        // first encountered wins
        assert!(conflict.is_some());
        let conflict_id = conflict.unwrap();
        assert!(conflict_id == "File/Save" || conflict_id == "Edit/Undo");
    }

    #[test]
    fn test_no_self_conflict() {
        let reg = ShortcutRegistry::default();
        let defaults = [("File/Save", "Ctrl+S")];

        // File/Save checking Ctrl+S should not conflict with itself
        let conflict = reg.find_conflict("ctrl+s", "File/Save", defaults.iter().copied());
        assert!(conflict.is_none());
    }

    #[test]
    fn test_from_settings() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "File/Save".to_string(),
            ShortcutOverride {
                shortcut: "Ctrl+Shift+S".to_string(),
                enabled: true,
            },
        );
        let reg = ShortcutRegistry::from_settings(&overrides);
        assert_eq!(
            reg.effective_shortcut("File/Save", "Ctrl+S"),
            "Ctrl+Shift+S"
        );
    }

    #[test]
    fn test_to_settings_roundtrip() {
        let mut reg = ShortcutRegistry::default();
        reg.set_override(
            "File/Save".to_string(),
            ShortcutOverride {
                shortcut: "Ctrl+Shift+S".to_string(),
                enabled: true,
            },
        );
        let exported = reg.to_settings();
        let reg2 = ShortcutRegistry::from_settings(&exported);
        assert_eq!(
            reg2.effective_shortcut("File/Save", "Ctrl+S"),
            "Ctrl+Shift+S"
        );
    }
}
