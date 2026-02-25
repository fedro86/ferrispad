use std::collections::HashSet;

use fltk::{
    app::Sender,
    enums::{Font, Key, Shortcut},
    menu::{MenuBar, MenuFlag},
    prelude::*,
};

use crate::app::plugins::PluginManager;
use crate::app::{AppSettings, Message};

/// Reserved keyboard shortcuts that plugins cannot override.
/// These are built-in editor functions.
pub const RESERVED_SHORTCUTS: &[&str] = &[
    "ctrl+t",
    "ctrl+n",
    "ctrl+o",
    "ctrl+s",
    "ctrl+shift+s",
    "ctrl+w",
    "ctrl+tab",
    "ctrl+shift+tab",
    "ctrl+q",
    "ctrl+z",
    "ctrl+shift+z",
    "ctrl+x",
    "ctrl+c",
    "ctrl+v",
    "ctrl+a",
    "ctrl+f",
    "ctrl+h",
    "ctrl+g",
    "ctrl+m",
    "ctrl+shift+l", // Run All Checks
];

/// Parse a shortcut string like "Ctrl+Shift+P" into an FLTK Shortcut.
/// Returns None if the string is invalid or empty.
fn parse_shortcut(s: &str) -> Option<Shortcut> {
    if s.is_empty() {
        return None;
    }

    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let mut result = Shortcut::None;
    let mut has_key = false;

    for part in parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => result = result | Shortcut::Ctrl,
            "shift" => result = result | Shortcut::Shift,
            "alt" => result = result | Shortcut::Alt,
            _ => {
                // This should be the key
                if has_key {
                    // Multiple keys specified, invalid
                    return None;
                }
                has_key = true;

                // Handle function keys F1-F12
                if lower.starts_with('f') && lower.len() >= 2 {
                    if let Ok(num) = lower[1..].parse::<u32>() {
                        if (1..=12).contains(&num) {
                            let fkey = match num {
                                1 => Key::F1,
                                2 => Key::F2,
                                3 => Key::F3,
                                4 => Key::F4,
                                5 => Key::F5,
                                6 => Key::F6,
                                7 => Key::F7,
                                8 => Key::F8,
                                9 => Key::F9,
                                10 => Key::F10,
                                11 => Key::F11,
                                12 => Key::F12,
                                _ => unreachable!(),
                            };
                            result = result | fkey;
                            continue;
                        }
                    }
                }

                // Single character key (A-Z, 0-9)
                if part.len() == 1 {
                    let ch = part.chars().next().unwrap().to_ascii_lowercase();
                    if ch.is_ascii_alphanumeric() {
                        result = result | ch;
                        continue;
                    }
                }

                // Unknown key
                return None;
            }
        }
    }

    if !has_key {
        // No key specified, only modifiers
        return None;
    }

    Some(result)
}

/// Normalize a shortcut string for comparison (lowercase, sorted modifiers)
pub fn normalize_shortcut(s: &str) -> String {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let mut modifiers: Vec<String> = Vec::new();
    let mut key = String::new();

    for part in parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => modifiers.push("ctrl".to_string()),
            "shift" => modifiers.push("shift".to_string()),
            "alt" => modifiers.push("alt".to_string()),
            _ => key = lower,
        }
    }

    modifiers.sort();
    if !key.is_empty() {
        modifiers.push(key);
    }
    modifiers.join("+")
}

/// Check if a shortcut string is valid (can be parsed into an FLTK shortcut)
pub fn is_valid_shortcut(s: &str) -> bool {
    !s.is_empty() && parse_shortcut(s).is_some()
}

