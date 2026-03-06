//! Plugin registry service for fetching and installing plugins from the official repository.

use serde::Deserialize;

use crate::app::infrastructure::error::AppError;
use crate::app::plugins::get_plugin_dir;
use crate::app::services::plugin_verify::{verify_plugin, VerificationStatus};

/// URL to the official plugin registry
const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/fedro86/ferrispad-plugins/main/plugins.json";

/// Base URL for downloading plugin files
const REPO_RAW_BASE: &str = "https://raw.githubusercontent.com/fedro86/ferrispad-plugins/main/";

/// The plugin registry containing available plugins
#[derive(Debug, Deserialize)]
pub struct PluginRegistry {
    /// Schema version
    #[serde(rename = "version")]
    pub _version: u32,
    /// Last update date
    #[serde(rename = "updated")]
    pub _updated: String,
    /// List of available plugins
    pub plugins: Vec<AvailablePluginInfo>,
}

/// Information about an available plugin in the registry
#[derive(Debug, Clone, Deserialize)]
pub struct AvailablePluginInfo {
    /// Plugin name (directory name in repo)
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Short description
    pub description: String,
    /// Relative path in the repo (e.g., "python-lint/")
    pub path: String,
    /// Author name
    pub author: String,
    /// License identifier
    #[serde(rename = "license")]
    pub _license: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Minimum FerrisPad version required
    #[serde(rename = "min_ferrispad_version")]
    pub _min_ferrispad_version: String,
    /// SHA-256 checksums of plugin files (optional, for verification)
    #[serde(default)]
    pub checksums: Option<PluginChecksums>,
    /// ed25519 signature of the plugin (optional, base64-encoded)
    #[serde(default)]
    pub signature: Option<String>,
    /// URL to the plugin's README (e.g., GitHub page)
    #[serde(default)]
    pub readme_url: Option<String>,
}

/// SHA-256 checksums for plugin files
#[derive(Debug, Clone, Deserialize)]
pub struct PluginChecksums {
    /// Checksum of init.lua in format "sha256:hexstring"
    #[serde(rename = "init.lua")]
    pub init_lua: String,
    /// Checksum of plugin.toml in format "sha256:hexstring"
    #[serde(rename = "plugin.toml")]
    pub plugin_toml: String,
}

impl AvailablePluginInfo {
    /// Check if this plugin has verification data (checksums and signature)
    pub fn is_verified(&self) -> bool {
        self.checksums.is_some() && self.signature.is_some()
    }
}

/// Fetch the plugin registry from GitHub
pub fn fetch_plugin_registry() -> Result<PluginRegistry, AppError> {
    let response = minreq::get(REGISTRY_URL)
        .with_header("User-Agent", "FerrisPad")
        .with_timeout(10)
        .send()
        .map_err(|e| AppError::Network(format!("Failed to fetch plugin registry: {}", e)))?;

    if response.status_code != 200 {
        return Err(AppError::Network(format!(
            "HTTP {} fetching registry",
            response.status_code
        )));
    }

    response
        .json()
        .map_err(|e| AppError::Network(format!("Invalid JSON in registry: {}", e)))
}

/// Fetch a single file from a URL
fn fetch_file(url: &str) -> Result<String, AppError> {
    let response = minreq::get(url)
        .with_header("User-Agent", "FerrisPad")
        .with_timeout(30)
        .send()
        .map_err(|e| AppError::Network(format!("Download failed: {}", e)))?;

    if response.status_code != 200 {
        return Err(AppError::Network(format!(
            "HTTP {} downloading {}",
            response.status_code, url
        )));
    }

    response
        .as_str()
        .map(|s| s.to_string())
        .map_err(|e| AppError::Network(format!("Invalid response encoding: {}", e)))
}

/// Install a plugin from the registry with verification
///
/// Downloads init.lua and plugin.toml, verifies checksums and signature,
/// then writes to the plugins directory.
///
/// # Returns
/// * `Ok(VerificationStatus)` - Installation succeeded with verification status
/// * `Err(AppError)` - Installation failed (network, checksum, or file error)
pub fn install_plugin(plugin_info: &AvailablePluginInfo) -> Result<VerificationStatus, AppError> {
    // Derive directory name from path (e.g., "python-lint/" -> "python-lint")
    let dir_name = plugin_info.path.trim_end_matches('/');
    let plugin_dir = get_plugin_dir().join(dir_name);
    let base_url = format!("{}{}", REPO_RAW_BASE, plugin_info.path);

    // Download files to memory first (don't write until verified)
    let init_lua_url = format!("{}init.lua", base_url);
    let init_lua_content = fetch_file(&init_lua_url)?;

    let plugin_toml_url = format!("{}plugin.toml", base_url);
    let plugin_toml_content = fetch_file(&plugin_toml_url)?;

    // Verify checksums and signature
    let (expected_init, expected_toml) = match &plugin_info.checksums {
        Some(checksums) => (Some(checksums.init_lua.as_str()), Some(checksums.plugin_toml.as_str())),
        None => (None, None),
    };

    let verification_status = verify_plugin(
        &plugin_info.path,
        &plugin_info.version,
        init_lua_content.as_bytes(),
        plugin_toml_content.as_bytes(),
        expected_init,
        expected_toml,
        plugin_info.signature.as_deref(),
    )?;

    // Don't install if signature is invalid
    if !verification_status.allows_install() {
        return Ok(verification_status);
    }

    // Verification passed (or unverified) - now write files
    std::fs::create_dir_all(&plugin_dir)?;

    let init_path = plugin_dir.join("init.lua");
    std::fs::write(&init_path, &init_lua_content)?;

    let toml_path = plugin_dir.join("plugin.toml");
    std::fs::write(&toml_path, &plugin_toml_content)?;

    // Try to download README.md (optional, don't fail if missing)
    let readme_url = format!("{}README.md", base_url);
    if let Ok(readme) = fetch_file(&readme_url) {
        let readme_path = plugin_dir.join("README.md");
        let _ = std::fs::write(&readme_path, &readme);
    }

    eprintln!(
        "[plugins] Installed {} v{} to {:?} ({})",
        plugin_info.name, plugin_info.version, plugin_dir, verification_status.display()
    );

    Ok(verification_status)
}

/// Check if a plugin is already installed
pub fn is_plugin_installed(plugin_name: &str) -> bool {
    let plugin_dir = get_plugin_dir().join(plugin_name);
    plugin_dir.join("init.lua").exists()
}

/// Compare version strings (simple semver comparison)
/// Returns true if available > installed
pub fn is_update_available(installed_version: &str, available_version: &str) -> bool {
    let parse_version = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    let installed = parse_version(installed_version);
    let available = parse_version(available_version);

    // Compare component by component
    for (i, av) in available.iter().enumerate() {
        let iv = installed.get(i).copied().unwrap_or(0);
        if *av > iv {
            return true;
        } else if *av < iv {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_update_available() {
        assert!(is_update_available("1.0.0", "1.0.1"));
        assert!(is_update_available("1.0.0", "1.1.0"));
        assert!(is_update_available("1.0.0", "2.0.0"));
        assert!(!is_update_available("1.0.0", "1.0.0"));
        assert!(!is_update_available("1.0.1", "1.0.0"));
        assert!(!is_update_available("2.0.0", "1.9.9"));
    }

    #[test]
    fn test_is_update_available_partial_versions() {
        assert!(is_update_available("1.0", "1.0.1"));
        assert!(is_update_available("1", "1.1"));
        assert!(!is_update_available("1.0.1", "1.0"));
    }
}
