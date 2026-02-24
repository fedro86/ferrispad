//! Plugin signature and checksum verification.
//!
//! This module provides cryptographic verification for plugins:
//! - SHA-256 checksums to verify file integrity
//! - ed25519 digital signatures to verify plugin authenticity

use ed25519_dalek::{Signature, VerifyingKey, Verifier};
use sha2::{Sha256, Digest};

use crate::app::infrastructure::error::AppError;

/// FerrisPad's official plugin signing public key (embedded at compile time).
/// This key is used to verify signatures on plugins from the official registry.
///
/// Generated with: SigningKey::generate(&mut OsRng).verifying_key().to_bytes()
/// The corresponding private key is kept offline for signing plugins.
const PLUGIN_PUBLIC_KEY: [u8; 32] = [
    // TODO: Replace with actual public key bytes after generation
    // For now, use a placeholder that will cause all signatures to fail
    // This ensures we don't accidentally verify unsigned plugins as signed
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Result of plugin verification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationStatus {
    /// Plugin is verified with valid signature and checksums
    Verified,
    /// Plugin has no signature (third-party or legacy)
    Unverified,
    /// Signature verification failed - do not install
    Invalid(String),
}

impl VerificationStatus {
    /// Returns true if the plugin should be allowed to install
    pub fn allows_install(&self) -> bool {
        !matches!(self, VerificationStatus::Invalid(_))
    }

    /// Returns a user-friendly display string
    pub fn display(&self) -> &'static str {
        match self {
            VerificationStatus::Verified => "Verified",
            VerificationStatus::Unverified => "Unverified",
            VerificationStatus::Invalid(_) => "Invalid",
        }
    }
}

/// Compute SHA-256 hash of data and return as hex string with "sha256:" prefix
pub fn compute_checksum(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

/// Verify that data matches expected checksum
///
/// # Arguments
/// * `data` - The raw bytes to verify
/// * `expected` - Expected checksum in format "sha256:hexstring"
/// * `file_name` - Name of the file (for error messages)
///
/// # Returns
/// * `Ok(())` if checksum matches
/// * `Err(AppError::ChecksumMismatch)` if mismatch
pub fn verify_checksum(data: &[u8], expected: &str, file_name: &str) -> Result<(), AppError> {
    let actual = compute_checksum(data);
    if actual == expected {
        Ok(())
    } else {
        Err(AppError::ChecksumMismatch(
            file_name.to_string(),
            expected.to_string(),
            actual,
        ))
    }
}

/// Build the canonical message that is signed
///
/// Format: "{path}:{version}:{init_lua_checksum}:{plugin_toml_checksum}"
///
/// This ensures:
/// - Version-specific signatures (can't replay old versions)
/// - File content tied to signature (any tampering invalidates)
pub fn build_signed_message(
    path: &str,
    version: &str,
    init_lua_checksum: &str,
    plugin_toml_checksum: &str,
) -> String {
    format!("{}:{}:{}:{}", path, version, init_lua_checksum, plugin_toml_checksum)
}

/// Verify plugin signature against embedded public key
///
/// # Arguments
/// * `path` - Plugin path (e.g., "python-lint/")
/// * `version` - Plugin version (e.g., "2.1.0")
/// * `init_lua_checksum` - Checksum of init.lua
/// * `plugin_toml_checksum` - Checksum of plugin.toml
/// * `signature_b64` - Base64-encoded ed25519 signature
///
/// # Returns
/// * `VerificationStatus::Verified` if signature is valid
/// * `VerificationStatus::Invalid(reason)` if verification fails
pub fn verify_signature(
    path: &str,
    version: &str,
    init_lua_checksum: &str,
    plugin_toml_checksum: &str,
    signature_b64: &str,
) -> VerificationStatus {
    // Decode public key
    let verifying_key = match VerifyingKey::from_bytes(&PLUGIN_PUBLIC_KEY) {
        Ok(k) => k,
        Err(e) => return VerificationStatus::Invalid(format!("Invalid public key: {}", e)),
    };

    // Decode signature from base64
    let signature_bytes = match base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        signature_b64,
    ) {
        Ok(b) => b,
        Err(e) => return VerificationStatus::Invalid(format!("Invalid signature encoding: {}", e)),
    };

    // ed25519 signatures are exactly 64 bytes
    if signature_bytes.len() != 64 {
        return VerificationStatus::Invalid(format!(
            "Invalid signature length: expected 64, got {}",
            signature_bytes.len()
        ));
    }

    let signature = match Signature::from_slice(&signature_bytes) {
        Ok(s) => s,
        Err(e) => return VerificationStatus::Invalid(format!("Invalid signature format: {}", e)),
    };

    // Build message and verify
    let message = build_signed_message(path, version, init_lua_checksum, plugin_toml_checksum);

    match verifying_key.verify(message.as_bytes(), &signature) {
        Ok(()) => VerificationStatus::Verified,
        Err(e) => VerificationStatus::Invalid(format!("Signature mismatch: {}", e)),
    }
}

