//! Plugin update checking service.
//!
//! Checks for available updates to installed plugins by comparing
//! installed versions against the plugin registry.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::plugins::get_plugin_dir;
use crate::app::plugins::loader::{discover_plugins, load_plugin_toml};
use crate::app::services::plugin_registry::{
    PluginTier, fetch_community_registry_cached, fetch_plugin_registry_cached, is_update_available,
    read_plugin_source,
};

/// Information about an available plugin update
#[derive(Debug, Clone)]
pub struct PluginUpdateInfo {
    /// Plugin name
    pub plugin_name: String,
    /// Currently installed version
    pub installed_version: String,
    /// Available version in registry
    pub available_version: String,
}

/// Check if enough time has passed since the last plugin update check.
/// Returns true if 24 hours have passed since `last_check` timestamp.
pub fn should_check_plugin_updates(last_check: i64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let twenty_four_hours = 24 * 60 * 60; // 24 hours in seconds
    (now - last_check) >= twenty_four_hours
}

/// Get the current timestamp as i64 (UNIX epoch seconds)
pub fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Check for available plugin updates.
///
/// Compares installed plugins against the registry and returns
/// a list of plugins that have updates available.
///
/// # Returns
/// * `Ok(Vec<PluginUpdateInfo>)` - List of available updates (may be empty)
/// * `Err(String)` - Error message if check failed
pub fn check_for_plugin_updates() -> Result<Vec<PluginUpdateInfo>, String> {
    // Fetch the plugin registry
    let registry =
        fetch_plugin_registry_cached().map_err(|e| format!("Failed to fetch registry: {}", e))?;

    // Discover installed plugins
    let plugin_dir = get_plugin_dir();
    let installed_plugins = discover_plugins(&plugin_dir);

    let mut updates = Vec::new();

    // For each installed plugin, check if an update is available
    for plugin_path in &installed_plugins {
        // Get the directory name (plugin identifier)
        let dir_name = match plugin_path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Load the installed plugin's metadata
        let metadata = match load_plugin_toml(plugin_path) {
            Some(m) => m,
            None => continue, // Skip plugins without plugin.toml
        };

        // Find this plugin in the registry
        // Match by path (directory name) since name might differ
        let registry_plugin = registry.plugins.iter().find(|p| {
            let registry_dir = p.path.trim_end_matches('/');
            registry_dir == dir_name
        });

        if let Some(available) = registry_plugin
            && is_update_available(&metadata.version, &available.version)
        {
            updates.push(PluginUpdateInfo {
                plugin_name: metadata.name.clone(),
                installed_version: metadata.version.clone(),
                available_version: available.version.clone(),
            });
        }
    }

    // Check community registry for updates to community plugins
    match fetch_community_registry_cached() {
        Ok(community_registry) => {
            // Re-discover installed plugins to check community sources
            for plugin_path in discover_plugins(&plugin_dir) {
                let dir_name = match plugin_path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name,
                    None => continue,
                };

                // Only check plugins with a community .source file
                let source = match read_plugin_source(dir_name) {
                    Some(s) if s.tier == PluginTier::Community => s,
                    _ => continue,
                };

                // Find in community registry by name
                let community_plugin = community_registry
                    .plugins
                    .iter()
                    .find(|p| p.name == dir_name);

                if let Some(available) = community_plugin
                    && is_update_available(&source.installed_version, &available.version)
                {
                    // Don't add duplicates (in case it was already caught by official check)
                    if !updates.iter().any(|u| u.plugin_name == dir_name) {
                        // Load the plugin metadata for display name
                        let display_name = load_plugin_toml(&plugin_path)
                            .map(|m| m.name)
                            .unwrap_or_else(|| dir_name.to_string());
                        updates.push(PluginUpdateInfo {
                            plugin_name: display_name,
                            installed_version: source.installed_version.clone(),
                            available_version: available.version.clone(),
                        });
                    }
                }
            }
        }
        Err(e) => {
            // Community registry fetch failure should not block official updates
            eprintln!(
                "[plugin-update-checker] Community registry fetch failed: {}",
                e
            );
        }
    }

    eprintln!(
        "[plugin-update-checker] Found {} plugin update(s) available",
        updates.len()
    );

    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_check_plugin_updates_first_time() {
        // First check (timestamp 0) should always return true
        assert!(should_check_plugin_updates(0));
    }

    #[test]
    fn test_should_check_plugin_updates_recent() {
        // Recent check should return false
        let now = current_timestamp();
        let one_hour_ago = now - 3600; // 1 hour ago
        assert!(!should_check_plugin_updates(one_hour_ago));
    }

    #[test]
    fn test_should_check_plugin_updates_old() {
        // Old check (25 hours ago) should return true
        let now = current_timestamp();
        let twenty_five_hours_ago = now - (25 * 60 * 60);
        assert!(should_check_plugin_updates(twenty_five_hours_ago));
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        // Should be a reasonable timestamp (after year 2020)
        assert!(ts > 1577836800); // Jan 1, 2020
    }
}
