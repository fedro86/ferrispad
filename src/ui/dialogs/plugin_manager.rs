//! Plugin Manager dialog for viewing, enabling/disabling, and installing plugins.
//!
//! VSCode-inspired design with:
//! - Themed tabs and buttons
//! - Icon letter placeholders
//! - Compact row layout with truncated descriptions

use fltk::{
    button::{Button, CheckButton},
    enums::{Align, Color, Font, FrameType},
    frame::Frame,
    group::{Group, Pack, PackType, Scroll, Tabs},
    prelude::*,
    window::Window,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::app::plugins::PluginManager;
use crate::app::services::plugin_registry::{
    fetch_plugin_registry, install_plugin, is_plugin_installed, is_update_available,
    AvailablePluginInfo,
};

use super::DialogTheme;

// Layout constants
const DIALOG_WIDTH: i32 = 550;
const DIALOG_HEIGHT: i32 = 480;
const PADDING: i32 = 10;
const TAB_HEIGHT: i32 = 30;
const BUTTON_HEIGHT: i32 = 30;
const PLUGIN_ROW_HEIGHT: i32 = 58;

// Row layout constants
const CHECKBOX_WIDTH: i32 = 24;
const ICON_SIZE: i32 = 32;
const ICON_MARGIN: i32 = 8;
const ACTION_BUTTON_WIDTH: i32 = 80;

// Description truncation (approximate chars that fit)
const DESC_MAX_CHARS: usize = 55;

/// Result from the plugin manager dialog
#[derive(Debug, Clone)]
pub enum PluginManagerResult {
    /// User toggled plugins on/off: (name, enabled)
    ToggledPlugins(Vec<(String, bool)>),
    /// User requested reload all
    ReloadAll,
    /// User installed new plugins
    InstalledPlugins(Vec<String>),
    /// User uninstalled plugins
    UninstalledPlugins(Vec<String>),
    /// Dialog closed with no changes
    Cancelled,
}

/// Truncate text to max_chars, adding "..." if truncated
fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated.trim_end())
    }
}

/// Get a color for the icon background based on the first letter
fn icon_color_for_letter(letter: char, is_dark: bool) -> Color {
    // Simple palette - 6 distinct colors that work in both dark and light
    let palette_dark: [(u8, u8, u8); 6] = [
        (100, 140, 180), // Blue-ish
        (140, 100, 160), // Purple-ish
        (100, 160, 140), // Teal-ish
        (160, 120, 100), // Brown-ish
        (140, 140, 100), // Olive-ish
        (160, 100, 120), // Rose-ish
    ];
    let palette_light: [(u8, u8, u8); 6] = [
        (70, 110, 150),
        (110, 70, 130),
        (70, 130, 110),
        (130, 90, 70),
        (110, 110, 70),
        (130, 70, 90),
    ];
    let idx = (letter.to_ascii_uppercase() as usize) % 6;
    let (r, g, b) = if is_dark {
        palette_dark[idx]
    } else {
        palette_light[idx]
    };
    Color::from_rgb(r, g, b)
}

