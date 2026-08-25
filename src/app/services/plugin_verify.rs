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

/// Result of the advisory Lua source lint.
///
/// This lint is **not** a security boundary and never rejects a plugin on its
/// own — see [`scan_lua_source`]. It only reports informational notes for the
/// user to weigh before installing an unverified plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaScanResult {
    /// No noteworthy patterns found.
    Clean,
    /// Advisory notes worth surfacing to the user before install.
    Warnings(Vec<String>),
}

/// Best-effort **advisory** lint over Lua source — *not* a security boundary.
///
/// # This is not the sandbox
///
/// Plugin isolation is enforced elsewhere and does not depend on this function:
///
/// - **Runtime primitive removal** (`plugins::runtime::setup_sandbox`) nils out
///   `os`, `io`, `debug`, `load`, `loadfile`, `dofile`, `require`, `package`,
///   and `coroutine`, and PUC Lua 5.4 has no `ffi`/`jit`. So the patterns this
///   lint reports are already unreachable at runtime — flagging them is a hint
///   about intent, never the thing that stops them.
/// - **Signature/checksum verification** ([`verify_plugin`]) is what actually
///   gates trust for registry plugins.
/// - **Per-plugin environment isolation** (T0009) is what will contain
///   cross-plugin `_ENV`/metatable tampering.
///
/// A text scanner cannot be made sound against a determined author (`_ENV`
/// reached through computed keys, `string.char` concatenation, staged
/// `getmetatable('')`, …). Treat every result as informational: the install
/// flow surfaces the notes to the user, and loading is never blocked by them.
///
/// Comments (`-- …` and `--[[ … ]]`, including `--[==[ … ]==]`) are stripped
/// before scanning, and string literals are preserved, so a payload hidden in a
/// comment is not mistaken for live code and a `--` inside a string does not
/// swallow the rest of the line.
///
/// # Returns
/// * [`LuaScanResult::Clean`] if nothing noteworthy was found.
/// * [`LuaScanResult::Warnings`] with one note per advisory pattern otherwise.
pub fn scan_lua_source(source: &str) -> LuaScanResult {
    let mut warnings: Vec<String> = Vec::new();

    // Strip comments once; scan the live code only.
    let code = strip_lua_comments(source);
    // Whitespace-compacted copy so structural checks survive line breaks and
    // spacing (e.g. `load\n(...)`, `setmetatable ( string`).
    let compact: String = code.chars().filter(|c| !c.is_ascii_whitespace()).collect();

    // Dynamic code loading (removed from the sandbox at runtime).
    if code.contains("loadstring") {
        warnings
            .push("Uses loadstring (dynamic code loading; unavailable in the sandbox)".to_string());
    }
    if contains_load_call(&compact) {
        warnings.push("Uses load() (dynamic code loading; unavailable in the sandbox)".to_string());
    }

    // FFI / JIT / debug (absent or removed in the sandbox).
    if compact.contains("ffi.cdef")
        || compact.contains("ffi.new")
        || compact.contains("ffi.load")
        || compact.contains("require(\"ffi\")")
        || compact.contains("require('ffi')")
    {
        warnings.push("References the FFI library (unavailable in the sandbox)".to_string());
    }
    if code.contains("jit.") {
        warnings.push("References LuaJIT (unavailable in the sandbox)".to_string());
    }
    if code.contains("debug.") {
        warnings.push("References the debug library (unavailable in the sandbox)".to_string());
    }

    // Global-environment access — whole-token `_G` / `_ENV` only, so identifiers
    // like `MY_GROUP` or `LOG_GREEN` no longer false-positive.
    if contains_identifier(&code, "_G") || contains_identifier(&code, "_ENV") {
        warnings.push("Accesses the global environment (_G/_ENV)".to_string());
    }

    // String-library metatable tampering. Targets the `string` library or the
    // metatable shared by all string values (`getmetatable("")`); a normal
    // `setmetatable(t, {__tostring = f})` no longer false-positives.
    if compact.contains("setmetatable(string,")
        || compact.contains("setmetatable(string)")
        || compact.contains("getmetatable(\"")
        || compact.contains("getmetatable('")
    {
        warnings.push("Tampers with the string metatable".to_string());
    }

    // Per-line heuristics.
    let mut warned_urls = false;
    let mut warned_long_lines = false;
    for line in code.lines() {
        if !warned_urls && (line.contains("http://") || line.contains("https://")) {
            warnings.push("Contains URLs (potential network intent)".to_string());
            warned_urls = true;
        }
        if !warned_long_lines && line.len() > 1000 {
            warnings.push("Contains very long lines (potential obfuscation)".to_string());
            warned_long_lines = true;
        }
        if warned_urls && warned_long_lines {
            break;
        }
    }

    if warnings.is_empty() {
        LuaScanResult::Clean
    } else {
        LuaScanResult::Warnings(warnings)
    }
}

