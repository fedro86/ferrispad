//! Integration tests for plugin manager change handling.
//!
//! Tests the `apply_plugin_uninstalls` function that processes
//! the `PluginManagerResult::Changed` variant — specifically the
//! uninstall-then-reinstall scenario where name formats may differ.

use std::fs;
use tempfile::TempDir;

use ferris_pad::app::controllers::plugin::apply_plugin_uninstalls;

/// Helper: create a fake plugin directory with an init.lua file.
fn create_fake_plugin(plugins_dir: &std::path::Path, name: &str) {
    let dir = plugins_dir.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("init.lua"), "-- stub").unwrap();
    fs::write(dir.join("plugin.toml"), "[plugin]\nname = \"stub\"").unwrap();
}

#[test]
fn uninstall_only_deletes_directory() {
    let tmp = TempDir::new().unwrap();
    let plugins_dir = tmp.path();

    create_fake_plugin(plugins_dir, "python-lint");

    let (deleted, errors) = apply_plugin_uninstalls(
        plugins_dir,
        &[],                          // nothing installed
        &["python-lint".to_string()], // uninstall this
    );

    assert!(errors.is_empty());
    assert_eq!(deleted, vec!["python-lint"]);
    assert!(!plugins_dir.join("python-lint").exists());
}

#[test]
fn install_only_does_not_delete() {
    let tmp = TempDir::new().unwrap();
    let plugins_dir = tmp.path();

    create_fake_plugin(plugins_dir, "python-lint");

    let (deleted, errors) = apply_plugin_uninstalls(
        plugins_dir,
        &["python-lint".to_string()], // installed
        &[],                          // nothing uninstalled
    );

    assert!(errors.is_empty());
    assert!(deleted.is_empty());
    assert!(plugins_dir.join("python-lint").exists());
}

#[test]
fn reinstall_same_name_skips_deletion() {
    let tmp = TempDir::new().unwrap();
    let plugins_dir = tmp.path();

    create_fake_plugin(plugins_dir, "python-lint");

    // User uninstalled then reinstalled — same name format in both lists
    let (deleted, errors) = apply_plugin_uninstalls(
        plugins_dir,
        &["python-lint".to_string()], // reinstalled
        &["python-lint".to_string()], // was uninstalled
    );

    assert!(errors.is_empty());
    assert!(
        deleted.is_empty(),
        "Should skip deletion for reinstalled plugin"
    );
    assert!(plugins_dir.join("python-lint").exists());
}

#[test]
fn reinstall_display_vs_registry_name_skips_deletion() {
    let tmp = TempDir::new().unwrap();
    let plugins_dir = tmp.path();

    create_fake_plugin(plugins_dir, "python-lint");

    // The core bug: Installed tab uses display name "Python Lint",
    // Official tab uses registry name "python-lint".
    // Uninstall from Installed tab → reinstall from Official tab.
    let (deleted, errors) = apply_plugin_uninstalls(
        plugins_dir,
        &["python-lint".to_string()], // reinstalled (registry name)
        &["Python Lint".to_string()], // was uninstalled (display name)
    );

    assert!(errors.is_empty());
    assert!(
        deleted.is_empty(),
        "Should normalize names and skip deletion for reinstalled plugin"
    );
    assert!(plugins_dir.join("python-lint").exists());
}

#[test]
fn reinstall_registry_vs_display_name_skips_deletion() {
    let tmp = TempDir::new().unwrap();
    let plugins_dir = tmp.path();

    create_fake_plugin(plugins_dir, "python-lint");

    // Reverse scenario: uninstall via registry name, reinstall via display name
    let (deleted, errors) = apply_plugin_uninstalls(
        plugins_dir,
        &["Python Lint".to_string()], // reinstalled (display name)
        &["python-lint".to_string()], // was uninstalled (registry name)
    );

    assert!(errors.is_empty());
    assert!(
        deleted.is_empty(),
        "Should normalize names and skip deletion"
    );
    assert!(plugins_dir.join("python-lint").exists());
}

#[test]
fn mixed_operations() {
    let tmp = TempDir::new().unwrap();
    let plugins_dir = tmp.path();

    create_fake_plugin(plugins_dir, "plugin-a");
    create_fake_plugin(plugins_dir, "plugin-b");
    create_fake_plugin(plugins_dir, "plugin-c");

    // Uninstall A, reinstall B (with name mismatch), leave C alone
    let (deleted, errors) = apply_plugin_uninstalls(
        plugins_dir,
        &["plugin-b".to_string()],                         // reinstalled B
        &["plugin-a".to_string(), "Plugin B".to_string()], // uninstall A + B (display name)
    );

    assert!(errors.is_empty());
    assert_eq!(deleted, vec!["plugin-a"]);
    assert!(
        !plugins_dir.join("plugin-a").exists(),
        "A should be deleted"
    );
    assert!(
        plugins_dir.join("plugin-b").exists(),
        "B should be kept (reinstalled)"
    );
    assert!(
        plugins_dir.join("plugin-c").exists(),
        "C should be untouched"
    );
}

