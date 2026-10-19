use std::collections::HashSet;

use fltk::{
    app::Sender,
    enums::{Key, Shortcut},
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
    "ctrl+shift+n", // New Session Window
    "ctrl+r",
    "ctrl+shift+r",
];

/// Built-in shortcuts: (menu_path/command_id, default_shortcut_string).
/// This table is the single source of truth for default built-in shortcuts:
/// a menu item from `builtin_menu_entries()` gets a shortcut iff its path has
/// a row here. Read it through `builtin_shortcut_defaults()`, which applies
/// the `tabs_enabled` variants.
pub const BUILTIN_SHORTCUTS: &[(&str, &str)] = &[
    ("File/New", "Ctrl+T"), // Ctrl+N when tabs off
    ("File/Open...", "Ctrl+O"),
    ("File/Save", "Ctrl+S"),
    ("File/Save As...", "Ctrl+Shift+S"),
    ("File/Reload", "Ctrl+R"),
    ("File/Reload All", "Ctrl+Shift+R"),
    ("File/Close Tab", "Ctrl+W"),            // tabs only
    ("File/Next Tab", "Ctrl+Tab"),           // tabs only
    ("File/Previous Tab", "Ctrl+Shift+Tab"), // tabs only
    ("File/New Session Window...", "Ctrl+Shift+N"),
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

/// Built-in commands whose menu items exist only when tabs are enabled.
const TAB_ONLY_COMMANDS: &[&str] = &["File/Close Tab", "File/Next Tab", "File/Previous Tab"];

/// The built-in `(command_id, default_shortcut)` pairs that apply for this
/// tab mode: tab-only commands are left out when tabs are disabled, and
/// `File/New` falls back to Ctrl+N. The menu build, the in-place override
/// pass and the shortcut dialog all read `BUILTIN_SHORTCUTS` through here, so
/// the tab-mode rules live in one place.
pub fn builtin_shortcut_defaults(
    tabs_enabled: bool,
) -> impl Iterator<Item = (&'static str, &'static str)> {
    BUILTIN_SHORTCUTS.iter().filter_map(move |&(id, default)| {
        if tabs_enabled {
            Some((id, default))
        } else if TAB_ONLY_COMMANDS.contains(&id) {
            None
        } else if id == "File/New" {
            Some((id, "Ctrl+N"))
        } else {
            Some((id, default))
        }
    })
}

/// Resolve the effective shortcut for a built-in command.
/// Returns the FLTK Shortcut from the registry override or the provided default string.
pub fn resolve_shortcut(registry: &ShortcutRegistry, id: &str, default: &str) -> Shortcut {
    let effective = registry.effective_shortcut(id, default);
    if effective.is_empty() {
        return Shortcut::None;
    }
    parse_shortcut(effective).unwrap_or(Shortcut::None)
}

/// Preset sizes shown in the Format/Font Size submenu.
pub const PRESET_FONT_SIZES: &[i32] = &[8, 10, 12, 14, 16, 18, 20, 24, 28, 32];

/// Compose the label for a Font Size submenu entry.
/// All entries are right-padded to the same column; the active size gets a
/// trailing `← current` marker so the menu doesn't look bare.
pub fn font_size_label(sz: i32, is_active: bool) -> String {
    if is_active {
        format!(" {:>3}  \u{2190} current ", sz)
    } else {
        format!(" {:>3}  ", sz)
    }
}

fn font_size_path(sz: i32, is_active: bool) -> String {
    format!("Format/Font Size/{}", font_size_label(sz, is_active))
}

/// Move the `← current` marker from `prev` to `new` in the Font Size submenu.
/// Each side is best-effort (skipped if no preset matches), so non-preset
/// sizes (e.g. picked via the font picker) just leave the menu unmarked.
pub fn apply_font_size_active(menu: &mut MenuBar, prev: i32, new: i32) {
    if prev == new {
        return;
    }
    let old_path = font_size_path(prev, true);
    let idx = menu.find_index(&old_path);
    if idx >= 0
        && let Some(mut item) = menu.at(idx)
    {
        item.set_label(&font_size_label(prev, false));
    }
    let new_path = font_size_path(new, false);
    let idx = menu.find_index(&new_path);
    if idx >= 0
        && let Some(mut item) = menu.at(idx)
    {
        item.set_label(&font_size_label(new, true));
    }
}

/// Apply shortcut overrides to an existing menu bar (in-place mutation).
/// This avoids a full menu rebuild when only shortcuts change.
pub fn apply_shortcut_overrides(
    menu: &mut MenuBar,
    registry: &ShortcutRegistry,
    tabs_enabled: bool,
) {
    for (id, default) in builtin_shortcut_defaults(tabs_enabled) {
        let shortcut = resolve_shortcut(registry, id, default);
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

/// One built-in menu item: its path (which is also its command ID), its flags
/// and the Message it emits. The shortcut is deliberately not stored here: it
/// is looked up by path in `BUILTIN_SHORTCUTS`, so a default shortcut lives in
/// exactly one place and a path typo on either side fails the unit tests.
#[derive(Debug, Clone)]
pub struct MenuEntry {
    pub path: String,
    pub flag: MenuFlag,
    pub msg: Message,
}

/// The built-in menu, in display order. Pure data (no FLTK widget), so the
/// menu/shortcut-table consistency is unit-testable.
pub fn builtin_menu_entries(
    settings: &AppSettings,
    initial_dark_mode: bool,
    tabs_enabled: bool,
) -> Vec<MenuEntry> {
    let item = |path: &str, flag: MenuFlag, msg: Message| MenuEntry {
        path: path.to_string(),
        flag,
        msg,
    };
    let normal = |path: &str, msg: Message| item(path, MenuFlag::Normal, msg);
    let toggle = |path: &str, on: bool, msg: Message| {
        let flag = if on {
            MenuFlag::Toggle | MenuFlag::Value
        } else {
            MenuFlag::Toggle
        };
        item(path, flag, msg)
    };

    // File
    let mut entries = vec![
        normal("File/New", Message::FileNew),
        normal("File/Open...", Message::FileOpen),
        normal("File/Save", Message::FileSave),
        normal("File/Save As...", Message::FileSaveAs),
        normal("File/Reload", Message::FileReload),
        normal("File/Reload All", Message::FileReloadAll),
        normal("File/Sessions...", Message::SessionShowPicker),
        normal("File/New Session Window...", Message::SessionNewWindow),
    ];
    if tabs_enabled {
        entries.extend([
            normal("File/Close Tab", Message::TabCloseActive),
            normal("File/Next Tab", Message::TabNext),
            normal("File/Previous Tab", Message::TabPrevious),
        ]);
    }
    entries.extend([
        normal("File/Settings...", Message::OpenSettings),
        normal("File/Quit", Message::FileQuit),
        // Edit
        normal("Edit/Undo", Message::EditUndo),
        normal("Edit/Redo", Message::EditRedo),
        normal("Edit/Cut", Message::EditCut),
        normal("Edit/Copy", Message::EditCopy),
        normal("Edit/Paste", Message::EditPaste),
        normal("Edit/Select All", Message::SelectAll),
        normal("Edit/Find...", Message::ShowFind),
        normal("Edit/Replace...", Message::ShowReplace),
        normal("Edit/Go To Line...", Message::ShowGoToLine),
        normal("Edit/Key Shortcuts...", Message::ShowKeyShortcuts),
        // View
        toggle(
            "View/Toggle Line Numbers",
            settings.line_numbers_enabled,
            Message::ToggleLineNumbers,
        ),
        toggle(
            "View/Toggle Word Wrap",
            settings.word_wrap_enabled,
            Message::ToggleWordWrap,
        ),
        toggle(
            "View/Toggle Dark Mode",
            initial_dark_mode,
            Message::ToggleDarkMode,
        ),
        toggle(
            "View/Toggle Syntax Highlighting",
            settings.highlighting_enabled,
            Message::ToggleHighlighting,
        ),
        normal("View/Preview in Browser", Message::TogglePreview),
        normal("View/Diagnostics Panel", Message::ToggleDiagnosticsPanel),
        // Format
        normal(
            "Format/Font/Screen (Bold)",
            Message::SetFont("ScreenBold".to_string()),
        ),
        normal(
            "Format/Font/Courier",
            Message::SetFont("Courier".to_string()),
        ),
        normal(
            "Format/Font/Helvetica Mono",
            Message::SetFont("HelveticaMono".to_string()),
        ),
        normal("Format/Font/More Fonts...", Message::OpenFontPicker),
    ]);
    let current_size = settings.font_size as i32;
    entries.extend(PRESET_FONT_SIZES.iter().map(|&sz| {
        normal(
            &font_size_path(sz, sz == current_size),
            Message::SetFontSize(sz),
        )
    }));
    entries.extend([
        // Plugins - General submenu with core functionality
        toggle(
            "Plugins/General/Enable Plugins",
            settings.plugins_enabled,
            Message::PluginsToggleGlobal,
        ),
        normal("Plugins/General/Run All Checks", Message::ManualHighlight),
        normal("Plugins/General/Reload All", Message::PluginsReloadAll),
        normal(
            "Plugins/General/Plugin Manager...",
            Message::ShowPluginManager,
        ),
        normal("Plugins/General/Settings...", Message::ShowPluginSettings),
        // Help
        normal("Help/About FerrisPad", Message::ShowAbout),
        normal("Help/Check for Updates...", Message::CheckForUpdates),
    ]);
    entries
}

pub fn build_menu(
    menu: &mut MenuBar,
    sender: &Sender<Message>,
    settings: &AppSettings,
    initial_dark_mode: bool,
    tabs_enabled: bool,
    registry: &ShortcutRegistry,
) {
    for entry in builtin_menu_entries(settings, initial_dark_mode, tabs_enabled) {
        // Items without a BUILTIN_SHORTCUTS row get no shortcut (and are not
        // rebindable), exactly as before.
        let shortcut = builtin_shortcut_defaults(tabs_enabled)
            .find(|&(id, _)| id == entry.path)
            .map_or(Shortcut::None, |(id, default)| {
                resolve_shortcut(registry, id, default)
            });
        menu.add_emit(&entry.path, shortcut, entry.flag, *sender, entry.msg);
    }
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

    fn entries(tabs_enabled: bool) -> Vec<MenuEntry> {
        builtin_menu_entries(&AppSettings::default(), false, tabs_enabled)
    }

    #[test]
    fn every_builtin_shortcut_names_exactly_one_menu_item() {
        // A typo in either the table or the menu path would orphan the
        // shortcut (it silently never fires); catch it here.
        for tabs_enabled in [true, false] {
            let entries = entries(tabs_enabled);
            for (id, _) in builtin_shortcut_defaults(tabs_enabled) {
                let hits = entries.iter().filter(|e| e.path == id).count();
                assert_eq!(
                    hits, 1,
                    "shortcut id {id:?} matches {hits} menu items (tabs_enabled={tabs_enabled})"
                );
            }
        }
    }

    #[test]
    fn every_builtin_table_row_is_reachable_with_tabs() {
        // With tabs on, every row of the raw table must be in the menu.
        let entries = entries(true);
        for &(id, _) in BUILTIN_SHORTCUTS {
            assert!(
                entries.iter().any(|e| e.path == id),
                "BUILTIN_SHORTCUTS row {id:?} has no menu item"
            );
        }
    }

    #[test]
    fn builtin_menu_paths_are_unique() {
        for tabs_enabled in [true, false] {
            let mut seen = HashSet::new();
            for e in entries(tabs_enabled) {
                assert!(
                    seen.insert(e.path.clone()),
                    "duplicate menu path {:?}",
                    e.path
                );
            }
        }
    }

    #[test]
    fn builtin_shortcut_ids_and_defaults_are_unique() {
        for tabs_enabled in [true, false] {
            let mut ids = HashSet::new();
            let mut keys = HashSet::new();
            for (id, default) in builtin_shortcut_defaults(tabs_enabled) {
                assert!(ids.insert(id), "duplicate shortcut id {id:?}");
                assert!(
                    keys.insert(normalize_shortcut(default)),
                    "default {default:?} of {id:?} is already bound (tabs_enabled={tabs_enabled})"
                );
            }
        }
    }

    #[test]
    fn builtin_defaults_parse_and_are_reserved_from_plugins() {
        // Plugins may not take a built-in default; RESERVED_SHORTCUTS must
        // cover every one of them in both tab modes.
        for tabs_enabled in [true, false] {
            for (id, default) in builtin_shortcut_defaults(tabs_enabled) {
                assert!(
                    is_valid_shortcut(default),
                    "{id:?} default {default:?} does not parse"
                );
                let normalized = normalize_shortcut(default);
                assert!(
                    RESERVED_SHORTCUTS.contains(&normalized.as_str()),
                    "{id:?} default {default:?} is not in RESERVED_SHORTCUTS"
                );
            }
        }
    }

    #[test]
    fn tab_mode_rules_for_builtin_defaults() {
        let with_tabs: Vec<_> = builtin_shortcut_defaults(true).collect();
        let without_tabs: Vec<_> = builtin_shortcut_defaults(false).collect();

        assert_eq!(with_tabs.len(), BUILTIN_SHORTCUTS.len());
        assert!(with_tabs.contains(&("File/New", "Ctrl+T")));
        assert!(with_tabs.contains(&("File/Close Tab", "Ctrl+W")));

        assert!(without_tabs.contains(&("File/New", "Ctrl+N")));
        for id in TAB_ONLY_COMMANDS {
            assert!(
                without_tabs.iter().all(|(i, _)| i != id),
                "{id:?} kept without tabs"
            );
            assert!(
                entries(false).iter().all(|e| e.path != *id),
                "{id:?} in menu without tabs"
            );
        }
    }

    #[test]
    fn run_all_checks_default_matches_settings_default() {
        // `AppState` mirrors this shortcut into `run_all_checks_shortcut`;
        // the table and the settings default must agree.
        let default = builtin_shortcut_defaults(true)
            .find(|&(id, _)| id == "Plugins/General/Run All Checks")
            .map(|(_, d)| d);
        assert_eq!(
            default,
            Some(AppSettings::default().run_all_checks_shortcut.as_str())
        );
    }

    #[test]
    fn registry_override_applies_to_a_builtin_menu_id() {
        // The registry keys built-ins by menu path; an override stored under a
        // table id must win over the table default.
        let mut registry = ShortcutRegistry::default();
        registry.set_override(
            "File/Save".to_string(),
            crate::app::domain::settings::ShortcutOverride {
                shortcut: "Ctrl+Shift+P".to_string(),
                enabled: true,
            },
        );
        assert_eq!(
            resolve_shortcut(&registry, "File/Save", "Ctrl+S"),
            Shortcut::Ctrl | Shortcut::Shift | 'p'
        );
        assert_eq!(
            resolve_shortcut(&registry, "File/Open...", "Ctrl+O"),
            Shortcut::Ctrl | 'o'
        );
    }

    #[test]
    fn toggle_entries_reflect_settings() {
        let settings = AppSettings {
            line_numbers_enabled: true,
            word_wrap_enabled: false,
            ..AppSettings::default()
        };
        let entries = builtin_menu_entries(&settings, true, true);
        let flag = |path: &str| entries.iter().find(|e| e.path == path).map(|e| e.flag);
        assert_eq!(
            flag("View/Toggle Line Numbers"),
            Some(MenuFlag::Toggle | MenuFlag::Value)
        );
        assert_eq!(flag("View/Toggle Word Wrap"), Some(MenuFlag::Toggle));
        assert_eq!(
            flag("View/Toggle Dark Mode"),
            Some(MenuFlag::Toggle | MenuFlag::Value)
        );
        assert_eq!(flag("File/Save"), Some(MenuFlag::Normal));
    }

    #[test]
    fn test_normalize_shortcut() {
        assert_eq!(normalize_shortcut("Ctrl+Shift+P"), "ctrl+shift+p");
        assert_eq!(normalize_shortcut("Shift+Ctrl+P"), "ctrl+shift+p");
        assert_eq!(normalize_shortcut("CTRL+p"), "ctrl+p");
    }
}