/// Show the plugin manager dialog
///
/// Returns the result indicating what actions were taken
pub fn show_plugin_manager_dialog(
    plugins: &PluginManager,
    theme_bg: (u8, u8, u8),
) -> PluginManagerResult {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let is_dark = theme.is_dark();

    let mut dialog = Window::default()
        .with_size(DIALOG_WIDTH, DIALOG_HEIGHT)
        .with_label("Plugin Manager")
        .center_screen();
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    // Track result
    let result: Rc<RefCell<PluginManagerResult>> =
        Rc::new(RefCell::new(PluginManagerResult::Cancelled));

    // Track toggled plugins
    let toggles: Rc<RefCell<Vec<(String, bool)>>> = Rc::new(RefCell::new(Vec::new()));

    // Track installed plugins
    let installed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    // Track uninstalled plugins
    let uninstalled: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    // Cross-tab sync: map of plugin name -> Available tab button (to update on uninstall)
    let available_buttons: Rc<RefCell<HashMap<String, Button>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // Cross-tab sync: map of plugin name -> Installed tab row (to hide on update)
    let installed_rows: Rc<RefCell<HashMap<String, Group>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // Create tabs
    let tabs_y = PADDING;
    let tabs_height = DIALOG_HEIGHT - PADDING * 3 - BUTTON_HEIGHT - 10;
    let mut tabs = Tabs::default()
        .with_pos(PADDING, tabs_y)
        .with_size(DIALOG_WIDTH - PADDING * 2, tabs_height);
    tabs.set_color(theme.bg);
    tabs.set_selection_color(theme.button_bg);

    // ============ INSTALLED TAB ============
    let mut installed_group = Group::default()
        .with_pos(PADDING, tabs_y + TAB_HEIGHT)
        .with_size(DIALOG_WIDTH - PADDING * 2, tabs_height - TAB_HEIGHT)
        .with_label("Installed");
    installed_group.set_label_color(theme.text);
    installed_group.set_color(theme.bg);

    let mut scroll_installed = Scroll::default()
        .with_pos(PADDING + 5, tabs_y + TAB_HEIGHT + 5)
        .with_size(DIALOG_WIDTH - PADDING * 2 - 10, tabs_height - TAB_HEIGHT - 10);
    scroll_installed.set_color(theme.bg);

    let mut pack_installed_inner = Pack::default()
        .with_pos(PADDING + 5, tabs_y + TAB_HEIGHT + 5)
        .with_size(DIALOG_WIDTH - PADDING * 2 - 30, 0);
    pack_installed_inner.set_type(PackType::Vertical);
    pack_installed_inner.set_spacing(5);

    // Add installed plugins
    let plugin_list = plugins.list_plugins();
    // Track the empty label so we can hide it when a plugin is installed from Available tab
    let empty_label_for_tracking: Rc<RefCell<Option<Frame>>> = Rc::new(RefCell::new(None));
    if plugin_list.is_empty() {
        let mut empty_label = Frame::default()
            .with_size(DIALOG_WIDTH - PADDING * 2 - 30, 40)
            .with_label("No plugins installed");
        empty_label.set_label_color(theme.text_dim);
        *empty_label_for_tracking.borrow_mut() = Some(empty_label);
    } else {
        for plugin in plugin_list {
            let row = create_installed_plugin_row(
                &plugin.name,
                &plugin.version,
                &plugin.description,
                plugin.enabled,
                &theme,
                toggles.clone(),
                uninstalled.clone(),
                available_buttons.clone(),
                installed_rows.clone(),
            );
            // Track installed row for cross-tab sync (hide on update)
            installed_rows
                .borrow_mut()
                .insert(plugin.name.clone(), row.clone());
            pack_installed_inner.add(&row);
        }
    }

    pack_installed_inner.end();
    scroll_installed.end();
    installed_group.end();

    // Wrap pack_installed for cross-tab sharing
    let pack_installed: Rc<RefCell<Pack>> = Rc::new(RefCell::new(pack_installed_inner));

    // Copy the empty label reference for cross-tab sync
    let empty_installed_label = empty_label_for_tracking.clone();

    // ============ AVAILABLE TAB ============
    let mut available_group = Group::default()
        .with_pos(PADDING, tabs_y + TAB_HEIGHT)
        .with_size(DIALOG_WIDTH - PADDING * 2, tabs_height - TAB_HEIGHT)
        .with_label("Available");
    available_group.set_label_color(theme.text);
    available_group.set_color(theme.bg);

    let mut scroll_available = Scroll::default()
        .with_pos(PADDING + 5, tabs_y + TAB_HEIGHT + 5)
        .with_size(DIALOG_WIDTH - PADDING * 2 - 10, tabs_height - TAB_HEIGHT - 10);
    scroll_available.set_color(theme.bg);

    let mut pack_available = Pack::default()
        .with_pos(PADDING + 5, tabs_y + TAB_HEIGHT + 5)
        .with_size(DIALOG_WIDTH - PADDING * 2 - 30, 0);
    pack_available.set_type(PackType::Vertical);
    pack_available.set_spacing(5);

    // Fetch available plugins
    let fetch_result = fetch_plugin_registry();

    match fetch_result {
        Ok(registry) => {
            if registry.plugins.is_empty() {
                let mut empty_label = Frame::default()
                    .with_size(DIALOG_WIDTH - PADDING * 2 - 30, 40)
                    .with_label("No plugins available in registry");
                empty_label.set_label_color(theme.text_dim);
            } else {
                // Get installed plugin versions for update detection
                let installed_versions: Vec<(String, String)> = plugin_list
                    .iter()
                    .map(|p| {
                        (
                            p.name.to_lowercase().replace(' ', "-"),
                            p.version.clone(),
                        )
                    })
                    .collect();

                for plugin_info in &registry.plugins {
                    let dir_name = plugin_info.path.trim_end_matches('/');
                    let already_installed = is_plugin_installed(dir_name);

                    // Check if update available
                    let update_available = installed_versions
                        .iter()
                        .find(|(name, _)| name == dir_name)
                        .is_some_and(|(_, installed_ver)| {
                            is_update_available(installed_ver, &plugin_info.version)
                        });

                    let row = create_available_plugin_row(
                        plugin_info,
                        already_installed,
                        update_available,
                        &theme,
                        installed.clone(),
                        toggles.clone(),
                        uninstalled.clone(),
                        pack_installed.clone(),
                        available_buttons.clone(),
                        empty_installed_label.clone(),
                        installed_rows.clone(),
                    );
                    pack_available.add(&row);
                }
            }
        }
        Err(e) => {
            let error_color = if is_dark {
                Color::from_rgb(255, 120, 120)
            } else {
                Color::from_rgb(180, 0, 0)
            };
            let mut error_label = Frame::default()
                .with_size(DIALOG_WIDTH - PADDING * 2 - 30, 60)
                .with_label(&format!("Failed to fetch plugins:\n{}", e));
            error_label.set_label_color(error_color);
        }
    }

    pack_available.end();
    scroll_available.end();
    available_group.end();

    tabs.end();
    tabs.auto_layout();

    // ============ BUTTONS ============
    let btn_y = DIALOG_HEIGHT - PADDING - BUTTON_HEIGHT;

    let mut reload_btn = Button::default()
        .with_pos(PADDING, btn_y)
        .with_size(100, BUTTON_HEIGHT)
        .with_label("Reload All");
    reload_btn.set_color(theme.button_bg);
    reload_btn.set_label_color(theme.text);

    let mut close_btn = Button::default()
        .with_pos(DIALOG_WIDTH - PADDING - 80, btn_y)
        .with_size(80, BUTTON_HEIGHT)
        .with_label("Close");
    close_btn.set_color(theme.button_bg);
    close_btn.set_label_color(theme.text);

    dialog.end();

    // Set up callbacks
    let result_reload = result.clone();
    let dialog_reload = dialog.clone();
    reload_btn.set_callback(move |_| {
        *result_reload.borrow_mut() = PluginManagerResult::ReloadAll;
        dialog_reload.clone().hide();
    });

    let result_close = result.clone();
    let toggles_close = toggles.clone();
    let installed_close = installed.clone();
    let uninstalled_close = uninstalled.clone();
    let dialog_close = dialog.clone();
    close_btn.set_callback(move |_| {
        let toggled = toggles_close.borrow().clone();
        let inst = installed_close.borrow().clone();
        let uninst = uninstalled_close.borrow().clone();

        if !uninst.is_empty() {
            *result_close.borrow_mut() = PluginManagerResult::UninstalledPlugins(uninst);
        } else if !inst.is_empty() {
            *result_close.borrow_mut() = PluginManagerResult::InstalledPlugins(inst);
        } else if !toggled.is_empty() {
            *result_close.borrow_mut() = PluginManagerResult::ToggledPlugins(toggled);
        } else {
            *result_close.borrow_mut() = PluginManagerResult::Cancelled;
        }
        dialog_close.clone().hide();
    });

    // Window close (X button)
    let result_x = result.clone();
    let toggles_x = toggles.clone();
    let installed_x = installed.clone();
    let uninstalled_x = uninstalled.clone();
    dialog.set_callback(move |w| {
        let toggled = toggles_x.borrow().clone();
        let inst = installed_x.borrow().clone();
        let uninst = uninstalled_x.borrow().clone();

        if !uninst.is_empty() {
            *result_x.borrow_mut() = PluginManagerResult::UninstalledPlugins(uninst);
        } else if !inst.is_empty() {
            *result_x.borrow_mut() = PluginManagerResult::InstalledPlugins(inst);
        } else if !toggled.is_empty() {
            *result_x.borrow_mut() = PluginManagerResult::ToggledPlugins(toggled);
        }
        w.hide();
    });

    dialog.show();
    super::run_dialog(&dialog);

    result.borrow().clone()
}

