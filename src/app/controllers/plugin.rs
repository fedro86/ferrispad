use std::cell::RefCell;
use std::rc::Rc;

use fltk::{app::Sender, menu::MenuBar};

use crate::app::domain::messages::Message;
use crate::app::domain::settings::AppSettings;
use crate::app::plugins::{get_plugin_dir, PluginManager, WidgetManager};
use crate::app::services::shortcut_registry::ShortcutRegistry;
use crate::ui::dialogs::plugin_manager::{show_plugin_manager_dialog, PluginManagerResult};
use crate::ui::dialogs::plugin_permissions::{show_permission_dialog, ApprovalResult, PermissionRequest};

/// Manages plugin lifecycle operations (toggle, reload, permissions, config dialogs).
///
/// Stateless — all plugin state lives in `PluginManager`.
/// Holds cheap FLTK widget clones for menu rebuilds.
pub struct PluginController {
    menu: MenuBar,
    sender: Sender<Message>,
}

impl PluginController {
    pub fn new(menu: MenuBar, sender: Sender<Message>) -> Self {
        Self { menu, sender }
    }

    /// Rebuild the plugins menu bar from current plugin state.
    fn rebuild_plugins_menu(
        &mut self,
        settings: &Rc<RefCell<AppSettings>>,
        plugins: &PluginManager,
        shortcut_registry: &ShortcutRegistry,
    ) {
        crate::ui::menu::rebuild_plugins_menu(
            &mut self.menu,
            &self.sender,
            &settings.borrow(),
            plugins,
            shortcut_registry,
        );
    }

    /// Check plugin permissions and show approval dialog for unapproved commands.
    pub fn check_permissions(
        plugins: &mut PluginManager,
        settings: &Rc<RefCell<AppSettings>>,
    ) {
        for plugin in plugins.plugins_mut() {
            let unapproved: Vec<String> = plugin
                .permissions
                .execute
                .iter()
                .filter(|cmd| !plugin.approved_commands.contains(cmd))
                .cloned()
                .collect();

            if unapproved.is_empty() {
                continue;
            }

            let request = PermissionRequest {
                plugin_name: plugin.name.clone(),
                description: plugin.description.clone(),
                commands: unapproved,
            };

            match show_permission_dialog(&request) {
                ApprovalResult::Approved(cmds) => {
                    plugin.approved_commands.extend(cmds.clone());
                    {
                        let mut s = settings.borrow_mut();
                        let approvals = s
                            .plugin_approvals
                            .entry(plugin.name.clone())
                            .or_default();
                        for cmd in cmds {
                            if !approvals.approved_commands.contains(&cmd) {
                                approvals.approved_commands.push(cmd);
                            }
                        }
                    }
                    if let Err(e) = settings.borrow().save() {
                        eprintln!("[plugins] Failed to save permission approvals: {}", e);
                    }
                }
                ApprovalResult::Denied => {
                    plugin.enabled = false;
                    eprintln!(
                        "[plugins] {} disabled: user denied permissions",
                        plugin.name
                    );
                }
                ApprovalResult::Cancelled => {
                    eprintln!(
                        "[plugins] {} permission dialog cancelled - running with limited permissions",
                        plugin.name
                    );
                }
            }
        }
    }

    /// Check plugin permissions (deferred until after UI is ready).
    pub fn check_permissions_deferred(
        plugins: &mut PluginManager,
        settings: &Rc<RefCell<AppSettings>>,
    ) {
        Self::check_permissions(plugins, settings);
    }

    /// Toggle the global plugin system on/off
    pub fn handle_toggle_global(
        &mut self,
        plugins: &mut PluginManager,
        settings: &Rc<RefCell<AppSettings>>,
        shortcut_registry: &ShortcutRegistry,
        widget_manager: &mut WidgetManager,
    ) {
        let currently_enabled = settings.borrow().plugins_enabled;
        let new_enabled = !currently_enabled;

        {
            let mut s = settings.borrow_mut();
            s.plugins_enabled = new_enabled;
            let _ = s.save();
        }

        // Clean up all widget sessions when disabling plugins globally
        if !new_enabled {
            for plugin in plugins.list_plugins() {
                widget_manager.clear_plugin_sessions(&plugin.name);
            }
        }

        plugins.set_enabled(new_enabled);
        if new_enabled {
            plugins.reload_all(&get_plugin_dir());
            let disabled = settings.borrow().disabled_plugins.clone();
            for name in &disabled {
                plugins.toggle_plugin(name, false);
            }
            Self::check_permissions(plugins, settings);
        }

        self.rebuild_plugins_menu(settings, plugins, shortcut_registry);
    }

