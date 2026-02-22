use fltk::{
    app::Sender,
    enums::{Font, Key, Shortcut},
    menu::{MenuBar, MenuFlag},
    prelude::*,
};

use crate::app::plugins::PluginManager;
use crate::app::{AppSettings, Message};

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

    // Plugins - static menu, will be rebuilt when plugins change
    let plugins_flag = if settings.plugins_enabled {
        MenuFlag::Toggle | MenuFlag::Value
    } else {
        MenuFlag::Toggle
    };
    menu.add(
        "Plugins/Enable Plugins",
        Shortcut::None,
        plugins_flag,
        {
            let s = *s;
            move |_| s.send(Message::PluginsToggleGlobal)
        },
    );
    menu.add(
        "Plugins/Reload All",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let s = *s;
            move |_| s.send(Message::PluginsReloadAll)
        },
    );

    // Help
    menu.add("Help/About FerrisPad", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::ShowAbout) });
    menu.add("Help/Check for Updates...", Shortcut::None, MenuFlag::Normal, { let s = *s; move |_| s.send(Message::CheckForUpdates) });
}

/// Rebuild the plugins submenu with the current list of plugins
pub fn rebuild_plugins_menu(
    menu: &mut MenuBar,
    sender: &Sender<Message>,
    settings: &AppSettings,
    plugins: &PluginManager,
) {
    // Find and remove old plugin entries (except Enable Plugins and Reload All)
    // This is a bit hacky but FLTK doesn't have great dynamic menu support
    // FLTK's find_index returns -1 if not found, not Option

    // Clear all Plugins/* separator entries
    loop {
        let idx = menu.find_index("Plugins/---");
        if idx < 0 {
            break;
        }
        menu.remove(idx);
    }

    // Remove individual plugin entries (they start after the separator)
    let plugin_names: Vec<String> = plugins
        .list_plugins()
        .iter()
        .map(|p| format!("Plugins/{}", p.name))
        .collect();

    for name in &plugin_names {
        let idx = menu.find_index(name);
        if idx >= 0 {
            menu.remove(idx);
        }
    }

    // Add separator if there are plugins
    let plugin_list = plugins.list_plugins();
    if !plugin_list.is_empty() {
        // Find position after "Reload All" to insert separator
        let reload_idx = menu.find_index("Plugins/Reload All");
        if reload_idx >= 0 {
            menu.insert(
                reload_idx + 1,
                "---",
                Shortcut::None,
                MenuFlag::Normal,
                |_| {},
            );

            // Add each plugin with a toggle
            for (i, plugin) in plugin_list.iter().enumerate() {
                let flag = if plugin.enabled {
                    MenuFlag::Toggle | MenuFlag::Value
                } else {
                    MenuFlag::Toggle
                };

                let label = format!("Plugins/{}", plugin.name);

                let plugin_name = plugin.name.clone();
                let s = *sender;

                menu.insert(
                    reload_idx + 2 + i as i32,
                    &label,
                    Shortcut::None,
                    flag,
                    move |_| s.send(Message::PluginToggle(plugin_name.clone())),
                );
            }
        }
    }

    // Update the global enable checkbox
    let idx = menu.find_index("Plugins/Enable Plugins");
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
