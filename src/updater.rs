use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum UpdateChannel {
    Stable,
    Beta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub name: String,
    #[serde(default)]
    pub body: String,
    pub html_url: String,
    pub published_at: String,
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

impl ReleaseInfo {
    /// Get the version string (tag_name without 'v' prefix)
    pub fn version(&self) -> String {
        self.tag_name.trim_start_matches('v').to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub enum UpdateCheckResult {
    NoUpdate,
    UpdateAvailable(ReleaseInfo),
    Error(String),
}

/// Compare two semantic versions
/// Returns true if remote is newer than current
pub fn is_newer_version(current: &str, remote: &str) -> bool {
    match (semver::Version::parse(current), semver::Version::parse(remote)) {
        (Ok(curr), Ok(rem)) => rem > curr,
        _ => false, // If parsing fails, assume not newer
    }
}

/// Check if enough time has passed since last check (24 hours)
pub fn should_check_now(last_check_timestamp: i64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let twenty_four_hours = 24 * 60 * 60; // 24 hours in seconds
    (now - last_check_timestamp) >= twenty_four_hours
}

/// Fetch the latest release from GitHub
pub fn fetch_latest_release(
    owner: &str,
    repo: &str,
    channel: UpdateChannel,
) -> Result<ReleaseInfo, String> {
    let url = match channel {
        UpdateChannel::Stable => {
            format!("https://api.github.com/repos/{}/{}/releases/latest", owner, repo)
        }
        UpdateChannel::Beta => {
            // For beta channel, we fetch all releases and get the most recent one
            format!("https://api.github.com/repos/{}/{}/releases", owner, repo)
        }
    };

    let client = reqwest::blocking::Client::builder()
        .user_agent("FerrisPad")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("Failed to connect to update server: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Update server returned error: {}",
            response.status()
        ));
    }

    match channel {
        UpdateChannel::Stable => {
            // For stable, the API returns a single release
            let release: ReleaseInfo = response
                .json()
                .map_err(|e| format!("Failed to parse update information: {}", e))?;
            Ok(release)
        }
        UpdateChannel::Beta => {
            // For beta, get the first release from the list (most recent)
            let releases: Vec<ReleaseInfo> = response
                .json()
                .map_err(|e| format!("Failed to parse update information: {}", e))?;

            releases.into_iter()
                .next()
                .ok_or_else(|| "No releases found".to_string())
        }
    }
}

/// Check for updates given current version and settings
pub fn check_for_updates(
    current_version: &str,
    channel: UpdateChannel,
    skipped_versions: &[String],
) -> UpdateCheckResult {
    // Fetch latest release from GitHub
    let release = match fetch_latest_release("fedro86", "ferrispad", channel) {
        Ok(r) => r,
        Err(e) => return UpdateCheckResult::Error(e),
    };

    // Extract version from tag_name (remove 'v' prefix if present)
    let remote_version = release.tag_name.trim_start_matches('v');

    // Check if this version is skipped by user
    if skipped_versions.iter().any(|v| v == remote_version) {
        return UpdateCheckResult::NoUpdate;
    }

    // Compare versions
    if is_newer_version(current_version, remote_version) {
        UpdateCheckResult::UpdateAvailable(release)
    } else {
        UpdateCheckResult::NoUpdate
    }
}

/// Get current Unix timestamp
pub fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison_newer() {
        assert!(is_newer_version("0.1.4", "0.1.5"));
        assert!(is_newer_version("0.1.4", "0.2.0"));
        assert!(is_newer_version("0.1.4", "1.0.0"));
    }

    #[test]
    fn test_version_comparison_same() {
        assert!(!is_newer_version("0.1.5", "0.1.5"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_version_comparison_older() {
        assert!(!is_newer_version("0.1.5", "0.1.4"));
        assert!(!is_newer_version("1.0.0", "0.9.9"));
    }

    #[test]
    fn test_version_comparison_prerelease() {
        // Prereleases are considered lower than releases
        assert!(is_newer_version("0.1.4", "0.1.5-beta.1"));
        assert!(is_newer_version("0.1.5-beta.1", "0.1.5"));
        assert!(!is_newer_version("0.1.5", "0.1.5-beta.1"));
    }

    #[test]
    fn test_version_comparison_invalid() {
        // Invalid versions should return false
        assert!(!is_newer_version("invalid", "0.1.5"));
        assert!(!is_newer_version("0.1.4", "invalid"));
        assert!(!is_newer_version("invalid", "invalid"));
    }

    #[test]
    fn test_should_check_now_yes() {
        // 25 hours ago
        let twenty_five_hours_ago = current_timestamp() - (25 * 60 * 60);
        assert!(should_check_now(twenty_five_hours_ago));
    }

    #[test]
    fn test_should_check_now_no() {
        // 1 hour ago
        let one_hour_ago = current_timestamp() - (1 * 60 * 60);
        assert!(!should_check_now(one_hour_ago));
    }

    #[test]
    fn test_should_check_now_exactly_24h() {
        // Exactly 24 hours - should return true
        let exactly_24h_ago = current_timestamp() - (24 * 60 * 60);
        assert!(should_check_now(exactly_24h_ago));
    }

    #[test]
    fn test_should_check_now_never_checked() {
        // Never checked before (timestamp = 0)
        assert!(should_check_now(0));
    }

    #[test]
    fn test_release_info_serialization() {
        let release = ReleaseInfo {
            tag_name: "0.1.5".to_string(),
            name: "Release 0.1.5".to_string(),
            body: "Test release".to_string(),
            html_url: "https://github.com/test/test/releases/tag/0.1.5".to_string(),
            published_at: "2025-10-02T00:00:00Z".to_string(),
            prerelease: false,
            assets: vec![],
        };

        let json = serde_json::to_string(&release).unwrap();
        let parsed: ReleaseInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(release.version(), parsed.version());
        assert_eq!(release.tag_name, parsed.tag_name);
        assert_eq!(release.version(), "0.1.5");
    }

    #[test]
    fn test_update_channel_serialization() {
        let stable = UpdateChannel::Stable;
        let beta = UpdateChannel::Beta;

        let stable_json = serde_json::to_string(&stable).unwrap();
        let beta_json = serde_json::to_string(&beta).unwrap();

        assert!(stable_json.contains("Stable"));
        assert!(beta_json.contains("Beta"));
    }
}
