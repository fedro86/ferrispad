//! Plugin system for FerrisPad.
//!
//! Provides a Lua-based plugin system that allows users to extend
//! the editor with custom functionality. Plugins are loaded from
//! ~/.config/ferrispad/plugins/ and can hook into various editor events.
//!
//! ## Philosophy Compliance
//! - **0% CPU when idle**: Hooks only fire on user actions
//! - **Event-driven**: All hooks are reactive (open, save, close, edit)
//! - **Single binary**: Lua is statically linked via mlua vendored feature
//! - **Passive aids**: Format on save OK; background indexing NOT OK

pub mod annotations;
pub mod api;
pub mod diff;
mod hook_dispatch;
mod hook_result_parser;
pub mod hooks;
pub mod loader;
pub mod runtime;
pub mod security;
pub mod widgets;

use std::collections::HashMap;
use std::path::PathBuf;

use mlua::Table;

pub use annotations::{AnnotationColor, GutterMark, InlineHighlight, LineAnnotation};
pub use hooks::{Diagnostic, DiagnosticLevel, HookResult, PluginHook, WidgetActionData};
pub use loader::{ConfigParamDef, PluginConfigDef, PluginMenuItem, get_plugin_dir};
pub use widgets::{SplitViewRequest, TerminalViewRequest, TreeViewRequest, WidgetManager};
// Re-export widget types for public API (may not be used internally yet)
#[allow(unused_imports)]
pub use widgets::{
    HighlightColor, IntralineSpan, LineHighlight, SplitPane, SplitViewAction, TreeNode,
};

use loader::{PluginPermissions, discover_plugins, load_plugin_toml};
use runtime::LuaRuntime;