#[test]
fn uninstall_nonexistent_directory_is_silent() {
    let tmp = TempDir::new().unwrap();
    let plugins_dir = tmp.path();

    // No plugin directory exists — should not error
    let (deleted, errors) =
        apply_plugin_uninstalls(plugins_dir, &[], &["nonexistent-plugin".to_string()]);

    assert!(errors.is_empty());
    assert!(deleted.is_empty());
}

// ============ Row width / scrollbar tests ============

use ferris_pad::ui::dialogs::plugin_manager::row_width_for_scroll;

/// Scrollbar size used in the plugin manager dialog (matches SCROLLBAR_SIZE).
const SCROLLBAR_SIZE: i32 = 12;

#[test]
fn row_width_full_when_content_fits() {
    // 5 rows × 70px + 4 × 5px spacing = 370px, viewport = 400px → no scrollbar
    let content_h = 5 * 70 + 4 * 5;
    let viewport_h = 400;
    let full_width = 520;

    assert_eq!(
        row_width_for_scroll(content_h, viewport_h, full_width),
        full_width,
        "Rows should use full width when content fits in viewport"
    );
}

#[test]
fn row_width_shrinks_when_content_overflows() {
    // 6 rows × 70px + 5 × 5px spacing = 445px, viewport = 400px → scrollbar active
    let content_h = 6 * 70 + 5 * 5;
    let viewport_h = 400;
    let full_width = 520;

    assert_eq!(
        row_width_for_scroll(content_h, viewport_h, full_width),
        full_width - SCROLLBAR_SIZE,
        "Rows should shrink by SCROLLBAR_SIZE when content overflows viewport"
    );
}

#[test]
fn row_width_exact_boundary_no_scrollbar() {
    // Content exactly equals viewport → no scrollbar
    let content_h = 400;
    let viewport_h = 400;
    let full_width = 520;

    assert_eq!(
        row_width_for_scroll(content_h, viewport_h, full_width),
        full_width,
        "No scrollbar when content exactly equals viewport"
    );
}

#[test]
fn row_width_one_pixel_over_triggers_scrollbar() {
    // Content 1px taller than viewport → scrollbar
    let content_h = 401;
    let viewport_h = 400;
    let full_width = 520;

    assert_eq!(
        row_width_for_scroll(content_h, viewport_h, full_width),
        full_width - SCROLLBAR_SIZE,
        "Scrollbar should appear when content is even 1px taller than viewport"
    );
}

#[test]
fn row_width_expands_after_uninstall_removes_overflow() {
    let row_h = 70;
    let spacing = 5;
    let viewport_h = 400;
    let full_width = 520;

    // 6 visible rows: 6×70 + 5×5 = 445 > 400 → scrollbar → narrow
    let content_6 = 6 * row_h + 5 * spacing;
    assert_eq!(
        row_width_for_scroll(content_6, viewport_h, full_width),
        full_width - SCROLLBAR_SIZE,
        "6 rows should need scrollbar"
    );

    // After hiding one row: 5 visible rows: 5×70 + 4×5 = 370 < 400 → no scrollbar → full width
    let content_5 = 5 * row_h + 4 * spacing;
    assert_eq!(
        row_width_for_scroll(content_5, viewport_h, full_width),
        full_width,
        "After uninstalling one plugin, 5 rows should fit without scrollbar and rows should expand"
    );
}

#[test]
fn row_width_stays_narrow_if_still_overflows_after_uninstall() {
    let row_h = 70;
    let spacing = 5;
    let viewport_h = 400;
    let full_width = 520;

    // 7 visible rows: 7×70 + 6×5 = 520 > 400 → scrollbar
    let content_7 = 7 * row_h + 6 * spacing;
    assert_eq!(
        row_width_for_scroll(content_7, viewport_h, full_width),
        full_width - SCROLLBAR_SIZE,
    );

    // After hiding one: 6 visible rows: 6×70 + 5×5 = 445 > 400 → still scrollbar
    let content_6 = 6 * row_h + 5 * spacing;
    assert_eq!(
        row_width_for_scroll(content_6, viewport_h, full_width),
        full_width - SCROLLBAR_SIZE,
        "After uninstalling one of 7 plugins, 6 rows still overflow — should stay narrow"
    );
}
