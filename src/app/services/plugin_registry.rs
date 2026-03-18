//! Plugin registry service for fetching and installing plugins from the official repository
//! and community sources.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::app::infrastructure::error::AppError;
use crate::app::plugins::get_plugin_dir;
use crate::app::services::plugin_verify::{
    LuaScanResult, VerificationStatus, scan_lua_source, verify_checksum, verify_plugin,
};

/// URL to the official plugin registry
const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/fedro86/ferrispad-plugins/main/plugins.json";

/// Base URL for downloading plugin files
const REPO_RAW_BASE: &str = "https://raw.githubusercontent.com/fedro86/ferrispad-plugins/main/";

/// URL to the community plugin registry
pub const COMMUNITY_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/fedro86/ferrispad-plugins/main/community-plugins.json";

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

// ---------------------------------------------------------------------------
// Plugin tier and source tracking
// ---------------------------------------------------------------------------

/// Indicates the trust tier of an installed plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginTier {
    /// Plugin from the official FerrisPad registry (signed)
    Official,
    /// Plugin from a community registry entry (reviewed)
    Community,
    /// Manually installed plugin (unknown provenance)
    Manual,
}

impl fmt::Display for PluginTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PluginTier::Official => "official",
            PluginTier::Community => "community",
            PluginTier::Manual => "manual",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for PluginTier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "official" => Ok(PluginTier::Official),
            "community" => Ok(PluginTier::Community),
            "manual" => Ok(PluginTier::Manual),
            other => Err(format!("Unknown plugin tier: {}", other)),
        }
    }
}

/// Metadata written to `{plugin_dir}/.source` to record installation provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSource {
    /// Trust tier of the plugin
    pub tier: PluginTier,
    /// Repository URL (empty for manual installs)
    pub repo: String,
    /// Version string at install time
    pub installed_version: String,
    /// ISO-8601 date when the plugin was installed
    pub installed_date: String,
}

/// The community plugin registry
#[derive(Debug, Deserialize)]
pub struct CommunityRegistry {
    /// Schema version
    pub version: u32,
    /// Last update date
    pub updated: String,
    /// List of community plugins
    pub plugins: Vec<CommunityPluginInfo>,
}

/// Information about a community plugin
#[derive(Debug, Clone, Deserialize)]
pub struct CommunityPluginInfo {
    /// Plugin name (directory name)
    pub name: String,
    /// Repository URL
    pub repo: String,
    /// Git branch to fetch from
    #[serde(default = "default_branch")]
    pub branch: String,
    /// Plugin version
    pub version: String,
    /// Short description
    pub description: String,
    /// Author name
    pub author: String,
    /// License identifier
    pub license: String,
    /// Minimum FerrisPad version required
    pub min_ferrispad_version: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Optional SHA-256 checksums of plugin files
    #[serde(default)]
    pub checksums: Option<PluginChecksums>,
}

fn default_branch() -> String {
    "main".to_string()
}

// ---------------------------------------------------------------------------
// .source file I/O
// ---------------------------------------------------------------------------