/// Verify a plugin's checksums and signature
///
/// This is the main entry point for plugin verification. It:
/// 1. Verifies init.lua checksum (if provided)
/// 2. Verifies plugin.toml checksum (if provided)
/// 3. Verifies signature (if provided and checksums present)
///
/// # Arguments
/// * `path` - Plugin path (e.g., "python-lint/")
/// * `version` - Plugin version
/// * `init_lua_content` - Content of init.lua
/// * `plugin_toml_content` - Content of plugin.toml
/// * `expected_init_checksum` - Expected checksum of init.lua (optional)
/// * `expected_toml_checksum` - Expected checksum of plugin.toml (optional)
/// * `signature` - Base64-encoded signature (optional)
///
/// # Returns
/// * `Ok(VerificationStatus)` - Verification result
/// * `Err(AppError)` - If checksum verification fails
pub fn verify_plugin(
    path: &str,
    version: &str,
    init_lua_content: &[u8],
    plugin_toml_content: &[u8],
    expected_init_checksum: Option<&str>,
    expected_toml_checksum: Option<&str>,
    signature: Option<&str>,
) -> Result<VerificationStatus, AppError> {
    // If no checksums provided, plugin is unverified
    let (init_checksum, toml_checksum) = match (expected_init_checksum, expected_toml_checksum) {
        (Some(init), Some(toml)) => (init, toml),
        _ => return Ok(VerificationStatus::Unverified),
    };

    // Verify checksums
    verify_checksum(init_lua_content, init_checksum, "init.lua")?;
    verify_checksum(plugin_toml_content, toml_checksum, "plugin.toml")?;

    // If no signature provided, checksums pass but unverified
    let sig = match signature {
        Some(s) => s,
        None => return Ok(VerificationStatus::Unverified),
    };

    // Verify signature
    Ok(verify_signature(path, version, init_checksum, toml_checksum, sig))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_checksum() {
        let data = b"Hello, World!";
        let checksum = compute_checksum(data);
        assert!(checksum.starts_with("sha256:"));
        // SHA-256 produces 64 hex characters
        assert_eq!(checksum.len(), 7 + 64); // "sha256:" + 64 hex chars
    }

    #[test]
    fn test_compute_checksum_consistency() {
        let data = b"test data";
        let checksum1 = compute_checksum(data);
        let checksum2 = compute_checksum(data);
        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn test_verify_checksum_success() {
        let data = b"test content";
        let checksum = compute_checksum(data);
        assert!(verify_checksum(data, &checksum, "test.txt").is_ok());
    }

    #[test]
    fn test_verify_checksum_failure() {
        let data = b"test content";
        let wrong_checksum = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let result = verify_checksum(data, wrong_checksum, "test.txt");
        assert!(matches!(result, Err(AppError::ChecksumMismatch(_, _, _))));
    }

    #[test]
    fn test_build_signed_message() {
        let msg = build_signed_message(
            "python-lint/",
            "2.1.0",
            "sha256:abc123",
            "sha256:def456",
        );
        assert_eq!(msg, "python-lint/:2.1.0:sha256:abc123:sha256:def456");
    }

    #[test]
    fn test_verification_status_allows_install() {
        assert!(VerificationStatus::Verified.allows_install());
        assert!(VerificationStatus::Unverified.allows_install());
        assert!(!VerificationStatus::Invalid("test".to_string()).allows_install());
    }

    #[test]
    fn test_verify_plugin_no_checksums() {
        let result = verify_plugin(
            "test/",
            "1.0.0",
            b"init content",
            b"toml content",
            None,
            None,
            None,
        );
        assert!(matches!(result, Ok(VerificationStatus::Unverified)));
    }

    #[test]
    fn test_verify_plugin_checksum_mismatch() {
        let result = verify_plugin(
            "test/",
            "1.0.0",
            b"init content",
            b"toml content",
            Some("sha256:wrong"),
            Some("sha256:alsowrong"),
            None,
        );
        assert!(matches!(result, Err(AppError::ChecksumMismatch(_, _, _))));
    }

    #[test]
    fn test_verify_signature_invalid_base64() {
        let status = verify_signature(
            "test/",
            "1.0.0",
            "sha256:abc",
            "sha256:def",
            "not valid base64!!!",
        );
        assert!(matches!(status, VerificationStatus::Invalid(_)));
    }

    #[test]
    fn test_verify_signature_wrong_length() {
        // Valid base64 but wrong length for ed25519 signature
        let status = verify_signature(
            "test/",
            "1.0.0",
            "sha256:abc",
            "sha256:def",
            "dG9vIHNob3J0", // "too short" in base64
        );
        assert!(matches!(status, VerificationStatus::Invalid(_)));
    }
}