pub fn build_menu(
    menu: &mut MenuBar,
    sender: &Sender<Message>,
    settings: &AppSettings,
    initial_dark_mode: bool,
    tabs_enabled: bool,
) {
    let s = sender;

    // File
    let new_shortcut = if tabs_enabled { Shortcut::Ctrl | 't' } else { Shortcut::Ctrl | 'n' };
    menu.add("File/New", new_shortcut, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::FileNew) });
    menu.add("File/Open...", Shortcut::Ctrl | 'o', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::FileOpen) });
    menu.add("File/Save", Shortcut::Ctrl | 's', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::FileSave) });
    menu.add("File/Save As...", Shortcut::Ctrl | Shortcut::Shift | 's', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::FileSaveAs) });
    if tabs_enabled {
        menu.add("File/Close Tab", Shortcut::Ctrl | 'w', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::TabCloseActive) });
        menu.add("File/Next Tab", Shortcut::Ctrl | Key::Tab, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::TabNext) });
        menu.add("File/Previous Tab", Shortcut::Ctrl | Shortcut::Shift | Key::Tab, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::TabPrevious) });
    }
    menu.add("File/Settings...", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::OpenSettings) });
    menu.add("File/Quit", Shortcut::Ctrl | 'q', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::FileQuit) });

    // Edit
    menu.add("Edit/Undo", Shortcut::Ctrl | 'z', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::EditUndo) });
    menu.add("Edit/Redo", Shortcut::Ctrl | Shortcut::Shift | 'z', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::EditRedo) });
    menu.add("Edit/Cut", Shortcut::Ctrl | 'x', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::EditCut) });
    menu.add("Edit/Copy", Shortcut::Ctrl | 'c', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::EditCopy) });
    menu.add("Edit/Paste", Shortcut::Ctrl | 'v', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::EditPaste) });
    menu.add("Edit/Select All", Shortcut::Ctrl | 'a', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::SelectAll) });
    menu.add("Edit/Find...", Shortcut::Ctrl | 'f', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::ShowFind) });
    menu.add("Edit/Replace...", Shortcut::Ctrl | 'h', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::ShowReplace) });
    menu.add("Edit/Go To Line...", Shortcut::Ctrl | 'g', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::ShowGoToLine) });

    // View
    let ln_flag = if settings.line_numbers_enabled { MenuFlag::Toggle | MenuFlag::Value } else { MenuFlag::Toggle };
    menu.add("View/Toggle Line Numbers", Shortcut::None, ln_flag, { let s = *s; move |_| s.send(Message::ToggleLineNumbers) });
    let ww_flag = if settings.word_wrap_enabled { MenuFlag::Toggle | MenuFlag::Value } else { MenuFlag::Toggle };
    menu.add("View/Toggle Word Wrap", Shortcut::None, ww_flag, { let s = *s; move |_| s.send(Message::ToggleWordWrap) });
    let dm_flag = if initial_dark_mode { MenuFlag::Toggle | MenuFlag::Value } else { MenuFlag::Toggle };
    menu.add("View/Toggle Dark Mode", Shortcut::None, dm_flag, { let s = *s; move |_| s.send(Message::ToggleDarkMode) });
    let hl_flag = if settings.highlighting_enabled { MenuFlag::Toggle | MenuFlag::Value } else { MenuFlag::Toggle };
    menu.add("View/Toggle Syntax Highlighting", Shortcut::None, hl_flag, { let s = *s; move |_| s.send(Message::ToggleHighlighting) });
    menu.add("View/Preview in Browser", Shortcut::Ctrl | 'm', MenuFlag::Normal, { let s = *s; move |_| s.send(Message::TogglePreview) });

    // Format
    menu.add("Format/Font/Screen (Bold)", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::SetFont(Font::ScreenBold)) });
    menu.add("Format/Font/Courier", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::SetFont(Font::Courier)) });
    menu.add("Format/Font/Helvetica Mono", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::SetFont(Font::Screen)) });
    menu.add("Format/Font Size/Small (12)", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::SetFontSize(12)) });
    menu.add("Format/Font Size/Medium (16)", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::SetFontSize(16)) });
    menu.add("Format/Font Size/Large (20)", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::SetFontSize(20)) });

    // Plugins - General submenu with core functionality
    let plugins_flag = if settings.plugins_enabled {
        MenuFlag::Toggle | MenuFlag::Value
    } else {
        MenuFlag::Toggle
    };
    menu.add(
        "Plugins/General/Enable Plugins",
        Shortcut::None,
        plugins_flag,
        {
            let s = *s;
            move |_| s.send(Message::PluginsToggleGlobal)
        },
    );

    // Parse the custom shortcut or fall back to default
    let run_checks_shortcut = parse_shortcut(&settings.run_all_checks_shortcut)
        .unwrap_or(Shortcut::Ctrl | Shortcut::Shift | 'l');
    menu.add(
        "Plugins/General/Run All Checks",
        run_checks_shortcut,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::ManualHighlight)
        },
    );
    menu.add(
        "Plugins/General/Reload All",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::PluginsReloadAll)
        },
    );
    menu.add(
        "Plugins/General/Plugin Manager...",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::ShowPluginManager)
        },
    );
    menu.add(
        "Plugins/General/Settings...",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::ShowPluginSettings)
        },
    );

    // Help
    menu.add("Help/About FerrisPad", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::ShowAbout) });
    menu.add("Help/Check for Updates...", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::CheckForUpdates) });
}

