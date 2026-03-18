use std::collections::HashSet;

use fltk::{
    app::Sender,
    enums::{Font, Key, Shortcut},
    menu::{MenuBar, MenuFlag},
    prelude::*,
};

use crate::app::plugins::{PluginManager, plugin_display_name};
use crate::app::services::shortcut_registry::{ShortcutRegistry, normalize_shortcut};
use crate::app::{AppSettings, Message};

/// Reserved keyboard shortcuts that plugins cannot override.
/// These are built-in editor functions.
const RESERVED_SHORTCUTS: &[&str] = &[
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

/// Built-in shortcuts: (menu_path/command_id, default_shortcut_string).
/// This table is the single source of truth for default built-in shortcuts.
/// The `tabs_enabled` variants are handled separately in `build_menu()`.
pub const BUILTIN_SHORTCUTS: &[(&str, &str)] = &[
    ("File/New", "Ctrl+T"), // Ctrl+N when tabs off
    ("File/Open...", "Ctrl+O"),
    ("File/Save", "Ctrl+S"),
    ("File/Save As...", "Ctrl+Shift+S"),
    ("File/Close Tab", "Ctrl+W"),            // tabs only
    ("File/Next Tab", "Ctrl+Tab"),           // tabs only
    ("File/Previous Tab", "Ctrl+Shift+Tab"), // tabs only
    ("File/Quit", "Ctrl+Q"),
    ("Edit/Undo", "Ctrl+Z"),
    ("Edit/Redo", "Ctrl+Shift+Z"),
    ("Edit/Cut", "Ctrl+X"),
    ("Edit/Copy", "Ctrl+C"),
    ("Edit/Paste", "Ctrl+V"),
    ("Edit/Select All", "Ctrl+A"),
    ("Edit/Find...", "Ctrl+F"),
    ("Edit/Replace...", "Ctrl+H"),
    ("Edit/Go To Line...", "Ctrl+G"),
    ("View/Preview in Browser", "Ctrl+M"),
    ("Plugins/General/Run All Checks", "Ctrl+Shift+L"),
];

/// Resolve the effective shortcut for a built-in command.
/// Returns the FLTK Shortcut from the registry override or the provided default string.
pub fn resolve_shortcut(registry: &ShortcutRegistry, id: &str, default: &str) -> Shortcut {
    let effective = registry.effective_shortcut(id, default);
    if effective.is_empty() {
        return Shortcut::None;
    }
    parse_shortcut(effective).unwrap_or(Shortcut::None)
}

/// Apply shortcut overrides to an existing menu bar (in-place mutation).
/// This avoids a full menu rebuild when only shortcuts change.
pub fn apply_shortcut_overrides(
    menu: &mut MenuBar,
    registry: &ShortcutRegistry,
    tabs_enabled: bool,
) {
    for &(id, default) in BUILTIN_SHORTCUTS {
        // Skip tab-only shortcuts when tabs are disabled
        if !tabs_enabled && matches!(id, "File/Close Tab" | "File/Next Tab" | "File/Previous Tab") {
            continue;
        }
        // Handle File/New special case (Ctrl+N when tabs off)
        let actual_default = if id == "File/New" && !tabs_enabled {
            "Ctrl+N"
        } else {
            default
        };

        let shortcut = resolve_shortcut(registry, id, actual_default);
        let idx = menu.find_index(id);
        if idx >= 0
            && let Some(mut item) = menu.at(idx)
        {
            item.set_shortcut(shortcut);
        }
    }
}

/// Build a plugin command ID from plugin name and action.
pub fn plugin_command_id(plugin_name: &str, action: &str) -> String {
    format!("plugin:{}:{}", plugin_name, action)
}

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
            "ctrl" | "control" => result |= Shortcut::Ctrl,
            "shift" => result |= Shortcut::Shift,
            "alt" => result |= Shortcut::Alt,
            _ => {
                // This should be the key
                if has_key {
                    // Multiple keys specified, invalid
                    return None;
                }
                has_key = true;

                // Handle function keys F1-F12
                if lower.starts_with('f')
                    && lower.len() >= 2
                    && let Ok(num) = lower[1..].parse::<u32>()
                    && (1..=12).contains(&num)
                {
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

                // Named special keys
                match lower.as_str() {
                    "tab" => {
                        result = result | Key::Tab;
                        continue;
                    }
                    "backspace" => {
                        result = result | Key::BackSpace;
                        continue;
                    }
                    "delete" | "del" => {
                        result = result | Key::Delete;
                        continue;
                    }
                    "insert" | "ins" => {
                        result = result | Key::Insert;
                        continue;
                    }
                    "home" => {
                        result = result | Key::Home;
                        continue;
                    }
                    "end" => {
                        result = result | Key::End;
                        continue;
                    }
                    "pageup" | "pgup" => {
                        result = result | Key::PageUp;
                        continue;
                    }
                    "pagedown" | "pgdn" => {
                        result = result | Key::PageDown;
                        continue;
                    }
                    "left" => {
                        result = result | Key::Left;
                        continue;
                    }
                    "right" => {
                        result = result | Key::Right;
                        continue;
                    }
                    "up" => {
                        result = result | Key::Up;
                        continue;
                    }
                    "down" => {
                        result = result | Key::Down;
                        continue;
                    }
                    "space" => {
                        result = result | Key::from_char(' ');
                        continue;
                    }
                    "escape" | "esc" => {
                        result = result | Key::Escape;
                        continue;
                    }
                    _ => {}
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
    registry: &ShortcutRegistry,
) {
    let s = sender;

    // Helper: resolve shortcut from registry, with special-case for File/New
    let rs = |id: &str| -> Shortcut {
        let default = BUILTIN_SHORTCUTS
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, v)| *v)
            .unwrap_or("");
        // File/New uses Ctrl+N when tabs disabled
        let actual_default = if id == "File/New" && !tabs_enabled {
            "Ctrl+N"
        } else {
            default
        };
        resolve_shortcut(registry, id, actual_default)
    };

    // File
    menu.add("File/New", rs("File/New"), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::FileNew)
    });
    menu.add("File/Open...", rs("File/Open..."), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::FileOpen)
    });
    menu.add("File/Save", rs("File/Save"), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::FileSave)
    });
    menu.add(
        "File/Save As...",
        rs("File/Save As..."),
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::FileSaveAs)
        },
    );
    if tabs_enabled {
        menu.add("File/Close Tab", rs("File/Close Tab"), MenuFlag::Normal, {
            let s = *s;
            move |_| s.send(Message::TabCloseActive)
        });
        menu.add("File/Next Tab", rs("File/Next Tab"), MenuFlag::Normal, {
            let s = *s;
            move |_| s.send(Message::TabNext)
        });
        menu.add(
            "File/Previous Tab",
            rs("File/Previous Tab"),
            MenuFlag::Normal,
            {
                let s = *s;
                move |_| s.send(Message::TabPrevious)
            },
        );
    }
    menu.add("File/Settings...", Shortcut::None, MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::OpenSettings)
    });
    menu.add("File/Quit", rs("File/Quit"), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::FileQuit)
    });

    // Edit
    menu.add("Edit/Undo", rs("Edit/Undo"), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::EditUndo)
    });
    menu.add("Edit/Redo", rs("Edit/Redo"), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::EditRedo)
    });
    menu.add("Edit/Cut", rs("Edit/Cut"), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::EditCut)
    });
    menu.add("Edit/Copy", rs("Edit/Copy"), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::EditCopy)
    });
    menu.add("Edit/Paste", rs("Edit/Paste"), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::EditPaste)
    });
    menu.add(
        "Edit/Select All",
        rs("Edit/Select All"),
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::SelectAll)
        },
    );
    menu.add("Edit/Find...", rs("Edit/Find..."), MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::ShowFind)
    });
    menu.add(
        "Edit/Replace...",
        rs("Edit/Replace..."),
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::ShowReplace)
        },
    );
    menu.add(
        "Edit/Go To Line...",
        rs("Edit/Go To Line..."),
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::ShowGoToLine)
        },
    );
    menu.add("Edit/Key Shortcuts...", Shortcut::None, MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::ShowKeyShortcuts)
    });

    // View
    let ln_flag = if settings.line_numbers_enabled {
        MenuFlag::Toggle | MenuFlag::Value
    } else {
        MenuFlag::Toggle
    };
    menu.add("View/Toggle Line Numbers", Shortcut::None, ln_flag, {
        let s = *s;
        move |_| s.send(Message::ToggleLineNumbers)
    });
    let ww_flag = if settings.word_wrap_enabled {
        MenuFlag::Toggle | MenuFlag::Value
    } else {
        MenuFlag::Toggle
    };
    menu.add("View/Toggle Word Wrap", Shortcut::None, ww_flag, {
        let s = *s;
        move |_| s.send(Message::ToggleWordWrap)
    });
    let dm_flag = if initial_dark_mode {
        MenuFlag::Toggle | MenuFlag::Value
    } else {
        MenuFlag::Toggle
    };
    menu.add("View/Toggle Dark Mode", Shortcut::None, dm_flag, {
        let s = *s;
        move |_| s.send(Message::ToggleDarkMode)
    });
    let hl_flag = if settings.highlighting_enabled {
        MenuFlag::Toggle | MenuFlag::Value
    } else {
        MenuFlag::Toggle
    };
    menu.add(
        "View/Toggle Syntax Highlighting",
        Shortcut::None,
        hl_flag,
        {
            let s = *s;
            move |_| s.send(Message::ToggleHighlighting)
        },
    );
    menu.add(
        "View/Preview in Browser",
        rs("View/Preview in Browser"),
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::TogglePreview)
        },
    );
    menu.add(
        "View/Diagnostics Panel",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::ToggleDiagnosticsPanel)
        },
    );

    // Format
    menu.add(
        "Format/Font/Screen (Bold)",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::SetFont(Font::ScreenBold))
        },
    );
    menu.add("Format/Font/Courier", Shortcut::None, MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::SetFont(Font::Courier))
    });
    menu.add(
        "Format/Font/Helvetica Mono",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::SetFont(Font::Screen))
        },
    );
    menu.add(
        "Format/Font Size/Small (12)",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::SetFontSize(12))
        },
    );
    menu.add(
        "Format/Font Size/Medium (16)",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::SetFontSize(16))
        },
    );
    menu.add(
        "Format/Font Size/Large (20)",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::SetFontSize(20))
        },
    );

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

    menu.add(
        "Plugins/General/Run All Checks",
        rs("Plugins/General/Run All Checks"),
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
    menu.add("Help/About FerrisPad", Shortcut::None, MenuFlag::Normal, {
        let s = *s;
        move |_| s.send(Message::ShowAbout)
    });
    menu.add(
        "Help/Check for Updates...",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::CheckForUpdates)
        },
    );
}

