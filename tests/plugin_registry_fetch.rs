use ferris_pad::app::services::plugin_registry::{
    fetch_community_registry_cached, fetch_plugin_registry_cached,
};

#[test]
#[ignore] // requires network access — run with: cargo test --test plugin_registry_fetch -- --ignored
fn test_fetch_official_registry() {
    let result = fetch_plugin_registry_cached();
    assert!(
        result.is_ok(),
        "Failed to fetch official registry: {:?}",
        result.err()
    );
    let registry = result.unwrap();
    assert!(
        !registry.plugins.is_empty(),
        "Official registry returned 0 plugins"
    );
    for plugin in &registry.plugins {
        assert!(!plugin.name.is_empty(), "Plugin has empty name");
        assert!(!plugin.version.is_empty(), "Plugin has empty version");
        assert!(!plugin.path.is_empty(), "Plugin has empty path");
    }
    eprintln!(
        "[test] Official registry: {} plugins fetched successfully",
        registry.plugins.len()
    );
}

#[test]
#[ignore] // requires network access
fn test_fetch_community_registry() {
    let result = fetch_community_registry_cached();
    assert!(
        result.is_ok(),
        "Failed to fetch community registry: {:?}",
        result.err()
    );
    let registry = result.unwrap();
    assert!(
        !registry.plugins.is_empty(),
        "Community registry returned 0 plugins"
    );
    for plugin in &registry.plugins {
        assert!(!plugin.name.is_empty(), "Plugin has empty name");
        assert!(!plugin.repo.is_empty(), "Plugin has empty repo URL");
    }
    eprintln!(
        "[test] Community registry: {} plugins fetched successfully",
        registry.plugins.len()
    );
}