/// Remove all menu entries for a plugin by name.
/// This handles both flat toggles and submenu items.
fn remove_plugin_menu_entries(menu: &mut MenuBar, name: &str) {
    // Remove flat toggle if exists
    let flat_label = format!("Plugins/{}", name);
    loop {
        let idx = menu.find_index(&flat_label);
        if idx < 0 {
            break;
        }
        menu.remove(idx);
    }

    // Remove submenu items by trying common patterns
    let submenu_prefix = format!("Plugins/{}/", name);

    // Remove known entries: Enable, Settings...
    for suffix in ["Enable", "Settings..."] {
        loop {
            let idx = menu.find_index(&format!("{}{}", submenu_prefix, suffix));
            if idx < 0 {
                break;
            }
            menu.remove(idx);
        }
    }

    // Try to remove any remaining submenu entries
    // FLTK find_index can match partial paths, so this will find any item
    // that starts with "Plugins/{name}/"
    for _ in 0..50 {
        // Limit iterations to avoid infinite loop
        let idx = menu.find_index(&submenu_prefix);
        if idx < 0 {
            break;
        }
        menu.remove(idx);
    }
}

/// Rebuild the plugins submenu with the current list of plugins.
/// Plugins with menu_items get their own submenu; plugins without stay flat.
pub fn rebuild_plugins_menu(
    menu: &mut MenuBar,
    sender: &Sender<Message>,
    settings: &AppSettings,
    plugins: &PluginManager,
) {
    rebuild_plugins_menu_with_orphans(menu, sender, settings, plugins, &[]);
}

