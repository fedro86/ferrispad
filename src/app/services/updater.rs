use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::infrastructure::error::AppError;

/// FerrisPad's official signing public key (same key used for plugin signing).
/// This key verifies both plugin signatures and release binary signatures.
const SIGNING_PUBLIC_KEY: [u8; 32] = [
    0x7f, 0x14, 0x24, 0xc5, 0x14, 0x5d, 0x99, 0xd7,
    0xf0, 0xfd, 0x7c, 0x12, 0xdd, 0x5c, 0x3d, 0x8b,
    0x8f, 0x6d, 0x6b, 0x4e, 0xe7, 0xba, 0xb1, 0x2c,
    0xd0, 0xdb, 0xdf, 0xbc, 0x17, 0x54, 0xff, 0x56,
];

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
) -> Result<ReleaseInfo, AppError> {
    let url = match channel {
        UpdateChannel::Stable => {
            format!("https://api.github.com/repos/{}/{}/releases/latest", owner, repo)
        }
        UpdateChannel::Beta => {
            format!("https://api.github.com/repos/{}/{}/releases", owner, repo)
        }
    };

    let response = minreq::get(&url)
        .with_header("User-Agent", "FerrisPad")
        .with_timeout(10)
        .send()
        .map_err(|e| AppError::Update(format!("Failed to connect to update server: {}", e)))?;

    if response.status_code < 200 || response.status_code >= 300 {
        return Err(AppError::Update(format!(
            "Update server returned error: {}",
            response.status_code
        )));
    }

    match channel {
        UpdateChannel::Stable => {
            let release: ReleaseInfo = response
                .json()
                .map_err(|e| AppError::Update(format!("Failed to parse update information: {}", e)))?;
            Ok(release)
        }
        UpdateChannel::Beta => {
            let releases: Vec<ReleaseInfo> = response
                .json()
                .map_err(|e| AppError::Update(format!("Failed to parse update information: {}", e)))?;

            releases.into_iter()
                .next()
                .ok_or_else(|| AppError::Update("No releases found".to_string()))
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
        Err(e) => return UpdateCheckResult::Error(e.to_string()),
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

/// Get the expected asset name for the current platform
pub fn get_platform_asset_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos-universal"
    } else if cfg!(target_os = "windows") {
        "windows-x64.exe"
    } else {
        "linux-amd64"
    }
}

/// Compute SHA-256 checksum of binary data
fn compute_checksum(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Verify a binary's signature
///
/// The signature is over the message: "{version}:{platform}:{sha256_hex}"
/// This ties the signature to a specific version and platform.
fn verify_binary_signature(
    binary_data: &[u8],
    version: &str,
    platform: &str,
    signature_b64: &str,
) -> Result<(), AppError> {
    // Compute checksum of the binary
    let checksum = compute_checksum(binary_data);

    // Build the message that was signed
    let message = format!("{}:{}:{}", version, platform, checksum);

    // Decode public key
    let verifying_key = VerifyingKey::from_bytes(&SIGNING_PUBLIC_KEY)
        .map_err(|e| AppError::Update(format!("Invalid public key: {}", e)))?;

    // Decode signature from base64
    let signature_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        signature_b64,
    )
    .map_err(|e| AppError::Update(format!("Invalid signature encoding: {}", e)))?;

    // ed25519 signatures are exactly 64 bytes
    if signature_bytes.len() != 64 {
        return Err(AppError::Update(format!(
            "Invalid signature length: expected 64, got {}",
            signature_bytes.len()
        )));
    }

    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|e| AppError::Update(format!("Invalid signature format: {}", e)))?;

    // Verify
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_| AppError::Update("Signature verification failed - binary may be tampered".to_string()))
}

/// Fetch the signature file for a release asset
fn fetch_signature(release: &ReleaseInfo, asset_name: &str) -> Result<String, AppError> {
    let sig_asset_name = format!("{}.sig", asset_name);

    let sig_asset = release
        .assets
        .iter()
        .find(|a| a.name == sig_asset_name)
        .ok_or_else(|| {
            AppError::Update(format!(
                "No signature file found for {} (expected {})",
                asset_name, sig_asset_name
            ))
        })?;

    let response = minreq::get(&sig_asset.browser_download_url)
        .with_header("User-Agent", "FerrisPad")
        .with_timeout(10)
        .send()
        .map_err(|e| AppError::Update(format!("Failed to download signature: {}", e)))?;

    if response.status_code < 200 || response.status_code >= 300 {
        return Err(AppError::Update(format!(
            "Failed to download signature: HTTP {}",
            response.status_code
        )));
    }

    // Signature file contains just the base64 signature (trimmed)
    let signature = response
        .as_str()
        .map_err(|e| AppError::Update(format!("Invalid signature file: {}", e)))?
        .trim()
        .to_string();

    Ok(signature)
}