/// Remove all menu entries for a plugin by name.
/// This handles both flat toggles and submenu items.
fn remove_plugin_menu_entries(menu: &mut MenuBar, name: &str) {
    let display = plugin_display_name(name);

    // Remove flat toggle if exists (try both raw name and display name)
    for label_name in [name, display.as_str()] {
        let flat_label = format!("Plugins/{}", label_name);
        loop {
            let idx = menu.find_index(&flat_label);
            if idx < 0 {
                break;
            }
            menu.remove(idx);
        }
    }

    // Remove submenu items by trying common patterns (both raw and display name)
    let submenu_prefix = format!("Plugins/{}/", display);

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
    registry: &ShortcutRegistry,
) {
    rebuild_plugins_menu_with_orphans(menu, sender, settings, plugins, &[], registry);
}

/// Rebuild the plugins menu, also cleaning up entries for orphaned plugins
/// (plugins that were uninstalled and are no longer in the plugin list)
pub fn rebuild_plugins_menu_with_orphans(
    menu: &mut MenuBar,
    sender: &Sender<Message>,
    settings: &AppSettings,
    plugins: &PluginManager,
    orphaned_names: &[String],
    registry: &ShortcutRegistry,
) {
    // Build set of reserved shortcuts
    let mut used_shortcuts: HashSet<String> =
        RESERVED_SHORTCUTS.iter().map(|s| s.to_string()).collect();

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

                    let display = plugin_display_name(&plugin.name);
                    let label = format!("Plugins/{}", display);
                    let plugin_name = plugin.name.clone();
                    let s = *sender;

                    menu.insert(insert_pos, &label, Shortcut::None, flag, move |_| {
                        s.send(Message::PluginToggle(plugin_name.clone()))
                    });
                    insert_pos += 1;
                } else {
                    // Plugin with menu items: create submenu
                    let display = plugin_display_name(&plugin.name);
                    let submenu_base = format!("Plugins/{}", display);

                    // Enable toggle as first item in submenu, with divider below it
                    let enable_label = format!("{}/Enable", submenu_base);
                    let flag = if plugin.enabled {
                        MenuFlag::Toggle | MenuFlag::Value | MenuFlag::MenuDivider
                    } else {
                        MenuFlag::Toggle | MenuFlag::MenuDivider
                    };

                    let plugin_name = plugin.name.clone();
                    let s = *sender;

                    menu.insert(insert_pos, &enable_label, Shortcut::None, flag, move |_| {
                        s.send(Message::PluginToggle(plugin_name.clone()))
                    });
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
                    for item in &plugin.menu_items {
                        let item_label = format!("{}/{}", submenu_base, item.label);

                        // Shortcut resolution: registry override > manifest default
                        let cmd_id = plugin_command_id(&plugin.name, &item.action);
                        let manifest_default = item.shortcut.as_deref().unwrap_or("");

                        let shortcut_str: Option<&str> =
                            if let Some(ovr) = registry.get_override(&cmd_id) {
                                if ovr.enabled && !ovr.shortcut.is_empty() {
                                    Some(&ovr.shortcut)
                                } else if ovr.enabled {
                                    None // explicitly unbound
                                } else {
                                    Some(manifest_default).filter(|s| !s.is_empty())
                                }
                            } else {
                                Some(manifest_default).filter(|s| !s.is_empty())
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
    if idx >= 0
        && let Some(mut item) = menu.at(idx)
    {
        if enabled {
            item.activate();
        } else {
            item.deactivate();
        }
        return true;
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