/// Rebuild the plugins menu, also cleaning up entries for orphaned plugins
/// (plugins that were uninstalled and are no longer in the plugin list)
pub fn rebuild_plugins_menu_with_orphans(
    menu: &mut MenuBar,
    sender: &Sender<Message>,
    settings: &AppSettings,
    plugins: &PluginManager,
    orphaned_names: &[String],
) {
    // Build set of reserved shortcuts
    let mut used_shortcuts: HashSet<String> = RESERVED_SHORTCUTS
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Find and remove old plugin entries (except Enable Plugins, Reload All, Run Checks)
    // This is a bit hacky but FLTK doesn't have great dynamic menu support

    // Note: We no longer use text separators "---". Instead, we use MenuDivider flag
    // on menu items to draw visual divider lines below them.

    // Remove the "Installed Plugins" label if it exists
    loop {
        let idx = menu.find_index("Plugins/───── Installed Plugins ─────");
        if idx < 0 {
            break;
        }
        menu.remove(idx);
    }

    // Remove all plugin-related entries (flat toggles and submenus)
    let plugin_list = plugins.list_plugins();
    for plugin in plugin_list.iter() {
        remove_plugin_menu_entries(menu, &plugin.name);
    }

    // Also remove entries for orphaned plugins (just uninstalled)
    for name in orphaned_names {
        remove_plugin_menu_entries(menu, name);
    }

    // Add plugins to menu if any exist
    if !plugin_list.is_empty() {
        // Find position after "General/Settings..." to insert plugins
        let settings_idx = menu.find_index("Plugins/General/Settings...");
        if settings_idx >= 0 {
            let mut insert_pos = settings_idx + 1;

            // Add "Installed Plugins" label
            menu.insert(
                insert_pos,
                "Plugins/───── Installed Plugins ─────",
                Shortcut::None,
                MenuFlag::Normal,
                |_| {},
            );
            insert_pos += 1;

            // Add each plugin
            for plugin in plugin_list {
                if plugin.menu_items.is_empty() {
                    // Plugin without menu items: flat toggle (backward compatible)
                    let flag = if plugin.enabled {
                        MenuFlag::Toggle | MenuFlag::Value
                    } else {
                        MenuFlag::Toggle
                    };

                    let label = format!("Plugins/{}", plugin.name);
                    let plugin_name = plugin.name.clone();
                    let s = *sender;

                    menu.insert(
                        insert_pos,
                        &label,
                        Shortcut::None,
                        flag,
                        move |_| s.send(Message::PluginToggle(plugin_name.clone())),
                    );
                    insert_pos += 1;
                } else {
                    // Plugin with menu items: create submenu
                    let submenu_base = format!("Plugins/{}", plugin.name);

                    // Enable toggle as first item in submenu, with divider below it
                    let enable_label = format!("{}/Enable", submenu_base);
                    let flag = if plugin.enabled {
                        MenuFlag::Toggle | MenuFlag::Value | MenuFlag::MenuDivider
                    } else {
                        MenuFlag::Toggle | MenuFlag::MenuDivider
                    };

                    let plugin_name = plugin.name.clone();
                    let s = *sender;

                    menu.insert(
                        insert_pos,
                        &enable_label,
                        Shortcut::None,
                        flag,
                        move |_| s.send(Message::PluginToggle(plugin_name.clone())),
                    );
                    insert_pos += 1;

                    // Add "Settings..." menu item for plugin configuration
                    let settings_label = format!("{}/Settings...", submenu_base);
                    let plugin_name_settings = plugin.name.clone();
                    let s = *sender;

                    menu.insert(
                        insert_pos,
                        &settings_label,
                        Shortcut::None,
                        MenuFlag::Normal | MenuFlag::MenuDivider,
                        move |_| s.send(Message::ShowPluginConfig(plugin_name_settings.clone())),
                    );
                    insert_pos += 1;

                    // Add each menu item
                    let mut is_first = true;
                    for item in &plugin.menu_items {
                        let item_label = format!("{}/{}", submenu_base, item.label);

                        // Look up per-action shortcut from plugin config params
                        // Convention: "run_X" action -> "X_shortcut" param key
                        // For "lint" action (primary), use the legacy shortcut field
                        let plugin_config = settings.plugin_configs.get(&plugin.name);

                        // Build shortcut key: "run_foo" -> "foo_shortcut"
                        let shortcut_key = if item.action.starts_with("run_") {
                            format!("{}_shortcut", &item.action[4..])
                        } else {
                            format!("{}_shortcut", item.action)
                        };

                        // Priority: config param shortcut > legacy shortcut field > manifest shortcut
                        let shortcut_str: Option<&str> =
                            if is_first && item.action == "lint" {
                                // Primary lint action: prefer top-level shortcut override
                                plugin_config
                                    .and_then(|c| c.shortcut.as_deref())
                                    .or(item.shortcut.as_deref())
                            } else {
                                // Per-action shortcut from config params
                                plugin_config
                                    .and_then(|c| c.params.get(&shortcut_key))
                                    .map(String::as_str)
                                    .filter(|s| !s.is_empty())
                                    .or(item.shortcut.as_deref())
                            };

                        // Parse and validate shortcut
                        let shortcut = if let Some(sc_str) = shortcut_str {
                            let normalized = normalize_shortcut(sc_str);
                            if used_shortcuts.contains(&normalized) {
                                eprintln!(
                                    "[plugins] Warning: shortcut '{}' for {}/{} conflicts with existing shortcut, skipping",
                                    sc_str, plugin.name, item.label
                                );
                                Shortcut::None
                            } else if let Some(sc) = parse_shortcut(sc_str) {
                                used_shortcuts.insert(normalized);
                                sc
                            } else {
                                eprintln!(
                                    "[plugins] Warning: invalid shortcut '{}' for {}/{}",
                                    sc_str, plugin.name, item.label
                                );
                                Shortcut::None
                            }
                        } else {
                            Shortcut::None
                        };

                        is_first = false;

                        let plugin_name = plugin.name.clone();
                        let action = item.action.clone();
                        let s = *sender;

                        menu.insert(
                            insert_pos,
                            &item_label,
                            shortcut,
                            MenuFlag::Normal,
                            move |_| {
                                s.send(Message::PluginMenuAction {
                                    plugin_name: plugin_name.clone(),
                                    action: action.clone(),
                                })
                            },
                        );
                        insert_pos += 1;
                    }
                }
            }
        }
    }

    // Update the global enable checkbox
    let idx = menu.find_index("Plugins/General/Enable Plugins");
    if idx >= 0
        && let Some(mut item) = menu.at(idx)
    {
        if settings.plugins_enabled {
            item.set();
        } else {
            item.clear();
        }
    }
}

