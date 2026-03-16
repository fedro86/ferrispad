use std::fs;
use tempfile::tempdir;

use ferris_pad::app::plugins::loader::{discover_plugins, load_plugin_toml};
use ferris_pad::app::services::plugin_verify::{compute_checksum, verify_checksum};

mod common;
use common::{create_plugin_dir, default_plugin_toml};

#[test]
fn test_discover_then_load_then_verify() {
    let dir = tempdir().unwrap();

    let init_lua = "return { name = 'verifiable', version = '1.0.0' }";
    let plugin_toml = &default_plugin_toml("verifiable", "1.0.0");
    create_plugin_dir(dir.path(), "verifiable", init_lua, plugin_toml);

    // Step 1: Discover
    let plugins = discover_plugins(dir.path());
    assert_eq!(plugins.len(), 1);

    // Step 2: Load metadata
    let metadata = load_plugin_toml(&plugins[0]).unwrap();
    assert_eq!(metadata.name, "verifiable");
    assert_eq!(metadata.version, "1.0.0");

    // Step 3: Verify checksums
    let init_content = fs::read(plugins[0].join("init.lua")).unwrap();
    let toml_content = fs::read(plugins[0].join("plugin.toml")).unwrap();
    let init_checksum = compute_checksum(&init_content);
    let toml_checksum = compute_checksum(&toml_content);

    // Self-verification should pass
    assert!(verify_checksum(&init_content, &init_checksum, "init.lua").is_ok());
    assert!(verify_checksum(&toml_content, &toml_checksum, "plugin.toml").is_ok());
}

#[test]
fn test_discover_ignores_dirs_without_init_lua() {
    let dir = tempdir().unwrap();

    // Plugin with init.lua -> should be discovered
    create_plugin_dir(dir.path(), "good-plugin", "return {}", &default_plugin_toml("good", "1.0.0"));

    // Directory with only plugin.toml (no init.lua) -> should NOT be discovered
    let bad_dir = dir.path().join("incomplete-plugin");
    fs::create_dir(&bad_dir).unwrap();
    fs::write(bad_dir.join("plugin.toml"), "name = \"incomplete\"").unwrap();

    // Empty directory -> should NOT be discovered
    fs::create_dir(dir.path().join("empty-dir")).unwrap();

    let plugins = discover_plugins(dir.path());
    assert_eq!(plugins.len(), 1);
    assert!(plugins[0].ends_with("good-plugin"));
}

#[test]
fn test_load_metadata_with_config_params() {
    let dir = tempdir().unwrap();

    let toml = r#"
name = "configurable"
version = "1.0.0"

[[config.params]]
key = "max_line_length"
label = "Max Line Length"
type = "number"
default = "120"
placeholder = "e.g. 80, 120"

[[config.params]]
key = "ignore_rules"
label = "Ignore Rules"
type = "string"
default = "E501,W503"
validate = "cli_args"

[[config.params]]
key = "format_on_save"
label = "Format on Save"
type = "boolean"
default = "false"
"#;

    create_plugin_dir(dir.path(), "configurable", "return {}", toml);

    let metadata = load_plugin_toml(&dir.path().join("configurable")).unwrap();
    assert_eq!(metadata.config.params.len(), 3);

    let p0 = &metadata.config.params[0];
    assert_eq!(p0.key, "max_line_length");
    assert_eq!(p0.label, "Max Line Length");
    assert_eq!(p0.param_type, "number");
    assert_eq!(p0.default, "120");
    assert_eq!(p0.placeholder.as_deref(), Some("e.g. 80, 120"));

    let p1 = &metadata.config.params[1];
    assert_eq!(p1.key, "ignore_rules");
    assert_eq!(p1.validate.as_deref(), Some("cli_args"));

    let p2 = &metadata.config.params[2];
    assert_eq!(p2.param_type, "boolean");
    assert_eq!(p2.default, "false");
}

#[test]
fn test_discover_multiple_plugins_sorted() {
    let dir = tempdir().unwrap();

    create_plugin_dir(dir.path(), "charlie", "return {}", &default_plugin_toml("charlie", "1.0.0"));
    create_plugin_dir(dir.path(), "alpha", "return {}", &default_plugin_toml("alpha", "1.0.0"));
    create_plugin_dir(dir.path(), "bravo", "return {}", &default_plugin_toml("bravo", "1.0.0"));

    let plugins = discover_plugins(dir.path());
    assert_eq!(plugins.len(), 3);

    // Should be sorted alphabetically
    let names: Vec<&str> = plugins
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, vec!["alpha", "bravo", "charlie"]);
}