/// True if `code` contains a `load` **call** — `load` as a whole identifier
/// followed (any whitespace already removed in the compacted input) by `(`.
/// Excludes `loaded`, `load_plugin`, `preload`, and `loadstring`.
fn contains_load_call(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut from = 0;
    while let Some(pos) = code[from..].find("load") {
        let abs = from + pos;
        let after = abs + 4;
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        // The char right after "load" must not extend the identifier (rules out
        // `loaded`, `loadstring`, `load_x`) and must be an opening paren.
        let after_ok = after < bytes.len() && bytes[after] == b'(';
        if before_ok && after_ok {
            return true;
        }
        from = abs + 1;
    }
    false
}

/// True if `haystack` contains `ident` as a whole identifier — not preceded or
/// followed by another identifier byte (`[A-Za-z0-9_]`).
fn contains_identifier(haystack: &str, ident: &str) -> bool {
    let hb = haystack.as_bytes();
    let n = ident.len();
    if n == 0 {
        return false;
    }
    let mut from = 0;
    while let Some(pos) = haystack[from..].find(ident) {
        let abs = from + pos;
        let before_ok = abs == 0 || !is_ident_byte(hb[abs - 1]);
        let after = abs + n;
        let after_ok = after >= hb.len() || !is_ident_byte(hb[after]);
        if before_ok && after_ok {
            return true;
        }
        from = abs + 1;
    }
    false
}

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Remove Lua comments from `source`, keeping string literals and newlines.
///
/// Line comments (`-- …`) and block comments (`--[[ … ]]`, including
/// `--[==[ … ]==]` levels) are replaced by spaces (newlines inside a block
/// comment are preserved so per-line heuristics stay aligned). Quoted and
/// long-bracket string literals are copied verbatim, so a `--` inside a string
/// is not treated as a comment.
///
/// Best-effort lexing for the advisory lint — not a full Lua parser.
fn strip_lua_comments(source: &str) -> String {
    let b = source.as_bytes();
    let n = b.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        let c = b[i];

        // Comment: `--` (line), or `--[[`/`--[=*[` (block).
        if c == b'-' && i + 1 < n && b[i + 1] == b'-' {
            let after = i + 2;
            if let Some(level) = open_long_bracket(b, after) {
                let end = close_long_bracket(b, after + level + 2, level);
                for &x in &b[i..end] {
                    out.push(if x == b'\n' { b'\n' } else { b' ' });
                }
                i = end;
            } else {
                let mut j = after;
                while j < n && b[j] != b'\n' {
                    j += 1;
                }
                out.resize(out.len() + (j - i), b' ');
                i = j;
            }
            continue;
        }

        // Quoted string literal.
        if c == b'"' || c == b'\'' {
            out.push(c);
            i += 1;
            while i < n {
                let d = b[i];
                out.push(d);
                i += 1;
                if d == b'\\' && i < n {
                    out.push(b[i]);
                    i += 1;
                } else if d == c {
                    break;
                }
            }
            continue;
        }

        // Long-bracket string literal `[[ … ]]` / `[=*[ … ]=*]`.
        if c == b'['
            && let Some(level) = open_long_bracket(b, i)
        {
            let end = close_long_bracket(b, i + level + 2, level);
            out.extend_from_slice(&b[i..end]);
            i = end;
            continue;
        }

        out.push(c);
        i += 1;
    }

    // Only ASCII delimiters are ever inspected/replaced and multibyte sequences
    // are copied whole, so the result is valid UTF-8; fall back defensively.
    String::from_utf8(out).unwrap_or_else(|_| source.to_string())
}

/// If `b[i..]` opens a long bracket (`[` `=`* `[`), return the number of `=`
/// (its "level"); otherwise `None`. `i` must point at the first `[`.
fn open_long_bracket(b: &[u8], i: usize) -> Option<usize> {
    if i >= b.len() || b[i] != b'[' {
        return None;
    }
    let mut j = i + 1;
    let mut level = 0;
    while j < b.len() && b[j] == b'=' {
        level += 1;
        j += 1;
    }
    if j < b.len() && b[j] == b'[' {
        Some(level)
    } else {
        None
    }
}

