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

/// Metadata extracted from a plugin's init.lua or plugin.toml
#[derive(Debug, Clone, Default)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: PluginPermissions,
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

    Some(PluginMetadata {
        name,
        version,
        description,
        permissions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
