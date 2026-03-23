//! Plugin signature and checksum verification.
//!
//! This module provides cryptographic verification for plugins:
//! - SHA-256 checksums to verify file integrity
//! - ed25519 digital signatures to verify plugin authenticity
//! - Static Lua source analysis to detect suspicious patterns

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::app::infrastructure::error::AppError;

/// FerrisPad's official plugin signing public key (embedded at compile time).
/// This key is used to verify signatures on plugins from the official registry.
///
/// Generated with: plugin-signer keygen (ferrispad-plugins/tools/signer)
/// The corresponding private key is kept offline for signing plugins.
const PLUGIN_PUBLIC_KEY: [u8; 32] = [
    0x7f, 0x14, 0x24, 0xc5, 0x14, 0x5d, 0x99, 0xd7, 0xf0, 0xfd, 0x7c, 0x12, 0xdd, 0x5c, 0x3d, 0x8b,
    0x8f, 0x6d, 0x6b, 0x4e, 0xe7, 0xba, 0xb1, 0x2c, 0xd0, 0xdb, 0xdf, 0xbc, 0x17, 0x54, 0xff, 0x56,
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
    format!(
        "{}:{}:{}:{}",
        path, version, init_lua_checksum, plugin_toml_checksum
    )
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
    let signature_bytes =
        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature_b64) {
            Ok(b) => b,
            Err(e) => {
                return VerificationStatus::Invalid(format!("Invalid signature encoding: {}", e));
            }
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
    Ok(verify_signature(
        path,
        version,
        init_checksum,
        toml_checksum,
        sig,
    ))
}

/// Result of static Lua source analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaScanResult {
    /// Source code is clean — no suspicious patterns found
    Clean,
    /// Source has warning patterns but is allowed to install
    Warnings(Vec<String>),
    /// Source has blocked patterns — installation should be rejected
    Blocked(Vec<String>),
}

/// Perform static analysis on Lua source code to detect suspicious patterns.
///
/// Checks for blocked patterns (dynamic code execution, FFI access, debug library,
/// global table manipulation, string metatable poisoning) and warning patterns
/// (URLs, very long lines that may indicate obfuscation).
///
/// Comment lines (starting with `--`) are skipped for blocked pattern detection
/// to reduce false positives.
///
/// # Arguments
/// * `source` - The Lua source code to scan
///
/// # Returns
/// * `LuaScanResult::Clean` if no suspicious patterns found
/// * `LuaScanResult::Warnings(msgs)` if only warning-level patterns found
/// * `LuaScanResult::Blocked(msgs)` if any blocked patterns found
pub fn scan_lua_source(source: &str) -> LuaScanResult {
    let mut blocked: Vec<String> = Vec::new();
    let mut warned: Vec<String> = Vec::new();
    let mut warned_urls = false;
    let mut warned_long_lines = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Warning checks apply to all lines (including comments)
        if !warned_urls && (trimmed.contains("http://") || trimmed.contains("https://")) {
            warned.push("Source contains URLs (potential network intent)".to_string());
            warned_urls = true;
        }

        if !warned_long_lines && line.len() > 1000 {
            warned.push("Contains very long lines (potential obfuscation)".to_string());
            warned_long_lines = true;
        }

        // Skip comment lines for blocked pattern detection
        if trimmed.starts_with("--") {
            continue;
        }

        // Blocked: loadstring
        if trimmed.contains("loadstring") && !blocked.iter().any(|m| m.contains("loadstring")) {
            blocked.push("Dynamic code execution attempt (loadstring)".to_string());
        }

        // Blocked: load( but not load_plugin, loaded, etc.
        // Look for "load(" preceded by a non-alphanumeric/non-underscore char or at start of line
        if !blocked
            .iter()
            .any(|m| m == "Dynamic code execution attempt (load)")
            && contains_load_call(trimmed)
        {
            blocked.push("Dynamic code execution attempt (load)".to_string());
        }

        // Blocked: FFI access — ffi.cdef, ffi.new, ffi.load, or require.*ffi
        if !blocked.iter().any(|m| m.contains("FFI"))
            && (trimmed.contains("ffi.cdef")
                || trimmed.contains("ffi.new")
                || trimmed.contains("ffi.load")
                || (trimmed.contains("require") && trimmed.contains("ffi")))
        {
            blocked.push("FFI access attempt".to_string());
        }

        // Blocked: jit.
        if trimmed.contains("jit.") && !blocked.iter().any(|m| m.contains("LuaJIT")) {
            blocked.push("LuaJIT access attempt".to_string());
        }

        // Blocked: rawset + _G on the same line
        if trimmed.contains("rawset")
            && trimmed.contains("_G")
            && !blocked.iter().any(|m| m.contains("rawset"))
        {
            blocked.push("Global table manipulation (rawset _G)".to_string());
        }

        // Blocked: rawget + _G on the same line
        if trimmed.contains("rawget")
            && trimmed.contains("_G")
            && !blocked.iter().any(|m| m.contains("rawget"))
        {
            blocked.push("Global table manipulation (rawget _G)".to_string());
        }

        // Blocked: setmetatable + string on the same line
        if trimmed.contains("setmetatable")
            && trimmed.contains("string")
            && !blocked.iter().any(|m| m.contains("metatable"))
        {
            blocked.push("String metatable poisoning attempt".to_string());
        }

        // Blocked: debug.
        if trimmed.contains("debug.") && !blocked.iter().any(|m| m.contains("debug")) {
            blocked.push("debug library access attempt".to_string());
        }
    }

    if !blocked.is_empty() {
        LuaScanResult::Blocked(blocked)
    } else if !warned.is_empty() {
        LuaScanResult::Warnings(warned)
    } else {
        LuaScanResult::Clean
    }
}

