//! Plugin Manager dialog for viewing, enabling/disabling, and installing plugins.

use fltk::{
    button::{Button, CheckButton},
    enums::{Align, Color, FrameType},
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

// Layout constants
const DIALOG_WIDTH: i32 = 550;
const DIALOG_HEIGHT: i32 = 480;
const PADDING: i32 = 10;
const TAB_HEIGHT: i32 = 30;
const BUTTON_HEIGHT: i32 = 30;
const PLUGIN_ROW_HEIGHT: i32 = 70;

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

/// Show the plugin manager dialog
///
/// Returns the result indicating what actions were taken
pub fn show_plugin_manager_dialog(
    plugins: &PluginManager,
    is_dark: bool,
) -> PluginManagerResult {
    let mut dialog = Window::default()
        .with_size(DIALOG_WIDTH, DIALOG_HEIGHT)
        .with_label("Plugin Manager")
        .center_screen();
    dialog.make_modal(true);

    // Apply dark theme if needed
    let bg_color = if is_dark {
        Color::from_rgb(45, 45, 45)
    } else {
        Color::from_rgb(245, 245, 245)
    };
    let text_color = if is_dark {
        Color::from_rgb(220, 220, 220)
    } else {
        Color::from_rgb(30, 30, 30)
    };
    let row_bg = if is_dark {
        Color::from_rgb(55, 55, 55)
    } else {
        Color::from_rgb(255, 255, 255)
    };

    dialog.set_color(bg_color);

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

    // ============ INSTALLED TAB ============
    let mut installed_group = Group::default()
        .with_pos(PADDING, tabs_y + TAB_HEIGHT)
        .with_size(DIALOG_WIDTH - PADDING * 2, tabs_height - TAB_HEIGHT)
        .with_label("Installed");
    installed_group.set_label_color(text_color);

    let scroll_installed = Scroll::default()
        .with_pos(PADDING + 5, tabs_y + TAB_HEIGHT + 5)
        .with_size(DIALOG_WIDTH - PADDING * 2 - 10, tabs_height - TAB_HEIGHT - 10);

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
        empty_label.set_label_color(text_color);
        *empty_label_for_tracking.borrow_mut() = Some(empty_label);
    } else {
        for plugin in plugin_list {
            let row = create_installed_plugin_row(
                &plugin.name,
                &plugin.version,
                &plugin.description,
                plugin.enabled,
                row_bg,
                text_color,
                is_dark,
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
    available_group.set_label_color(text_color);

    let scroll_available = Scroll::default()
        .with_pos(PADDING + 5, tabs_y + TAB_HEIGHT + 5)
        .with_size(DIALOG_WIDTH - PADDING * 2 - 10, tabs_height - TAB_HEIGHT - 10);

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
                empty_label.set_label_color(text_color);
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
                        row_bg,
                        text_color,
                        is_dark,
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
            let mut error_label = Frame::default()
                .with_size(DIALOG_WIDTH - PADDING * 2 - 30, 60)
                .with_label(&format!("Failed to fetch plugins:\n{}", e));
            error_label.set_label_color(if is_dark {
                Color::from_rgb(255, 120, 120)
            } else {
                Color::from_rgb(180, 0, 0)
            });
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

    let mut close_btn = Button::default()
        .with_pos(DIALOG_WIDTH - PADDING - 80, btn_y)
        .with_size(80, BUTTON_HEIGHT)
        .with_label("Close");

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

/// Create a row for an installed plugin
fn create_installed_plugin_row(
    name: &str,
    version: &str,
    description: &str,
    enabled: bool,
    row_bg: Color,
    text_color: Color,
    is_dark: bool,
    toggles: Rc<RefCell<Vec<(String, bool)>>>,
    uninstalled: Rc<RefCell<Vec<String>>>,
    available_buttons: Rc<RefCell<HashMap<String, Button>>>,
    installed_rows: Rc<RefCell<HashMap<String, Group>>>,
) -> Group {
    let row_width = DIALOG_WIDTH - PADDING * 2 - 30;
    let mut row = Group::default().with_size(row_width, PLUGIN_ROW_HEIGHT);
    row.set_frame(FrameType::FlatBox);
    row.set_color(row_bg);

    // Checkbox for enable/disable (narrower to make room for uninstall button)
    let mut checkbox = CheckButton::default()
        .with_pos(10, 10)
        .with_size(row_width - 100, 24)
        .with_label(&format!("{}  v{}", name, version));
    checkbox.set_value(enabled);
    checkbox.set_label_color(text_color);

    // Uninstall button (right side)
    let mut uninstall_btn = Button::default()
        .with_pos(row_width - 80, 10)
        .with_size(70, 24)
        .with_label("Uninstall");
    uninstall_btn.set_label_size(11);

    // Set red-ish color for destructive action
    if is_dark {
        uninstall_btn.set_color(Color::from_rgb(120, 50, 50));
        uninstall_btn.set_label_color(Color::from_rgb(255, 200, 200));
    } else {
        uninstall_btn.set_color(Color::from_rgb(255, 220, 220));
        uninstall_btn.set_label_color(Color::from_rgb(140, 30, 30));
    }

    // Description
    let mut desc = Frame::default()
        .with_pos(30, 36)
        .with_size(row_width - 40, 24)
        .with_label(description);
    desc.set_label_color(if is_dark {
        Color::from_rgb(160, 160, 160)
    } else {
        Color::from_rgb(100, 100, 100)
    });
    desc.set_align(Align::Left | Align::Inside);

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

/// Create a row for an available plugin
fn create_available_plugin_row(
    plugin_info: &AvailablePluginInfo,
    already_installed: bool,
    update_available: bool,
    row_bg: Color,
    text_color: Color,
    is_dark: bool,
    installed: Rc<RefCell<Vec<String>>>,
    toggles: Rc<RefCell<Vec<(String, bool)>>>,
    uninstalled: Rc<RefCell<Vec<String>>>,
    pack_installed: Rc<RefCell<Pack>>,
    available_buttons: Rc<RefCell<HashMap<String, Button>>>,
    empty_installed_label: Rc<RefCell<Option<Frame>>>,
    installed_rows: Rc<RefCell<HashMap<String, Group>>>,
) -> Group {
    let row_width = DIALOG_WIDTH - PADDING * 2 - 30;
    let mut row = Group::default().with_size(row_width, PLUGIN_ROW_HEIGHT);
    row.set_frame(FrameType::FlatBox);
    row.set_color(row_bg);

    // Verification badge
    let is_verified = plugin_info.is_verified();
    let badge_text = if is_verified { "Verified" } else { "Unverified" };
    let badge_color = if is_verified {
        Color::from_rgb(60, 160, 60) // Green
    } else {
        Color::from_rgb(180, 140, 0) // Yellow/orange
    };

    let mut badge = Frame::default()
        .with_pos(row_width - 180, 6)
        .with_size(70, 16)
        .with_label(badge_text);
    badge.set_label_color(badge_color);
    badge.set_label_size(10);
    badge.set_align(Align::Right | Align::Inside);

    // Name and version
    let mut title = Frame::default()
        .with_pos(10, 8)
        .with_size(row_width - 200, 22)
        .with_label(&format!("{}  v{}", plugin_info.name, plugin_info.version));
    title.set_label_color(text_color);
    title.set_align(Align::Left | Align::Inside);

    // Description
    let mut desc = Frame::default()
        .with_pos(10, 30)
        .with_size(row_width - 110, 20)
        .with_label(&plugin_info.description);
    desc.set_label_color(if is_dark {
        Color::from_rgb(160, 160, 160)
    } else {
        Color::from_rgb(100, 100, 100)
    });
    desc.set_align(Align::Left | Align::Inside);

    // Author and tags on the same line
    let author_tags = if plugin_info.author.is_empty() {
        format!("Tags: {}", plugin_info.tags.join(", "))
    } else {
        format!("by {} · Tags: {}", plugin_info.author, plugin_info.tags.join(", "))
    };
    let mut meta = Frame::default()
        .with_pos(10, 50)
        .with_size(row_width - 110, 16)
        .with_label(&author_tags);
    meta.set_label_color(if is_dark {
        Color::from_rgb(130, 130, 130)
    } else {
        Color::from_rgb(120, 120, 120)
    });
    meta.set_align(Align::Left | Align::Inside);
    meta.set_label_size(11);

    // Install/Update button
    let mut install_btn = Button::default()
        .with_pos(row_width - 90, 20)
        .with_size(80, 28);

    if already_installed && !update_available {
        install_btn.set_label("Installed");
        install_btn.deactivate();
    } else if update_available {
        install_btn.set_label("Update");
        install_btn.set_color(Color::from_rgb(80, 160, 80));
    } else {
        install_btn.set_label("Install");
    }

    // Store button reference for cross-tab sync (uninstall -> re-enable Install button)
    available_buttons
        .borrow_mut()
        .insert(plugin_info.name.clone(), install_btn.clone());

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
                            row_bg,
                            text_color,
                            is_dark,
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