/// Download a binary from a URL to a specified path with progress (no verification)
///
/// This is kept for backwards compatibility. New code should use `download_and_verify`.
pub fn download_file<F>(url: &str, dest_path: &std::path::Path, mut progress_cb: F) -> Result<(), AppError>
where
    F: FnMut(f32),
{
    let response = minreq::get(url)
        .with_header("User-Agent", "FerrisPad")
        .with_timeout(60)
        .send()
        .map_err(|e| AppError::Update(format!("Failed to download update: {}", e)))?;

    if response.status_code < 200 || response.status_code >= 300 {
        return Err(AppError::Update(format!("Download failed with status: {}", response.status_code)));
    }

    // minreq loads the entire response into memory, so we write it all at once
    // For large files, this isn't ideal, but update binaries are typically small (~10MB)
    let bytes = response.as_bytes();
    let total_size = bytes.len();

    progress_cb(0.0);
    std::fs::write(dest_path, bytes)?;
    progress_cb(1.0);

    // Log size for debugging
    if total_size > 0 {
        eprintln!("[update] Downloaded {} bytes", total_size);
    }

    Ok(())
}

/// Download and verify a release binary
///
/// This function:
/// 1. Downloads the binary from the release assets
/// 2. Downloads the corresponding .sig file
/// 3. Verifies the signature matches the binary
/// 4. Only writes to disk if verification passes
///
/// Returns an error if verification fails or signature is missing.
pub fn download_and_verify<F>(
    release: &ReleaseInfo,
    dest_path: &std::path::Path,
    mut progress_cb: F,
) -> Result<(), AppError>
where
    F: FnMut(f32),
{
    let platform = get_platform_asset_name();
    let version = release.version();

    // Find the binary asset
    let binary_asset = release
        .assets
        .iter()
        .find(|a| a.name.contains(platform))
        .ok_or_else(|| {
            AppError::Update(format!("No release binary found for platform: {}", platform))
        })?;

    eprintln!("[update] Downloading {} ...", binary_asset.name);
    progress_cb(0.0);

    // Download binary
    let response = minreq::get(&binary_asset.browser_download_url)
        .with_header("User-Agent", "FerrisPad")
        .with_timeout(120) // Longer timeout for large binaries
        .send()
        .map_err(|e| AppError::Update(format!("Failed to download update: {}", e)))?;

    if response.status_code < 200 || response.status_code >= 300 {
        return Err(AppError::Update(format!(
            "Download failed with status: {}",
            response.status_code
        )));
    }

    let binary_data = response.as_bytes();
    eprintln!("[update] Downloaded {} bytes", binary_data.len());
    progress_cb(0.5);

    // Fetch and verify signature
    eprintln!("[update] Fetching signature...");
    let signature = fetch_signature(release, &binary_asset.name)?;

    eprintln!("[update] Verifying signature...");
    verify_binary_signature(binary_data, &version, platform, &signature)?;

    eprintln!("[update] Signature verified successfully");
    progress_cb(0.9);

    // Only write to disk after verification passes
    std::fs::write(dest_path, binary_data)?;
    progress_cb(1.0);

    Ok(())
}

/// Replace the current executable with a new one
pub fn install_update(new_binary_path: &std::path::Path) -> Result<(), AppError> {
    let current_exe = std::env::current_exe()?;

    // On Windows, we can't overwrite a running exe, but we can rename it.
    // On macOS/Linux, it's also safer to rename the old one first.
    let old_exe = current_exe.with_extension("old");

    // Clean up any previous .old file
    if old_exe.exists() {
        let _ = std::fs::remove_file(&old_exe);
    }

    // Rename current exe to .old
    std::fs::rename(&current_exe, &old_exe)?;

    // Move new exe to current location
    if let Err(e) = std::fs::rename(new_binary_path, &current_exe) {
        // Rollback on failure
        let _ = std::fs::rename(&old_exe, &current_exe);
        return Err(AppError::Io(e));
    }

    // On Unix systems, ensure the new binary is executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&current_exe)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&current_exe, perms)?;
    }

    Ok(())
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
        let one_hour_ago = current_timestamp() - (60 * 60);
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