/// Read the `.source` provenance file for an installed plugin.
///
/// Returns `None` if the file does not exist or cannot be parsed.
pub fn read_plugin_source(plugin_name: &str) -> Option<PluginSource> {
    let path = get_plugin_dir().join(plugin_name).join(".source");
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// Write the `.source` provenance file for an installed plugin.
pub fn write_plugin_source(plugin_name: &str, source: &PluginSource) -> Result<(), AppError> {
    let dir = get_plugin_dir().join(plugin_name);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(".source");
    let content = toml::to_string(source)
        .map_err(|e| AppError::Network(format!("Failed to serialize .source: {}", e)))?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Determine the trust tier of an installed plugin by reading its `.source` file.
///
/// Falls back to `Manual` if the file is missing or unreadable.
pub fn determine_plugin_tier(plugin_name: &str) -> PluginTier {
    read_plugin_source(plugin_name)
        .map(|s| s.tier)
        .unwrap_or(PluginTier::Manual)
}

// ---------------------------------------------------------------------------
// Date helper (no chrono dependency)
// ---------------------------------------------------------------------------

/// Return today's date as an ISO-8601 string (YYYY-MM-DD).
///
/// Uses the civil-date algorithm from Howard Hinnant to avoid pulling in chrono.
fn today_date_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs / 86400;
    // Algorithm: http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

// ---------------------------------------------------------------------------
// Registry fetch
// ---------------------------------------------------------------------------

/// Fetch the official plugin registry from GitHub
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

/// Fetch the community plugin registry from GitHub
pub fn fetch_community_registry() -> Result<CommunityRegistry, AppError> {
    let response = minreq::get(COMMUNITY_REGISTRY_URL)
        .with_header("User-Agent", "FerrisPad")
        .with_timeout(10)
        .send()
        .map_err(|e| AppError::Network(format!("Failed to fetch community registry: {}", e)))?;

    if response.status_code == 403 || response.status_code == 429 {
        return Err(AppError::Network(
            "GitHub rate limit reached. Try again in a few minutes.".to_string(),
        ));
    }

    if response.status_code != 200 {
        return Err(AppError::Network(format!(
            "HTTP {} fetching community registry",
            response.status_code
        )));
    }

    response
        .json()
        .map_err(|e| AppError::Network(format!("Invalid JSON in community registry: {}", e)))
}

// ---------------------------------------------------------------------------
// File download
// ---------------------------------------------------------------------------

/// Fetch a single file from a URL as a string
fn fetch_file(url: &str) -> Result<String, AppError> {
    let response = minreq::get(url)
        .with_header("User-Agent", "FerrisPad")
        .with_timeout(30)
        .send()
        .map_err(|e| AppError::Network(format!("Download failed: {}", e)))?;

    if response.status_code == 403 || response.status_code == 429 {
        return Err(AppError::Network(
            "GitHub rate limit reached. Try again in a few minutes.".to_string(),
        ));
    }

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

/// Fetch a file from a URL as raw bytes
fn fetch_file_bytes(url: &str) -> Result<Vec<u8>, AppError> {
    let response = minreq::get(url)
        .with_header("User-Agent", "FerrisPad")
        .with_timeout(30)
        .send()
        .map_err(|e| AppError::Network(format!("Download failed: {}", e)))?;

    if response.status_code == 403 || response.status_code == 429 {
        return Err(AppError::Network(
            "GitHub rate limit reached. Try again in a few minutes.".to_string(),
        ));
    }

    if response.status_code != 200 {
        return Err(AppError::Network(format!(
            "HTTP {} downloading {}",
            response.status_code, url
        )));
    }

    Ok(response.as_bytes().to_vec())
}

// ---------------------------------------------------------------------------
// Size limit enforcement
// ---------------------------------------------------------------------------

/// Enforce a maximum byte-size limit on downloaded content.
///
/// Returns `Err(AppError::Network)` if the data exceeds `max_bytes`.
pub fn enforce_size_limit(data: &[u8], max_bytes: usize, file_name: &str) -> Result<(), AppError> {
    if data.len() > max_bytes {
        return Err(AppError::Network(format!(
            "{} exceeds size limit ({} bytes > {} bytes)",
            file_name,
            data.len(),
            max_bytes
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// GitHub URL parsing
// ---------------------------------------------------------------------------

/// Extract `(owner, repo)` from a GitHub URL.
///
/// Handles variations like trailing `/`, `.git` suffix.
/// Returns an error for non-GitHub URLs.
pub fn parse_github_url(url: &str) -> Result<(String, String), AppError> {
    let stripped = url.trim_end_matches('/');
    let stripped = stripped.strip_suffix(".git").unwrap_or(stripped);

    // Expect https://github.com/{owner}/{repo}
    let prefix = "https://github.com/";
    let path = stripped
        .strip_prefix(prefix)
        .ok_or_else(|| AppError::Network(format!("Not a GitHub URL: {}", url)))?;

    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AppError::Network(format!(
            "Could not parse owner/repo from GitHub URL: {}",
            url
        )));
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

// ---------------------------------------------------------------------------
// Community plugin helpers
// ---------------------------------------------------------------------------

/// Fetch the default branch name for a GitHub repository via the API.
///
/// Falls back to `"main"` if the API call fails (e.g. rate-limited).
pub fn fetch_default_branch(repo_url: &str) -> String {
    let Ok((owner, repo)) = parse_github_url(repo_url) else {
        return "main".to_string();
    };
    let api_url = format!("https://api.github.com/repos/{}/{}", owner, repo);
    let response = minreq::get(&api_url)
        .with_header("User-Agent", "FerrisPad")
        .with_header("Accept", "application/vnd.github.v3+json")
        .with_timeout(10)
        .send();
    match response {
        Ok(resp) if resp.status_code == 200 => {
            let body = resp.as_str().unwrap_or("");
            // Lightweight extraction — avoid pulling in a JSON library
            if let Some(pos) = body.find("\"default_branch\"") {
                let rest = &body[pos..];
                if let Some(start) = rest.find(':') {
                    let after_colon = rest[start + 1..].trim_start();
                    if let Some(branch) = after_colon
                        .strip_prefix('"')
                        .and_then(|s| s.split('"').next())
                    {
                        return branch.to_string();
                    }
                }
            }
            "main".to_string()
        }
        _ => "main".to_string(),
    }
}

/// Fetch the `plugin.toml` from a community plugin's GitHub repository.
///
/// Enforces a 10 KB size limit to prevent abuse.
pub fn fetch_community_plugin_toml(repo_url: &str, branch: &str) -> Result<String, AppError> {
    let (owner, repo) = parse_github_url(repo_url)?;
    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/{}/plugin.toml",
        owner, repo, branch
    );
    let data = fetch_file_bytes(&url)?;
    enforce_size_limit(&data, 10 * 1024, "plugin.toml")?;
    String::from_utf8(data)
        .map_err(|e| AppError::Network(format!("plugin.toml is not valid UTF-8: {}", e)))
}

// ---------------------------------------------------------------------------
// Official plugin installation
// ---------------------------------------------------------------------------

/// Install a plugin from the official registry with verification
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
        Some(checksums) => (
            Some(checksums.init_lua.as_str()),
            Some(checksums.plugin_toml.as_str()),
        ),
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

    // Write .source provenance file
    let source = PluginSource {
        tier: PluginTier::Official,
        repo: "https://github.com/fedro86/ferrispad-plugins".to_string(),
        installed_version: plugin_info.version.clone(),
        installed_date: today_date_string(),
    };
    let _ = write_plugin_source(dir_name, &source);

    eprintln!(
        "[plugins] Installed {} v{} to {:?} ({})",
        plugin_info.name,
        plugin_info.version,
        plugin_dir,
        verification_status.display()
    );

    Ok(verification_status)
}

// ---------------------------------------------------------------------------
// Community plugin installation
// ---------------------------------------------------------------------------

/// Install a community plugin from a GitHub repository.
///
/// Downloads `init.lua` (100 KB limit), verifies checksums if provided,
/// writes plugin files and a `.source` provenance file.
pub fn install_community_plugin(
    name: &str,
    repo_url: &str,
    branch: &str,
    plugin_toml_content: &str,
    tier: PluginTier,
    checksums: Option<&PluginChecksums>,
) -> Result<(), AppError> {
    let (owner, repo) = parse_github_url(repo_url)?;
    let init_lua_url = format!(
        "https://raw.githubusercontent.com/{}/{}/{}/init.lua",
        owner, repo, branch
    );

    let init_lua_bytes = fetch_file_bytes(&init_lua_url)?;
    enforce_size_limit(&init_lua_bytes, 100 * 1024, "init.lua")?;

    let init_lua_content = String::from_utf8(init_lua_bytes.clone())
        .map_err(|e| AppError::Network(format!("init.lua is not valid UTF-8: {}", e)))?;

    // Verify checksums if provided
    if let Some(cs) = checksums {
        verify_checksum(init_lua_content.as_bytes(), &cs.init_lua, "init.lua")?;
        verify_checksum(
            plugin_toml_content.as_bytes(),
            &cs.plugin_toml,
            "plugin.toml",
        )?;
    }

    // Static Lua analysis — reject if blocked patterns are found
    if let LuaScanResult::Blocked(reasons) = scan_lua_source(&init_lua_content) {
        return Err(AppError::Network(format!(
            "Plugin blocked by security scan:\n{}",
            reasons.join("\n")
        )));
    }

    // Write files
    let plugin_dir = get_plugin_dir().join(name);
    std::fs::create_dir_all(&plugin_dir)?;

    std::fs::write(plugin_dir.join("init.lua"), &init_lua_content)?;
    std::fs::write(plugin_dir.join("plugin.toml"), plugin_toml_content)?;

    // Try to download README.md (optional)
    let readme_url = format!(
        "https://raw.githubusercontent.com/{}/{}/{}/README.md",
        owner, repo, branch
    );
    if let Ok(readme) = fetch_file(&readme_url) {
        let _ = std::fs::write(plugin_dir.join("README.md"), &readme);
    }

    // Parse version from plugin.toml for the .source record
    let version = toml::from_str::<toml::Value>(plugin_toml_content)
        .ok()
        .and_then(|v| v.get("version")?.as_str().map(String::from))
        .unwrap_or_default();

    let source = PluginSource {
        tier,
        repo: repo_url.to_string(),
        installed_version: version.clone(),
        installed_date: today_date_string(),
    };
    write_plugin_source(name, &source)?;

    eprintln!(
        "[plugins] Installed community plugin {} v{} from {}",
        name, version, repo_url
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

/// Check if a plugin is already installed
pub fn is_plugin_installed(plugin_name: &str) -> bool {
    let plugin_dir = get_plugin_dir().join(plugin_name);
    plugin_dir.join("init.lua").exists()
}

/// Compare version strings (simple semver comparison)
/// Returns true if available > installed
pub fn is_update_available(installed_version: &str, available_version: &str) -> bool {
    let parse_version =
        |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse().ok()).collect() };

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    #[test]
    fn test_parse_github_url_basic() {
        let (owner, repo) = parse_github_url("https://github.com/someuser/somerepo").unwrap();
        assert_eq!(owner, "someuser");
        assert_eq!(repo, "somerepo");
    }

    #[test]
    fn test_parse_github_url_trailing_slash() {
        let (owner, repo) = parse_github_url("https://github.com/someuser/somerepo/").unwrap();
        assert_eq!(owner, "someuser");
        assert_eq!(repo, "somerepo");
    }

    #[test]
    fn test_parse_github_url_git_suffix() {
        let (owner, repo) = parse_github_url("https://github.com/someuser/somerepo.git").unwrap();
        assert_eq!(owner, "someuser");
        assert_eq!(repo, "somerepo");
    }

    #[test]
    fn test_parse_github_url_invalid() {
        assert!(parse_github_url("https://gitlab.com/user/repo").is_err());
        assert!(parse_github_url("not a url").is_err());
    }

    #[test]
    fn test_enforce_size_limit_ok() {
        assert!(enforce_size_limit(b"hello", 10, "test.lua").is_ok());
    }

    #[test]
    fn test_enforce_size_limit_exceeded() {
        assert!(enforce_size_limit(b"hello world", 5, "test.lua").is_err());
    }

    #[test]
    fn test_plugin_source_roundtrip() {
        let source = PluginSource {
            tier: PluginTier::Community,
            repo: "https://github.com/test/repo".to_string(),
            installed_version: "1.0.0".to_string(),
            installed_date: "2026-03-17".to_string(),
        };
        let toml_str = toml::to_string(&source).unwrap();
        let parsed: PluginSource = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.tier, PluginTier::Community);
        assert_eq!(parsed.repo, "https://github.com/test/repo");
    }

    #[test]
    fn test_plugin_tier_display() {
        assert_eq!(PluginTier::Official.to_string(), "official");
        assert_eq!(PluginTier::Community.to_string(), "community");
        assert_eq!(PluginTier::Manual.to_string(), "manual");
    }

    #[test]
    fn test_plugin_tier_from_str() {
        assert_eq!(
            "official".parse::<PluginTier>().unwrap(),
            PluginTier::Official
        );
        assert_eq!(
            "Community".parse::<PluginTier>().unwrap(),
            PluginTier::Community
        );
        assert_eq!("MANUAL".parse::<PluginTier>().unwrap(), PluginTier::Manual);
        assert!("unknown".parse::<PluginTier>().is_err());
    }

    #[test]
    fn test_plugin_tier_serde_lowercase() {
        let source = PluginSource {
            tier: PluginTier::Official,
            repo: String::new(),
            installed_version: "1.0.0".to_string(),
            installed_date: "2026-01-01".to_string(),
        };
        let toml_str = toml::to_string(&source).unwrap();
        assert!(
            toml_str.contains("tier = \"official\""),
            "Expected lowercase tier in TOML: {}",
            toml_str
        );
    }

    #[test]
    fn test_today_date_string_format() {
        let date = today_date_string();
        // Should be YYYY-MM-DD format
        assert_eq!(date.len(), 10);
        assert_eq!(date.as_bytes()[4], b'-');
        assert_eq!(date.as_bytes()[7], b'-');
    }
}