/// Convert a plugin directory name (e.g. "yaml-json-viewer") into a display name
/// (e.g. "Yaml Json Viewer") by title-casing each dash-separated word.
pub fn plugin_display_name(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A loaded plugin instance
pub struct LoadedPlugin {
    /// Plugin name (from init.lua or plugin.toml)
    pub name: String,

    /// Plugin version
    pub version: String,

    /// Plugin description (shown in UI)
    pub description: String,

    /// Path to the plugin directory
    pub path: PathBuf,

    /// Whether this plugin is currently enabled
    pub enabled: bool,

    /// Permissions declared in the plugin manifest
    pub permissions: PluginPermissions,

    /// Commands the user has approved for this plugin
    pub approved_commands: Vec<String>,

    /// Custom menu items registered by this plugin
    pub menu_items: Vec<PluginMenuItem>,

    /// Configuration schema from plugin.toml (what params are configurable)
    pub config_schema: PluginConfigDef,

    /// User configuration values (from AppSettings.plugin_configs)
    pub config_params: HashMap<String, String>,

    /// The Lua table returned by init.lua
    pub(crate) table: Table,
}

/// Plugin manager - coordinates plugin loading and hook dispatch
pub struct PluginManager {
    /// Lua runtime (None if plugins are globally disabled)
    runtime: Option<LuaRuntime>,

    /// Loaded plugins
    plugins: Vec<LoadedPlugin>,

    /// Whether the plugin system is globally enabled
    enabled: bool,
}

impl PluginManager {
    /// Create a new plugin manager.
    /// If `enabled` is false, no Lua runtime is created and no plugins are loaded.
    pub fn new(enabled: bool) -> Self {
        let runtime = if enabled {
            match LuaRuntime::new() {
                Ok(r) => Some(r),
                Err(e) => {
                    eprintln!("[plugins] Failed to create Lua runtime: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            runtime,
            plugins: Vec::new(),
            enabled,
        }
    }

    /// Create a disabled plugin manager (no runtime, no plugins)
    #[allow(dead_code)] // Reserved for fallback error handling
    pub fn disabled() -> Self {
        Self {
            runtime: None,
            plugins: Vec::new(),
            enabled: false,
        }
    }

    /// Check if the plugin system is enabled
    #[allow(dead_code)] // Reserved for future UI
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.runtime.is_some()
    }

    /// Load all plugins from a directory
    pub fn load_plugins(&mut self, dir: &std::path::Path) {
        let runtime = match &self.runtime {
            Some(r) => r,
            None => return,
        };

        let plugin_dirs = discover_plugins(dir);

        for plugin_path in plugin_dirs {
            match self.load_single_plugin(runtime, &plugin_path) {
                Ok(plugin) => {
                    eprintln!("[plugins] Loaded: {} v{}", plugin.name, plugin.version);
                    self.plugins.push(plugin);
                }
                Err(e) => {
                    eprintln!("[plugins] Failed to load {}: {}", plugin_path.display(), e);
                }
            }
        }
    }

    /// Load a single plugin from a directory
    fn load_single_plugin(
        &self,
        runtime: &LuaRuntime,
        plugin_path: &std::path::Path,
    ) -> Result<LoadedPlugin, String> {
        let init_lua = plugin_path.join("init.lua");

        // Load the Lua script
        let table = runtime
            .load_script(&init_lua)
            .map_err(|e| format!("Lua error: {}", e))?;

        // Try to get metadata from plugin.toml first, then fall back to Lua table
        let toml_meta = load_plugin_toml(plugin_path);

        // Get name from table or toml or directory name
        let name = if let Some(ref meta) = toml_meta {
            if !meta.name.is_empty() {
                meta.name.clone()
            } else {
                get_lua_string(&table, "name").unwrap_or_else(|| dir_name(plugin_path))
            }
        } else {
            get_lua_string(&table, "name").unwrap_or_else(|| dir_name(plugin_path))
        };

        // Get version
        let version = if let Some(ref meta) = toml_meta {
            if !meta.version.is_empty() {
                meta.version.clone()
            } else {
                get_lua_string(&table, "version").unwrap_or_else(|| "0.0.0".to_string())
            }
        } else {
            get_lua_string(&table, "version").unwrap_or_else(|| "0.0.0".to_string())
        };

        // Get description
        let description = if let Some(ref meta) = toml_meta {
            if !meta.description.is_empty() {
                meta.description.clone()
            } else {
                get_lua_string(&table, "description").unwrap_or_default()
            }
        } else {
            get_lua_string(&table, "description").unwrap_or_default()
        };

        // Get permissions from manifest (defaults to empty if no manifest)
        let permissions = toml_meta
            .as_ref()
            .map(|m| m.permissions.clone())
            .unwrap_or_default();

        // Get menu items from manifest (defaults to empty if no manifest)
        let menu_items = toml_meta
            .as_ref()
            .map(|m| m.menu_items.clone())
            .unwrap_or_default();

        // Get config schema from manifest (defaults to empty if no manifest)
        let config_schema = toml_meta
            .as_ref()
            .map(|m| m.config.clone())
            .unwrap_or_default();

        Ok(LoadedPlugin {
            name,
            version,
            description,
            path: plugin_path.to_path_buf(),
            enabled: true,
            permissions,
            approved_commands: Vec::new(), // Will be populated from settings
            menu_items,
            config_schema,
            config_params: HashMap::new(), // Will be populated from settings
            table,
        })
    }

    // ── Hook dispatch (delegates to hook_dispatch module) ──

    /// Call a hook on a specific plugin by name.
    /// Returns None if the plugin is not found or not enabled.
    pub fn call_hook_on_plugin(&self, plugin_name: &str, hook: PluginHook) -> Option<HookResult> {
        if !self.enabled {
            return None;
        }
        let runtime = self.runtime.as_ref()?;
        hook_dispatch::call_hook_on_plugin(runtime, &self.plugins, plugin_name, hook)
    }

    /// Call a hook on all enabled plugins
    pub fn call_hook(&self, hook: PluginHook) -> HookResult {
        if !self.enabled {
            return HookResult::default();
        }
        match &self.runtime {
            Some(r) => hook_dispatch::call_hook(r, &self.plugins, hook),
            None => HookResult::default(),
        }
    }

    // ── Plugin access / configuration / lifecycle ──

    /// Get a list of all loaded plugins
    pub fn list_plugins(&self) -> &[LoadedPlugin] {
        &self.plugins
    }

    /// Get mutable access to all loaded plugins (for permission management)
    pub fn plugins_mut(&mut self) -> &mut Vec<LoadedPlugin> {
        &mut self.plugins
    }

    /// Toggle a specific plugin on/off by name
    pub fn toggle_plugin(&mut self, name: &str, enabled: bool) {
        for plugin in &mut self.plugins {
            if plugin.name == name {
                plugin.enabled = enabled;
                break;
            }
        }
    }

    /// Get names of disabled plugins
    pub fn disabled_plugin_names(&self) -> Vec<String> {
        self.plugins
            .iter()
            .filter(|p| !p.enabled)
            .map(|p| p.name.clone())
            .collect()
    }

    /// Set configuration parameters for a plugin by name.
    /// Called from AppState when loading settings.
    pub fn set_plugin_config(&mut self, name: &str, params: HashMap<String, String>) {
        for plugin in &mut self.plugins {
            if plugin.name == name {
                plugin.config_params = params;
                break;
            }
        }
    }

    /// Get current Lua memory usage in bytes.
    /// Returns 0 if the plugin system is disabled.
    pub fn lua_memory_usage(&self) -> usize {
        self.runtime.as_ref().map(|r| r.used_memory()).unwrap_or(0)
    }

    /// Clear all plugins and trigger Lua garbage collection.
    /// This ensures memory is properly reclaimed when plugins are unloaded.
    fn clear_plugins(&mut self) {
        // Drop all Table references first
        self.plugins.clear();

        // Trigger Lua GC to reclaim memory from dropped Tables
        if let Some(ref runtime) = self.runtime {
            runtime.collect_garbage();
        }
    }

    /// Reload all plugins from disk
    pub fn reload_all(&mut self, dir: &std::path::Path) {
        // Remember which plugins were disabled
        let disabled: Vec<String> = self.disabled_plugin_names();
        // Remember approved commands for each plugin
        let approved: Vec<(String, Vec<String>)> = self
            .plugins
            .iter()
            .map(|p| (p.name.clone(), p.approved_commands.clone()))
            .collect();
        // Remember config params for each plugin
        let configs: Vec<(String, HashMap<String, String>)> = self
            .plugins
            .iter()
            .map(|p| (p.name.clone(), p.config_params.clone()))
            .collect();

        // Log memory before reload (debug aid)
        let mem_before = self.lua_memory_usage();

        // Clear with explicit GC to reclaim memory
        self.clear_plugins();
        self.load_plugins(dir);

        // Restore disabled state
        for name in disabled {
            self.toggle_plugin(&name, false);
        }

        // Restore approved commands
        for (name, commands) in approved {
            for plugin in &mut self.plugins {
                if plugin.name == name {
                    plugin.approved_commands = commands;
                    break;
                }
            }
        }

        // Restore config params
        for (name, params) in configs {
            self.set_plugin_config(&name, params);
        }

        // Log memory after reload
        let mem_after = self.lua_memory_usage();
        eprintln!(
            "[plugins] Reloaded. Memory: {} KB -> {} KB",
            mem_before / 1024,
            mem_after / 1024
        );

        // Call init hook on all enabled plugins
        self.call_hook(PluginHook::Init {
            project_root: crate::app::mcp::cwd_as_string(),
        });
    }

    /// Enable/disable the entire plugin system
    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled && self.runtime.is_none() {
            // Need to create runtime and load plugins
            self.runtime = match LuaRuntime::new() {
                Ok(r) => Some(r),
                Err(e) => {
                    eprintln!("[plugins] Failed to create Lua runtime: {}", e);
                    None
                }
            };
        } else if !enabled && self.runtime.is_some() {
            // Drop all plugin tables before dropping the runtime
            self.plugins.clear();
            self.runtime = None;
            eprintln!("[plugins] Plugin system disabled — runtime dropped");
        }
        self.enabled = enabled;
    }
}

/// Helper to get a string field from a Lua table
fn get_lua_string(table: &Table, key: &str) -> Option<String> {
    table.get::<mlua::Value>(key).ok().and_then(|v| match v {
        mlua::Value::String(s) => s.to_str().ok().map(|s| s.to_string()),
        _ => None,
    })
}

/// Helper to get directory name as string
fn dir_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_plugin_manager() {
        let pm = PluginManager::disabled();
        assert!(!pm.is_enabled());
        assert!(pm.list_plugins().is_empty());
    }

    #[test]
    fn test_enabled_plugin_manager() {
        let pm = PluginManager::new(true);
        assert!(pm.is_enabled());
    }

    #[test]
    fn test_call_hook_no_plugins() {
        let pm = PluginManager::new(true);
        let result = pm.call_hook(PluginHook::Init { project_root: None });
        assert!(result.modified_content.is_none());
    }

    #[test]
    fn test_lua_memory_usage() {
        let pm = PluginManager::new(true);
        let mem = pm.lua_memory_usage();
        // Should have some baseline memory from Lua runtime
        assert!(mem > 0, "Expected non-zero memory usage, got {}", mem);
    }

    #[test]
    fn test_reload_does_not_leak_memory() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        // Create a test plugin that allocates some memory
        let plugin_dir = dir.path().join("test-plugin");
        std::fs::create_dir(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("init.lua"),
            r#"
            local M = {
                name = "memory-test",
                version = "1.0.0",
                -- Allocate some data to make memory changes visible
                data = {}
            }
            for i = 1, 1000 do
                M.data[i] = "item_" .. i
            end
            return M
            "#,
        )
        .unwrap();

        let mut pm = PluginManager::new(true);
        pm.load_plugins(dir.path());

        // Let initial allocation settle
        let initial = pm.lua_memory_usage();

        // Reload multiple times
        for _ in 0..10 {
            pm.reload_all(dir.path());
        }

        let final_mem = pm.lua_memory_usage();

        // Memory should not grow significantly (allow 50% variance for GC timing)
        // The key is that it doesn't grow unboundedly
        assert!(
            final_mem < initial * 3 / 2,
            "Potential memory leak: initial={} bytes, final={} bytes ({}% growth)",
            initial,
            final_mem,
            (final_mem as f64 / initial as f64 * 100.0 - 100.0) as i32
        );
    }

    #[test]
    fn test_clear_plugins_triggers_gc() {
        let mut pm = PluginManager::new(true);

        // Memory before any plugins
        let before = pm.lua_memory_usage();

        // Clear (even with no plugins) should not panic
        pm.clear_plugins();

        // Memory should be similar (no crash, no leak)
        let after = pm.lua_memory_usage();
        assert!(
            after <= before + 1024, // Allow small variance
            "Memory increased unexpectedly: {} -> {}",
            before,
            after
        );
    }
}