    /// Toggle a specific plugin on/off
    pub fn handle_toggle(
        &mut self,
        plugins: &mut PluginManager,
        settings: &Rc<RefCell<AppSettings>>,
        shortcut_registry: &ShortcutRegistry,
        name: String,
        widget_manager: &mut WidgetManager,
    ) {
        let was_enabled = plugins
            .list_plugins()
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.enabled)
            .unwrap_or(false);

        plugins.toggle_plugin(&name, !was_enabled);

        // Clean up widget sessions when disabling a plugin
        if was_enabled {
            widget_manager.clear_plugin_sessions(&name);
        }

        {
            let mut s = settings.borrow_mut();
            let disabled = plugins.disabled_plugin_names();
            s.disabled_plugins = disabled;
            let _ = s.save();
        }

        self.rebuild_plugins_menu(settings, plugins, shortcut_registry);
    }

    /// Reload all plugins from disk
    pub fn handle_reload(
        &mut self,
        plugins: &mut PluginManager,
        settings: &Rc<RefCell<AppSettings>>,
        shortcut_registry: &ShortcutRegistry,
        widget_manager: &mut WidgetManager,
    ) {
        // Clean up all widget sessions before reload
        for plugin in plugins.list_plugins() {
            widget_manager.clear_plugin_sessions(&plugin.name);
        }

        plugins.reload_all(&get_plugin_dir());

        let disabled = settings.borrow().disabled_plugins.clone();
        for name in &disabled {
            plugins.toggle_plugin(name, false);
        }

        Self::check_permissions(plugins, settings);
        self.rebuild_plugins_menu(settings, plugins, shortcut_registry);
    }

    /// Show the plugin manager dialog and process results
    pub fn show_manager(
        &mut self,
        plugins: &mut PluginManager,
        settings: &Rc<RefCell<AppSettings>>,
        shortcut_registry: &ShortcutRegistry,
        theme_bg: (u8, u8, u8),
    ) {
        let result = show_plugin_manager_dialog(plugins, theme_bg);

        match result {
            PluginManagerResult::ToggledPlugins(toggles) => {
                for (name, enabled) in toggles {
                    plugins.toggle_plugin(&name, enabled);
                }
                {
                    let mut s = settings.borrow_mut();
                    s.disabled_plugins = plugins.disabled_plugin_names();
                    let _ = s.save();
                }
                self.rebuild_plugins_menu(settings, plugins, shortcut_registry);
            }
            PluginManagerResult::ReloadAll => {
                self.sender.send(Message::PluginsReloadAll);
            }
            PluginManagerResult::InstalledPlugins(names) => {
                self.sender.send(Message::PluginsReloadAll);
                eprintln!("[plugins] Installed plugins: {}", names.join(", "));
            }
            PluginManagerResult::UninstalledPlugins(names) => {
                use std::fs;

                let plugins_dir = crate::app::plugins::loader::get_plugin_dir();
                let mut errors = Vec::new();

                for name in &names {
                    let dir_name = name.to_lowercase().replace(' ', "-");
                    let plugin_path = plugins_dir.join(&dir_name);

                    if plugin_path.exists() {
                        if let Err(e) = fs::remove_dir_all(&plugin_path) {
                            errors.push(format!("{}: {}", name, e));
                        } else {
                            eprintln!("[plugins] Uninstalled: {}", name);
                        }
                    }
                }

                if !errors.is_empty() {
                    fltk::dialog::alert_default(&format!(
                        "Failed to uninstall some plugins:\n{}",
                        errors.join("\n")
                    ));
                }

                plugins.reload_all(&get_plugin_dir());
                let disabled = settings.borrow().disabled_plugins.clone();
                for disabled_name in &disabled {
                    plugins.toggle_plugin(disabled_name, false);
                }

                crate::ui::menu::rebuild_plugins_menu_with_orphans(
                    &mut self.menu,
                    &self.sender,
                    &settings.borrow(),
                    plugins,
                    &names,
                    shortcut_registry,
                );
            }
            PluginManagerResult::Cancelled => {}
        }
    }

    /// Show the plugin settings dialog (Run All Checks configuration)
    pub fn show_settings(
        &mut self,
        plugins: &PluginManager,
        settings: &Rc<RefCell<AppSettings>>,
        shortcut_registry: &ShortcutRegistry,
        theme_bg: (u8, u8, u8),
    ) {
        use crate::ui::dialogs::plugin_settings::show_plugin_settings_dialog;

        let available_plugins: Vec<(String, bool)> = plugins
            .list_plugins()
            .iter()
            .map(|p| (p.name.clone(), p.enabled))
            .collect();

        if let Some(result) = show_plugin_settings_dialog(
            &settings.borrow(),
            &available_plugins,
            theme_bg,
        ) {
            {
                let mut s = settings.borrow_mut();
                s.run_all_checks_plugins = result.run_all_checks_plugins;
                s.run_all_checks_shortcut = result.run_all_checks_shortcut;
                let _ = s.save();
            }
            self.rebuild_plugins_menu(settings, plugins, shortcut_registry);
        }
    }

    /// Show per-plugin configuration dialog
    pub fn show_config(
        &mut self,
        plugins: &mut PluginManager,
        settings: &Rc<RefCell<AppSettings>>,
        shortcut_registry: &ShortcutRegistry,
        plugin_name: &str,
        theme_bg: (u8, u8, u8),
    ) {
        use crate::app::domain::settings::PluginConfig;
        use crate::ui::dialogs::plugin_config::show_plugin_config_dialog;

        let plugin = plugins.list_plugins().iter().find(|p| p.name == plugin_name);

        let Some(plugin) = plugin else {
            eprintln!("[plugins] Plugin not found: {}", plugin_name);
            return;
        };

        let current_config = settings
            .borrow()
            .plugin_configs
            .get(plugin_name)
            .cloned()
            .unwrap_or_default();

        let config_schema = plugin.config_schema.clone();
        if let Some(result) = show_plugin_config_dialog(
            plugin_name,
            &config_schema.params,
            &current_config,
            theme_bg,
        ) {
            let new_config = PluginConfig {
                params: result.params.clone(),
            };
            {
                let mut s = settings.borrow_mut();
                s.plugin_configs
                    .insert(plugin_name.to_string(), new_config);
                let _ = s.save();
            }
            plugins.set_plugin_config(plugin_name, result.params);
            self.rebuild_plugins_menu(settings, plugins, shortcut_registry);
        }
    }

    /// Trigger a background check for plugin updates
    pub fn check_updates(sender: &Sender<Message>) {
        use crate::app::services::plugin_update_checker::check_for_plugin_updates;

        let sender = sender.clone();
        std::thread::spawn(move || {
            match check_for_plugin_updates() {
                Ok(updates) => {
                    sender.send(Message::PluginUpdatesChecked(updates));
                }
                Err(e) => {
                    eprintln!("[plugin-update-checker] Error: {}", e);
                    sender.send(Message::PluginUpdatesChecked(Vec::new()));
                }
            }
        });
    }

    /// Handle the result of a plugin update check
    pub fn handle_updates_checked(
        settings: &Rc<RefCell<AppSettings>>,
        updates: &[crate::app::services::plugin_update_checker::PluginUpdateInfo],
    ) {
        {
            let mut s = settings.borrow_mut();
            s.last_plugin_update_check =
                crate::app::services::plugin_update_checker::current_timestamp();
            let _ = s.save();
        }

        if updates.is_empty() {
            eprintln!("[plugin-update-checker] All plugins are up to date");
        } else {
            eprintln!(
                "[plugin-update-checker] {} plugin update(s) available:",
                updates.len()
            );
            for update in updates {
                eprintln!(
                    "  - {} ({} -> {})",
                    update.plugin_name, update.installed_version, update.available_version
                );
            }
        }
    }
}