/// Byte index just past the closing `]` `=`{level} `]`, searching from `from`.
/// Returns `b.len()` if the bracket is never closed (unterminated).
fn close_long_bracket(b: &[u8], from: usize, level: usize) -> usize {
    let mut j = from;
    while j < b.len() {
        if b[j] == b']' {
            let mut k = j + 1;
            let mut eqs = 0;
            while k < b.len() && b[k] == b'=' {
                eqs += 1;
                k += 1;
            }
            if eqs == level && k < b.len() && b[k] == b']' {
                return k + 1;
            }
        }
        j += 1;
    }
    b.len()
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

    /// Helper: assert the advisory scan produced a note containing `needle`.
    fn assert_warns(source: &str, needle: &str) {
        match scan_lua_source(source) {
            LuaScanResult::Warnings(msgs) => assert!(
                msgs.iter().any(|m| m.contains(needle)),
                "expected a warning containing {needle:?}, got {msgs:?}"
            ),
            LuaScanResult::Clean => panic!("expected Warnings containing {needle:?}, got Clean"),
        }
    }

    #[test]
    fn test_scan_lua_warns_loadstring() {
        assert_warns("local f = loadstring('print(1)')", "loadstring");
    }

    #[test]
    fn test_scan_lua_warns_debug() {
        assert_warns("debug.getinfo(1)", "debug");
    }

    #[test]
    fn test_scan_lua_warns_ffi() {
        assert_warns("local ffi = require('ffi')", "FFI");
    }

    #[test]
    fn test_scan_lua_warns_rawset_global() {
        assert_warns("rawset(_G, 'evil', true)", "_G");
    }

    #[test]
    fn test_scan_lua_warns_direct_g_access() {
        assert_warns("_G.my_var = 42", "_G");
    }

    #[test]
    fn test_scan_lua_warns_g_read() {
        assert_warns("local x = _G.some_value", "_G");
    }

    #[test]
    fn test_scan_lua_g_in_comment_ok() {
        let source = "-- _G.foo = bar\nlocal x = 1";
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    #[test]
    fn test_scan_lua_warns_string_metatable() {
        assert_warns("setmetatable(string, {})", "metatable");
    }

    #[test]
    fn test_scan_lua_warning_urls() {
        assert_warns(r#"local url = "https://example.com/api""#, "URL");
    }

    #[test]
    fn test_scan_lua_warning_long_lines() {
        let long_line = "x".repeat(1001);
        let source = format!("local s = '{}'", long_line);
        assert_warns(&source, "long lines");
    }

    #[test]
    fn test_scan_lua_comment_lines_ignored() {
        let source = "-- debug.getinfo is just a comment\nlocal x = 1";
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    #[test]
    fn test_scan_lua_warns_load_call() {
        assert_warns("local f = load('return 1')", "load");
    }

    #[test]
    fn test_scan_lua_load_in_identifier_ok() {
        // "loaded" / "load_plugin" / "preload" must NOT trigger the load( check.
        let source =
            "local loaded = true\nlocal load_plugin = require('plugin')\nreturn preload(x)";
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    // --- T0006 regression: bypasses the old line-oriented gate missed ---

    // `_ENV` is equivalent to `_G` in Lua 5.4 and was not detected at all.
    #[test]
    fn test_scan_lua_env_write_is_flagged() {
        assert_warns("_ENV.os = require", "_ENV");
    }

    // String-metatable poisoning via `getmetatable('')` (not `setmetatable`).
    #[test]
    fn test_scan_lua_getmetatable_string_poison_is_flagged() {
        assert_warns("getmetatable('').__index = function() end", "metatable");
    }

    // `load` split from its `(` by a newline defeated the per-line scan.
    #[test]
    fn test_scan_lua_load_split_across_newline_is_flagged() {
        assert_warns("local f = load\n('return 1')", "load");
    }

    // A payload hidden in a block comment must NOT be scanned as live code.
    #[test]
    fn test_scan_lua_payload_in_block_comment_is_ignored() {
        let source = "--[[ load('x'); _G.evil = 1; debug.traceback() ]]\nlocal x = 1";
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    // Leveled block comment `--[==[ ... ]==]` is stripped too.
    #[test]
    fn test_scan_lua_leveled_block_comment_is_ignored() {
        let source = "--[==[ _G.x = 1 ]==]\nreturn 1";
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    // --- T0006 regression: false positives that blocked legit plugins ---

    // `MY_GROUP` / `LOG_GREEN` contain "_G" as a substring but are not `_G`.
    #[test]
    fn test_scan_lua_identifier_with_g_is_clean() {
        let source = "local MY_GROUP = 1\nlocal LOG_GREEN = 2\nlocal x = MY_GROUP + LOG_GREEN";
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    // A normal `__tostring` metamethod is not string-metatable tampering.
    #[test]
    fn test_scan_lua_tostring_metamethod_is_clean() {
        let source = "setmetatable(t, {__tostring = function() return 'x' end})";
        assert_eq!(scan_lua_source(source), LuaScanResult::Clean);
    }

    // A `--` inside a string literal must not be treated as a comment: the URL
    // survives stripping and is still flagged.
    #[test]
    fn test_scan_lua_dashes_inside_string_are_not_a_comment() {
        assert_warns(r#"local u = "https://ex--ample.com/x""#, "URL");
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