// ============================================================================
// Menu Item State Utilities
// ============================================================================
// These functions provide reusable patterns for enabling/disabling menu items
// based on context (file type, plugin state, etc.)

/// Set a menu item's enabled state by its path.
/// Returns true if the item was found and updated.
pub fn set_menu_item_enabled(menu: &mut MenuBar, path: &str, enabled: bool) -> bool {
    let idx = menu.find_index(path);
    if idx >= 0 {
        if let Some(mut item) = menu.at(idx) {
            if enabled {
                item.activate();
            } else {
                item.deactivate();
            }
            return true;
        }
    }
    false
}

/// Check if a file path matches any of the given extensions (case-insensitive).
/// Extensions should include the dot, e.g., &[".py", ".pyi"]
pub fn file_matches_extensions(file_path: Option<&str>, extensions: &[&str]) -> bool {
    file_path
        .map(|p| {
            let lower = p.to_lowercase();
            extensions.iter().any(|ext| lower.ends_with(ext))
        })
        .unwrap_or(false)
}

/// Update a menu item's enabled state based on file extension.
/// This is the primary utility for lazy-loading menu items based on file type.
///
/// # Example
/// ```ignore
/// // Enable "Run Lint" only for Python files
/// update_menu_for_extensions(menu, "Plugins/Python Lint/Run Lint", file_path, &[".py", ".pyi"]);
///
/// // Enable preview only for Markdown files
/// update_menu_for_extensions(menu, "View/Preview in Browser", file_path, &[".md", ".markdown"]);
/// ```
pub fn update_menu_for_extensions(
    menu: &mut MenuBar,
    menu_path: &str,
    file_path: Option<&str>,
    extensions: &[&str],
) -> bool {
    let enabled = file_matches_extensions(file_path, extensions);
    set_menu_item_enabled(menu, menu_path, enabled)
}

/// Update multiple menu items based on file extension.
/// Useful for plugins that register multiple actions for a specific file type.
///
/// # Example
/// ```ignore
/// // Enable all Python Lint menu items for Python files
/// update_menus_for_extensions(
///     menu,
///     &["Plugins/Python Lint/Run Lint", "Plugins/Python Lint/Format"],
///     file_path,
///     &[".py", ".pyi"]
/// );
/// ```
pub fn update_menus_for_extensions(
    menu: &mut MenuBar,
    menu_paths: &[&str],
    file_path: Option<&str>,
    extensions: &[&str],
) {
    let enabled = file_matches_extensions(file_path, extensions);
    for path in menu_paths {
        set_menu_item_enabled(menu, path, enabled);
    }
}

// ============================================================================
// Specific Menu Update Functions
// ============================================================================

/// Update the "Preview in Browser" menu item based on whether the current file is markdown.
/// Only enables the menu item for .md files.
pub fn update_preview_menu(menu: &mut MenuBar, file_path: Option<&str>) {
    update_menu_for_extensions(
        menu,
        "View/Preview in Browser",
        file_path,
        &[".md", ".markdown"],
    );
}

