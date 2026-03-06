use std::fs;
use tempfile::tempdir;

use ferris_pad::app::plugins::loader::{discover_plugins, load_plugin_toml};
use ferris_pad::app::plugins::security::{
    find_project_root, validate_command_arg, validate_path, PathValidation,
};

mod common;
use common::create_plugin_dir;

#[test]
fn test_load_metadata_then_validate_commands() {
    let dir = tempdir().unwrap();
    let toml = r#"
name = "test-linter"
version = "1.0.0"
description = "A test linter"

[permissions]
execute = ["ruff", "mypy", "black"]
"#;
    create_plugin_dir(dir.path(), "test-linter", "return {}", toml);

    let metadata = load_plugin_toml(&dir.path().join("test-linter")).unwrap();
    assert_eq!(metadata.permissions.execute.len(), 3);

    // Each declared command should pass validation
    for cmd in &metadata.permissions.execute {
        assert!(
            validate_command_arg(cmd).is_ok(),
            "Command '{}' should be valid",
            cmd
        );
    }

    // Injection attempts should fail
    assert!(validate_command_arg("ruff; rm -rf /").is_err());
    assert!(validate_command_arg("ruff && cat /etc/passwd").is_err());
}

#[test]
fn test_validate_path_within_discovered_root() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create a project with a .git marker
    fs::create_dir(root.join(".git")).unwrap();
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let file = src_dir.join("main.rs");
    fs::write(&file, "fn main() {}").unwrap();

    // find_project_root should find our root
    let found_root = find_project_root(&file).unwrap();
    assert_eq!(found_root, root);

    // Validating a file within the project root should succeed
    match validate_path("src/main.rs", &found_root) {
        PathValidation::Valid(p) => {
            assert!(p.ends_with("main.rs"));
        }
        other => panic!("Expected Valid, got {:?}", other),
    }
}

#[test]
fn test_path_traversal_blocked_from_plugin() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create project structure
    fs::create_dir(root.join(".git")).unwrap();
    let plugin_dir = root.join("plugins").join("evil-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    // Create a file outside project root
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "secret data").unwrap();

    // Attempt traversal relative to plugin dir
    let traversal = format!("../../../../../../{}/secret.txt", outside.path().display());
    match validate_path(&traversal, root) {
        PathValidation::OutsideProjectRoot => {} // expected
        PathValidation::NotFound => {}           // also acceptable
        other => panic!("Expected OutsideProjectRoot or NotFound, got {:?}", other),
    }

    // Attempt absolute path outside project
    let abs_path = format!("{}/secret.txt", outside.path().display());
    match validate_path(&abs_path, root) {
        PathValidation::OutsideProjectRoot => {}
        PathValidation::NotFound => {}
        other => panic!("Expected OutsideProjectRoot or NotFound, got {:?}", other),
    }
}

#[test]
fn test_full_discovery_and_validation() {
    let dir = tempdir().unwrap();

    // Create multiple plugins with various permissions
    create_plugin_dir(
        dir.path(),
        "alpha-plugin",
        "return { name = 'alpha' }",
        r#"
name = "alpha-plugin"
version = "1.0.0"

[permissions]
execute = ["ruff", "--output-format=json"]
"#,
    );

    create_plugin_dir(
        dir.path(),
        "beta-plugin",
        "return { name = 'beta' }",
        r#"
name = "beta-plugin"
version = "2.0.0"

[permissions]
execute = ["mypy"]
"#,
    );

    create_plugin_dir(
        dir.path(),
        "gamma-plugin",
        "return { name = 'gamma' }",
        &common::default_plugin_toml("gamma-plugin", "0.1.0"),
    );

    // Discover plugins
    let plugins = discover_plugins(dir.path());
    assert_eq!(plugins.len(), 3);

    // Load metadata and validate all declared commands
    for plugin_path in &plugins {
        if let Some(meta) = load_plugin_toml(plugin_path) {
            for cmd in &meta.permissions.execute {
                assert!(
                    validate_command_arg(cmd).is_ok(),
                    "Plugin {} command '{}' should be valid",
                    meta.name,
                    cmd
                );
            }
        }
    }
}