/// Create a row for an installed plugin (VSCode-style layout)
fn create_installed_plugin_row(
    name: &str,
    version: &str,
    description: &str,
    enabled: bool,
    theme: &DialogTheme,
    toggles: Rc<RefCell<Vec<(String, bool)>>>,
    uninstalled: Rc<RefCell<Vec<String>>>,
    available_buttons: Rc<RefCell<HashMap<String, Button>>>,
    installed_rows: Rc<RefCell<HashMap<String, Group>>>,
) -> Group {
    let row_width = DIALOG_WIDTH - PADDING * 2 - 30;
    let is_dark = theme.is_dark();

    let mut row = Group::default().with_size(row_width, PLUGIN_ROW_HEIGHT);
    row.set_frame(FrameType::FlatBox);
    row.set_color(theme.row_bg);

    // Layout: [checkbox 24] [margin 4] [icon 32] [margin 8] [content...] [button 80] [margin 8]
    let mut x = 4;

    // Enable/disable checkbox
    let mut checkbox = CheckButton::default()
        .with_pos(x, (PLUGIN_ROW_HEIGHT - 20) / 2)
        .with_size(CHECKBOX_WIDTH, 20);
    checkbox.set_value(enabled);
    checkbox.set_color(theme.row_bg);
    x += CHECKBOX_WIDTH + 4;

    // Icon with first letter
    let first_letter = name.chars().next().unwrap_or('?');
    let icon_color = icon_color_for_letter(first_letter, is_dark);
    let mut icon = Frame::default()
        .with_pos(x, (PLUGIN_ROW_HEIGHT - ICON_SIZE) / 2)
        .with_size(ICON_SIZE, ICON_SIZE)
        .with_label(&first_letter.to_uppercase().to_string());
    icon.set_frame(FrameType::FlatBox);
    icon.set_color(icon_color);
    icon.set_label_color(Color::White);
    icon.set_label_font(Font::HelveticaBold);
    icon.set_label_size(16);
    x += ICON_SIZE + ICON_MARGIN;

    // Content area width (excluding button area)
    let content_width = row_width - x - ACTION_BUTTON_WIDTH - ICON_MARGIN;

    // Title: name + version (line 1)
    let mut title = Frame::default()
        .with_pos(x, 6)
        .with_size(content_width, 18)
        .with_label(&format!("{}  v{}", name, version));
    title.set_label_color(theme.text);
    title.set_align(Align::Left | Align::Inside);

    // Description (line 2) - truncated
    let desc_text = truncate_text(description, DESC_MAX_CHARS);
    let mut desc = Frame::default()
        .with_pos(x, 24)
        .with_size(content_width, 16)
        .with_label(&desc_text);
    desc.set_label_color(theme.text_dim);
    desc.set_align(Align::Left | Align::Inside);
    desc.set_label_size(11);

    // Meta line (line 3) - just show verified status for installed
    let mut meta = Frame::default()
        .with_pos(x, 40)
        .with_size(content_width, 14)
        .with_label("Installed");
    meta.set_label_color(theme.text_dim);
    meta.set_align(Align::Left | Align::Inside);
    meta.set_label_size(10);

    // Uninstall button (right side, vertically centered)
    let btn_x = row_width - ACTION_BUTTON_WIDTH - ICON_MARGIN;
    let mut uninstall_btn = Button::default()
        .with_pos(btn_x, (PLUGIN_ROW_HEIGHT - 26) / 2)
        .with_size(ACTION_BUTTON_WIDTH, 26)
        .with_label("Uninstall");
    uninstall_btn.set_label_size(11);
    uninstall_btn.set_color(theme.button_bg);
    // Red-tinted label for destructive action
    uninstall_btn.set_label_color(if is_dark {
        Color::from_rgb(255, 150, 150)
    } else {
        Color::from_rgb(180, 50, 50)
    });

    // Track toggle callback
    let plugin_name = name.to_string();
    let plugin_name_uninstall = name.to_string();
    checkbox.set_callback(move |cb| {
        let new_state = cb.value();
        let mut t = toggles.borrow_mut();
        // Remove any previous toggle for this plugin
        t.retain(|(n, _)| n != &plugin_name);
        t.push((plugin_name.clone(), new_state));
    });

    // Uninstall callback - clone row to hide it after confirmation
    let mut row_to_hide = row.clone();
    uninstall_btn.set_callback(move |_btn| {
        // Confirmation dialog
        let choice = fltk::dialog::choice2_default(
            &format!(
                "Uninstall \"{}\"?\n\nThis will delete the plugin files.",
                plugin_name_uninstall
            ),
            "Cancel",
            "Uninstall",
            "",
        );

        if choice == Some(1) {
            uninstalled.borrow_mut().push(plugin_name_uninstall.clone());
            // Hide the entire row immediately
            row_to_hide.hide();
            // Trigger parent redraw
            if let Some(mut parent) = row_to_hide.parent() {
                parent.redraw();
            }

            // Remove from installed_rows tracking
            installed_rows.borrow_mut().remove(&plugin_name_uninstall);

            // Cross-tab sync: reset the Available tab button to "Install"
            // Note: registry uses lowercase-hyphenated names (e.g., "python-lint")
            // while installed plugins use display names (e.g., "Python Lint")
            let registry_name = plugin_name_uninstall.to_lowercase().replace(' ', "-");
            if let Some(btn) = available_buttons.borrow_mut().get_mut(&registry_name) {
                btn.set_label("Install");
                btn.activate();
            }
        }
    });

    row.end();
    row
}