/// Update plugin menu items based on the current file type.
/// Each plugin can specify which file extensions its menu items should be active for.
///
/// # Arguments
/// * `menu` - The menu bar to update
/// * `plugins` - Reference to the plugin manager to get plugin info
/// * `file_path` - The current file path (if any)
pub fn update_plugin_menus_for_file(
    menu: &mut MenuBar,
    plugins: &crate::app::plugins::PluginManager,
    file_path: Option<&str>,
) {
    for plugin in plugins.list_plugins() {
        if !plugin.enabled || plugin.menu_items.is_empty() {
            continue;
        }

        // Determine which extensions this plugin supports based on its name/type
        // This could be extended to read from plugin.toml in the future
        let extensions: &[&str] = match plugin.name.to_lowercase().as_str() {
            name if name.contains("python") => &[".py", ".pyi", ".pyw"],
            name if name.contains("rust") => &[".rs"],
            name if name.contains("javascript") || name.contains("js") => &[".js", ".jsx", ".mjs"],
            name if name.contains("typescript") || name.contains("ts") => &[".ts", ".tsx"],
            name if name.contains("markdown") || name.contains("md") => &[".md", ".markdown"],
            name if name.contains("json") => &[".json"],
            name if name.contains("toml") => &[".toml"],
            name if name.contains("yaml") || name.contains("yml") => &[".yaml", ".yml"],
            name if name.contains("html") => &[".html", ".htm"],
            name if name.contains("css") => &[".css", ".scss", ".sass", ".less"],
            name if name.contains("lua") => &[".lua"],
            name if name.contains("go") => &[".go"],
            name if name.contains("c++") || name.contains("cpp") => &[".cpp", ".cc", ".cxx", ".hpp", ".h"],
            name if name.contains("c") && !name.contains("css") => &[".c", ".h"],
            _ => continue, // Unknown plugin type - leave all items enabled
        };

        // Update each menu item for this plugin
        for item in &plugin.menu_items {
            let menu_path = format!("Plugins/{}/{}", plugin.name, item.label);
            update_menu_for_extensions(menu, &menu_path, file_path, extensions);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shortcut_ctrl_key() {
        let sc = parse_shortcut("Ctrl+P").unwrap();
        assert_eq!(sc, Shortcut::Ctrl | 'p');
    }

    #[test]
    fn test_parse_shortcut_ctrl_shift_key() {
        let sc = parse_shortcut("Ctrl+Shift+P").unwrap();
        assert_eq!(sc, Shortcut::Ctrl | Shortcut::Shift | 'p');
    }

    #[test]
    fn test_parse_shortcut_alt_key() {
        let sc = parse_shortcut("Alt+F").unwrap();
        assert_eq!(sc, Shortcut::Alt | 'f');
    }

    #[test]
    fn test_parse_shortcut_function_key() {
        let sc = parse_shortcut("Ctrl+F5").unwrap();
        assert_eq!(sc, Shortcut::Ctrl | Key::F5);
    }

    #[test]
    fn test_parse_shortcut_case_insensitive() {
        let sc1 = parse_shortcut("ctrl+shift+p").unwrap();
        let sc2 = parse_shortcut("CTRL+SHIFT+P").unwrap();
        assert_eq!(sc1, sc2);
    }

    #[test]
    fn test_parse_shortcut_empty() {
        assert!(parse_shortcut("").is_none());
    }

    #[test]
    fn test_parse_shortcut_no_key() {
        assert!(parse_shortcut("Ctrl+Shift").is_none());
    }

    #[test]
    fn test_parse_shortcut_invalid_key() {
        assert!(parse_shortcut("Ctrl+!!!").is_none());
    }

    #[test]
    fn test_normalize_shortcut() {
        assert_eq!(normalize_shortcut("Ctrl+Shift+P"), "ctrl+shift+p");
        assert_eq!(normalize_shortcut("Shift+Ctrl+P"), "ctrl+shift+p");
        assert_eq!(normalize_shortcut("CTRL+p"), "ctrl+p");
    }
}
