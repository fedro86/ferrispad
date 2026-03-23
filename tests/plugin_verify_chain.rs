use std::fs;
use tempfile::tempdir;

use ferris_pad::app::infrastructure::error::AppError;
use ferris_pad::app::services::plugin_verify::{
    VerificationStatus, compute_checksum, verify_checksum, verify_plugin, verify_signature,
};

#[test]
fn test_checksum_compute_then_verify() {
    let content = b"print('hello from plugin')";

    // Compute checksum
    let checksum = compute_checksum(content);
    assert!(checksum.starts_with("sha256:"));

    // Verify same content passes
    assert!(verify_checksum(content, &checksum, "init.lua").is_ok());

    // Verify different content fails
    let modified = b"print('MODIFIED plugin')";
    let result = verify_checksum(modified, &checksum, "init.lua");
    assert!(matches!(result, Err(AppError::ChecksumMismatch(_, _, _))));
}

#[test]
fn test_verify_plugin_with_correct_checksums() {
    let dir = tempdir().unwrap();

    let init_lua = b"return { name = 'test', version = '1.0.0' }";
    let plugin_toml = b"name = \"test\"\nversion = \"1.0.0\"";

    fs::write(dir.path().join("init.lua"), init_lua).unwrap();
    fs::write(dir.path().join("plugin.toml"), plugin_toml).unwrap();

    let init_checksum = compute_checksum(init_lua);
    let toml_checksum = compute_checksum(plugin_toml);

    // Read back from disk (simulates real flow)
    let init_content = fs::read(dir.path().join("init.lua")).unwrap();
    let toml_content = fs::read(dir.path().join("plugin.toml")).unwrap();

    let status = verify_plugin(
        "test/",
        "1.0.0",
        &init_content,
        &toml_content,
        Some(&init_checksum),
        Some(&toml_checksum),
        None, // no signature
    )
    .unwrap();

    // Checksums match but no signature -> Unverified
    assert_eq!(status, VerificationStatus::Unverified);
}

#[test]
fn test_verify_plugin_tampered_file() {
    let init_lua = b"return { name = 'test', version = '1.0.0' }";
    let plugin_toml = b"name = \"test\"\nversion = \"1.0.0\"";

    let init_checksum = compute_checksum(init_lua);
    let toml_checksum = compute_checksum(plugin_toml);

    // Tamper with init.lua content
    let tampered_init = b"return { name = 'EVIL', version = '1.0.0' }";

    let result = verify_plugin(
        "test/",
        "1.0.0",
        tampered_init,
        plugin_toml,
        Some(&init_checksum),
        Some(&toml_checksum),
        None,
    );

    assert!(
        matches!(result, Err(AppError::ChecksumMismatch(ref f, _, _)) if f == "init.lua"),
        "Expected ChecksumMismatch for init.lua, got {:?}",
        result
    );
}

#[test]
fn test_verify_with_invalid_signature() {
    let init_lua = b"return {}";
    let plugin_toml = b"name = \"test\"\nversion = \"1.0.0\"";

    let init_checksum = compute_checksum(init_lua);
    let toml_checksum = compute_checksum(plugin_toml);

    // Garbage signature (valid base64, wrong length for ed25519)
    let status = verify_signature(
        "test/",
        "1.0.0",
        &init_checksum,
        &toml_checksum,
        "dGhpcyBpcyBub3QgYSB2YWxpZCBzaWduYXR1cmU=", // "this is not a valid signature"
    );

    assert!(
        matches!(status, VerificationStatus::Invalid(_)),
        "Expected Invalid, got {:?}",
        status
    );
}
