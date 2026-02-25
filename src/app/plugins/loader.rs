//! Plugin discovery and loading.
//!
//! Plugins are discovered from ~/.config/ferrispad/plugins/
//! Each plugin is a directory containing an init.lua file.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Permissions requested by a plugin in its manifest.
/// These must be approved by the user before the plugin can use them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginPermissions {
    /// Commands this plugin wants to execute via `api:run_command()`
    #[serde(default)]
    pub execute: Vec<String>,
}

/// A custom menu item registered by a plugin in its manifest.
/// Plugins declare these in `[[menu_items]]` arrays in plugin.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginMenuItem {
    /// Display label in menu (e.g., "Run Lint")
    pub label: String,

    /// Action name passed to `on_menu_action` hook
    pub action: String,

    /// Optional keyboard shortcut (e.g., "Ctrl+Shift+P")
    #[serde(default)]
    pub shortcut: Option<String>,
}

/// Definition of a configurable parameter for a plugin.
/// Plugins declare these in `[[config.params]]` arrays in plugin.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigParamDef {
    /// Parameter key used in Lua (e.g., "max_line_length")
    pub key: String,

    /// Human-readable label for the UI
    pub label: String,

    /// Parameter type: "string", "number", or "boolean"
    #[serde(rename = "type", default = "default_param_type")]
    pub param_type: String,

    /// Default value as string
    #[serde(default)]
    pub default: String,

    /// Placeholder text for input fields
    #[serde(default)]
    pub placeholder: Option<String>,
}

fn default_param_type() -> String {
    "string".to_string()
}

/// Plugin configuration schema from manifest.
/// Defines what parameters the plugin accepts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfigDef {
    /// Configurable parameters
    #[serde(default)]
    pub params: Vec<ConfigParamDef>,
}

/// Metadata extracted from a plugin's init.lua or plugin.toml
#[derive(Debug, Clone, Default)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: PluginPermissions,
    /// Custom menu items registered by this plugin
    pub menu_items: Vec<PluginMenuItem>,
    /// Configuration schema for this plugin
    pub config: PluginConfigDef,
}

/// Get the plugin directory path.
/// Returns ~/.config/ferrispad/plugins/ (or platform equivalent)
pub fn get_plugin_dir() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("ferrispad");
    path.push("plugins");
    path
}

/// Discover all plugin directories.
/// Returns paths to directories that contain an init.lua file.
pub fn discover_plugins(dir: &std::path::Path) -> Vec<PathBuf> {
    if !dir.exists() || !dir.is_dir() {
        return Vec::new();
    }

    let mut plugins = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let init_lua = path.join("init.lua");
                if init_lua.exists() && init_lua.is_file() {
                    plugins.push(path);
                }
            }
        }
    }

    // Sort by name for consistent ordering
    plugins.sort();
    plugins
}

/// Load plugin.toml metadata if it exists.
/// Falls back to default values if file doesn't exist or can't be parsed.
pub fn load_plugin_toml(plugin_dir: &std::path::Path) -> Option<PluginMetadata> {
    let toml_path = plugin_dir.join("plugin.toml");
    if !toml_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&toml_path).ok()?;
    let parsed: toml::Value = toml::from_str(&content).ok()?;

    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let version = parsed
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();

    let description = parsed
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Parse [permissions] section
    let permissions = if let Some(perms) = parsed.get("permissions") {
        let execute = perms
            .get("execute")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        PluginPermissions { execute }
    } else {
        PluginPermissions::default()
    };

    // Parse [[menu_items]] array
    let menu_items = parsed
        .get("menu_items")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let label = item.get("label")?.as_str()?.to_string();
                    let action = item.get("action")?.as_str()?.to_string();
                    let shortcut = item
                        .get("shortcut")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    // Sanitize label: no '/' allowed (would create submenu)
                    if label.contains('/') {
                        eprintln!(
                            "[plugins] Warning: menu item label '{}' contains '/', skipping",
                            label
                        );
                        return None;
                    }

                    Some(PluginMenuItem {
                        label,
                        action,
                        shortcut,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Parse [config] section with [[config.params]]
    let config = parsed
        .get("config")
        .map(|cfg| {
            let params = cfg
                .get("params")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|param| {
                            let key = param.get("key")?.as_str()?.to_string();
                            let label = param.get("label")?.as_str()?.to_string();
                            let param_type = param
                                .get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("string")
                                .to_string();
                            let default = param
                                .get("default")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let placeholder = param
                                .get("placeholder")
                                .and_then(|v| v.as_str())
                                .map(String::from);

                            Some(ConfigParamDef {
                                key,
                                label,
                                param_type,
                                default,
                                placeholder,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            PluginConfigDef { params }
        })
        .unwrap_or_default();

    Some(PluginMetadata {
        name,
        version,
        description,
        permissions,
        menu_items,
        config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_get_plugin_dir() {
        let dir = get_plugin_dir();
        assert!(dir.ends_with("ferrispad/plugins"));
    }

    #[test]
    fn test_discover_plugins_nonexistent() {
        let plugins = discover_plugins(std::path::Path::new("/nonexistent/path"));
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_load_plugin_toml_with_menu_items() {
        let dir = tempdir().unwrap();
        let toml_content = r#"
name = "Test Plugin"
version = "1.0.0"
description = "A test plugin"

[permissions]
execute = ["ruff"]

[[menu_items]]
label = "Run Lint"
action = "lint"
shortcut = "Ctrl+Shift+P"

[[menu_items]]
label = "Format Code"
action = "format"
"#;
        let toml_path = dir.path().join("plugin.toml");
        let mut file = std::fs::File::create(&toml_path).unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();

        let metadata = load_plugin_toml(dir.path()).unwrap();

        assert_eq!(metadata.name, "Test Plugin");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.menu_items.len(), 2);

        assert_eq!(metadata.menu_items[0].label, "Run Lint");
        assert_eq!(metadata.menu_items[0].action, "lint");
        assert_eq!(
            metadata.menu_items[0].shortcut,
            Some("Ctrl+Shift+P".to_string())
        );

        assert_eq!(metadata.menu_items[1].label, "Format Code");
        assert_eq!(metadata.menu_items[1].action, "format");
        assert!(metadata.menu_items[1].shortcut.is_none());
    }

    #[test]
    fn test_load_plugin_toml_without_menu_items() {
        let dir = tempdir().unwrap();
        let toml_content = r#"
name = "Simple Plugin"
version = "1.0.0"
"#;
        let toml_path = dir.path().join("plugin.toml");
        let mut file = std::fs::File::create(&toml_path).unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();

        let metadata = load_plugin_toml(dir.path()).unwrap();

        assert_eq!(metadata.name, "Simple Plugin");
        assert!(metadata.menu_items.is_empty());
    }

    #[test]
    fn test_menu_item_label_sanitization() {
        let dir = tempdir().unwrap();
        let toml_content = r#"
name = "Bad Plugin"
version = "1.0.0"

[[menu_items]]
label = "Sub/Menu"
action = "bad"

[[menu_items]]
label = "Good Label"
action = "good"
"#;
        let toml_path = dir.path().join("plugin.toml");
        let mut file = std::fs::File::create(&toml_path).unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();

        let metadata = load_plugin_toml(dir.path()).unwrap();

        // Only the valid menu item should be present
        assert_eq!(metadata.menu_items.len(), 1);
        assert_eq!(metadata.menu_items[0].label, "Good Label");
    }
}