/// Create a row for an available plugin (VSCode-style layout)
fn create_available_plugin_row(
    plugin_info: &AvailablePluginInfo,
    already_installed: bool,
    update_available: bool,
    theme: &DialogTheme,
    installed: Rc<RefCell<Vec<String>>>,
    toggles: Rc<RefCell<Vec<(String, bool)>>>,
    uninstalled: Rc<RefCell<Vec<String>>>,
    pack_installed: Rc<RefCell<Pack>>,
    available_buttons: Rc<RefCell<HashMap<String, Button>>>,
    empty_installed_label: Rc<RefCell<Option<Frame>>>,
    installed_rows: Rc<RefCell<HashMap<String, Group>>>,
) -> Group {
    let row_width = DIALOG_WIDTH - PADDING * 2 - 30;
    let is_dark = theme.is_dark();

    let mut row = Group::default().with_size(row_width, PLUGIN_ROW_HEIGHT);
    row.set_frame(FrameType::FlatBox);
    row.set_color(theme.row_bg);

    // Layout: [margin 8] [icon 32] [margin 8] [content...] [button 80] [margin 8]
    let mut x = ICON_MARGIN;

    // Icon with first letter
    let first_letter = plugin_info.name.chars().next().unwrap_or('?');
    let icon_color = icon_color_for_letter(first_letter, is_dark);
    let mut icon = Frame::default()
        .with_pos(x, (PLUGIN_ROW_HEIGHT - ICON_SIZE) / 2)
        .with_size(ICON_SIZE, ICON_SIZE)
        .with_label(&first_letter.to_uppercase().to_string());
    icon.set_frame(FrameType::FlatBox);
    icon.set_color(icon_color);
    icon.set_label_color(Color::White);
    icon.set_label_font(Font::HelveticaBold);
    icon.set_label_size(16);
    x += ICON_SIZE + ICON_MARGIN;

    // Content area width (excluding button area)
    let content_width = row_width - x - ACTION_BUTTON_WIDTH - ICON_MARGIN;

    // Title: name + version (line 1)
    let mut title = Frame::default()
        .with_pos(x, 6)
        .with_size(content_width, 18)
        .with_label(&format!("{}  v{}", plugin_info.name, plugin_info.version));
    title.set_label_color(theme.text);
    title.set_align(Align::Left | Align::Inside);

    // Description (line 2) - truncated
    let desc_text = truncate_text(&plugin_info.description, DESC_MAX_CHARS);
    let mut desc = Frame::default()
        .with_pos(x, 24)
        .with_size(content_width, 16)
        .with_label(&desc_text);
    desc.set_label_color(theme.text_dim);
    desc.set_align(Align::Left | Align::Inside);
    desc.set_label_size(11);

    // Meta line (line 3): author + tags + verification badge
    let is_verified = plugin_info.is_verified();
    let badge_text = if is_verified {
        "\u{2713} Verified"
    } else {
        "\u{26A0} Unverified"
    };
    let meta_text = if plugin_info.author.is_empty() {
        format!("{} \u{00B7} {}", plugin_info.tags.join(", "), badge_text)
    } else {
        format!(
            "by {} \u{00B7} {} \u{00B7} {}",
            plugin_info.author,
            plugin_info.tags.join(", "),
            badge_text
        )
    };
    let mut meta = Frame::default()
        .with_pos(x, 40)
        .with_size(content_width, 14)
        .with_label(&meta_text);
    // Color the meta line based on verification
    let meta_color = if is_verified {
        if is_dark {
            Color::from_rgb(100, 180, 100) // Green-ish
        } else {
            Color::from_rgb(50, 120, 50)
        }
    } else if is_dark {
        Color::from_rgb(200, 160, 80) // Yellow-ish
    } else {
        Color::from_rgb(160, 120, 40)
    };
    meta.set_label_color(meta_color);
    meta.set_align(Align::Left | Align::Inside);
    meta.set_label_size(10);

    // Install/Update button (right side, vertically centered)
    let btn_x = row_width - ACTION_BUTTON_WIDTH - ICON_MARGIN;
    let mut install_btn = Button::default()
        .with_pos(btn_x, (PLUGIN_ROW_HEIGHT - 26) / 2)
        .with_size(ACTION_BUTTON_WIDTH, 26);
    install_btn.set_label_size(11);
    install_btn.set_color(theme.button_bg);
    install_btn.set_label_color(theme.text);

    if already_installed && !update_available {
        install_btn.set_label("Installed");
        install_btn.deactivate();
    } else if update_available {
        install_btn.set_label("Update");
        // Green accent for update
        install_btn.set_label_color(if is_dark {
            Color::from_rgb(100, 200, 100)
        } else {
            Color::from_rgb(40, 140, 40)
        });
    } else {
        install_btn.set_label("Install");
    }

    // Store button reference for cross-tab sync (uninstall -> re-enable Install button)
    available_buttons
        .borrow_mut()
        .insert(plugin_info.name.clone(), install_btn.clone());

    // Capture theme values for callback
    let theme_clone = *theme;

    // Install callback
    let info = plugin_info.clone();
    let installed_rc = installed.clone();
    install_btn.set_callback(move |btn| {
        btn.set_label("...");
        btn.deactivate();
        fltk::app::awake(); // Force UI update

        match install_plugin(&info) {
            Ok(status) => {
                use crate::app::services::plugin_verify::VerificationStatus;
                match status {
                    VerificationStatus::Verified | VerificationStatus::Unverified => {
                        btn.set_label("Done!");
                        installed_rc.borrow_mut().push(info.name.clone());

                        // Hide "No plugins installed" label if present
                        if let Some(ref mut label) = *empty_installed_label.borrow_mut() {
                            label.hide();
                        }

                        // Cross-tab sync: hide old row if this is an update
                        if let Some(old_row) = installed_rows.borrow_mut().remove(&info.name) {
                            let mut old_row = old_row;
                            old_row.hide();
                            if let Some(mut parent) = old_row.parent() {
                                parent.redraw();
                            }
                        }

                        // Cross-tab sync: add row to Installed tab
                        let new_row = create_installed_plugin_row(
                            &info.name,
                            &info.version,
                            &info.description,
                            true, // enabled by default
                            &theme_clone,
                            toggles.clone(),
                            uninstalled.clone(),
                            available_buttons.clone(),
                            installed_rows.clone(),
                        );
                        // Track the new row for future updates
                        installed_rows
                            .borrow_mut()
                            .insert(info.name.clone(), new_row.clone());
                        pack_installed.borrow_mut().add(&new_row);
                        pack_installed.borrow_mut().redraw();
                    }
                    VerificationStatus::Invalid(reason) => {
                        btn.set_label("Failed");
                        btn.activate();
                        fltk::dialog::alert_default(&format!(
                            "Signature verification failed for {}:\n{}",
                            info.name, reason
                        ));
                    }
                }
            }
            Err(e) => {
                btn.set_label("Error");
                btn.activate();
                eprintln!("[plugins] Install error: {}", e);
                fltk::dialog::alert_default(&format!("Failed to install {}:\n{}", info.name, e));
            }
        }
    });

    row.end();
    row
}