/// Check if a line contains a `load(` call that is not part of a longer identifier
/// like `loaded`, `load_plugin`, `loadstring`, etc.
fn contains_load_call(line: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = line[search_from..].find("load(") {
        let abs_pos = search_from + pos;
        // Check character before "load(" — must not be alphanumeric or underscore
        if abs_pos == 0 {
            return true;
        }
        let prev_char = line.as_bytes()[abs_pos - 1];
        if !prev_char.is_ascii_alphanumeric() && prev_char != b'_' {
            return true;
        }
        search_from = abs_pos + 5; // skip past "load("
    }
    false
}

/// Check whether a plugin's init.lua registers an `on_text_changed` hook.
///
/// This is a simple string-based heuristic used to flag plugins that respond
/// to every keystroke, which may have performance implications.
///
/// # Arguments
/// * `init_lua` - The content of the plugin's init.lua file
///
/// # Returns
/// `true` if the source contains `on_text_changed`
pub fn detects_text_change_hook(init_lua: &str) -> bool {
    init_lua.contains("on_text_changed")
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
        let wrong_checksum =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let result = verify_checksum(data, wrong_checksum, "test.txt");
        assert!(matches!(result, Err(AppError::ChecksumMismatch(_, _, _))));
    }

    #[test]
    fn test_build_signed_message() {
        let msg = build_signed_message("python-lint/", "2.1.0", "sha256:abc123", "sha256:def456");
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

    // --- Static Lua analysis tests ---

    #[test]
    fn test_scan_lua_clean() {
        let source = r#"
local M = {}
M.name = "test"
function M.on_document_open(doc)
    return { status_message = "opened" }
end
return M
"#;
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    #[test]
    fn test_scan_lua_blocked_loadstring() {
        let source = "local f = loadstring('print(1)')";
        match scan_lua_source(source) {
            LuaScanResult::Blocked(msgs) => assert!(msgs.iter().any(|m| m.contains("loadstring"))),
            other => panic!("Expected Blocked, got {:?}", other),
        }
    }

    #[test]
    fn test_scan_lua_blocked_debug() {
        let source = "debug.getinfo(1)";
        match scan_lua_source(source) {
            LuaScanResult::Blocked(msgs) => assert!(msgs.iter().any(|m| m.contains("debug"))),
            other => panic!("Expected Blocked, got {:?}", other),
        }
    }

    #[test]
    fn test_scan_lua_blocked_ffi() {
        let source = "local ffi = require('ffi')";
        match scan_lua_source(source) {
            LuaScanResult::Blocked(msgs) => assert!(msgs.iter().any(|m| m.contains("FFI"))),
            other => panic!("Expected Blocked, got {:?}", other),
        }
    }

    #[test]
    fn test_scan_lua_blocked_rawset_global() {
        let source = "rawset(_G, 'evil', true)";
        match scan_lua_source(source) {
            LuaScanResult::Blocked(msgs) => assert!(msgs.iter().any(|m| m.contains("rawset"))),
            other => panic!("Expected Blocked, got {:?}", other),
        }
    }

    #[test]
    fn test_scan_lua_blocked_string_metatable() {
        let source = "setmetatable(string, {})";
        match scan_lua_source(source) {
            LuaScanResult::Blocked(msgs) => assert!(msgs.iter().any(|m| m.contains("metatable"))),
            other => panic!("Expected Blocked, got {:?}", other),
        }
    }

    #[test]
    fn test_scan_lua_warning_urls() {
        let source = r#"local url = "https://example.com/api""#;
        match scan_lua_source(source) {
            LuaScanResult::Warnings(msgs) => assert!(msgs.iter().any(|m| m.contains("URL"))),
            other => panic!("Expected Warnings, got {:?}", other),
        }
    }

    #[test]
    fn test_scan_lua_warning_long_lines() {
        let long_line = "x".repeat(1001);
        let source = format!("local s = '{}'", long_line);
        match scan_lua_source(&source) {
            LuaScanResult::Warnings(msgs) => assert!(msgs.iter().any(|m| m.contains("long lines"))),
            other => panic!("Expected Warnings, got {:?}", other),
        }
    }

    #[test]
    fn test_scan_lua_comment_lines_ignored() {
        let source = "-- debug.getinfo is just a comment\nlocal x = 1";
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    #[test]
    fn test_scan_lua_blocked_load_call() {
        let source = "local f = load('return 1')";
        match scan_lua_source(source) {
            LuaScanResult::Blocked(msgs) => assert!(msgs.iter().any(|m| m.contains("load"))),
            other => panic!("Expected Blocked, got {:?}", other),
        }
    }

    #[test]
    fn test_scan_lua_load_in_identifier_ok() {
        // "loaded" or "load_plugin" should NOT trigger the load( check
        let source = "local loaded = true\nlocal load_plugin = require('plugin')";
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    #[test]
    fn test_detects_text_change_hook_true() {
        let source = r#"function M.on_text_changed(doc) end"#;
        assert!(detects_text_change_hook(source));
    }

    #[test]
    fn test_detects_text_change_hook_false() {
        let source = r#"function M.on_document_open(doc) end"#;
        assert!(!detects_text_change_hook(source));
    }
}
