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

pub mod api;
pub mod hooks;
pub mod loader;
pub mod runtime;

use std::path::PathBuf;

use mlua::Table;

pub use api::EditorApi;
pub use hooks::{Diagnostic, DiagnosticLevel, HookResult, PluginHook};
pub use loader::get_plugin_dir;

use loader::{discover_plugins, load_plugin_toml};
use runtime::LuaRuntime;

/// A loaded plugin instance
#[allow(dead_code)]  // description and path used for UI display
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

    /// The Lua table returned by init.lua
    table: Table,
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
    #[allow(dead_code)]  // Reserved for fallback error handling
    pub fn disabled() -> Self {
        Self {
            runtime: None,
            plugins: Vec::new(),
            enabled: false,
        }
    }

    /// Check if the plugin system is enabled
    #[allow(dead_code)]  // Reserved for future UI
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
                    eprintln!(
                        "[plugins] Failed to load {}: {}",
                        plugin_path.display(),
                        e
                    );
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
                self.get_lua_string(&table, "name")
                    .unwrap_or_else(|| self.dir_name(plugin_path))
            }
        } else {
            self.get_lua_string(&table, "name")
                .unwrap_or_else(|| self.dir_name(plugin_path))
        };

        // Get version
        let version = if let Some(ref meta) = toml_meta {
            if !meta.version.is_empty() {
                meta.version.clone()
            } else {
                self.get_lua_string(&table, "version")
                    .unwrap_or_else(|| "0.0.0".to_string())
            }
        } else {
            self.get_lua_string(&table, "version")
                .unwrap_or_else(|| "0.0.0".to_string())
        };

        // Get description
        let description = if let Some(ref meta) = toml_meta {
            if !meta.description.is_empty() {
                meta.description.clone()
            } else {
                self.get_lua_string(&table, "description")
                    .unwrap_or_default()
            }
        } else {
            self.get_lua_string(&table, "description")
                .unwrap_or_default()
        };

        Ok(LoadedPlugin {
            name,
            version,
            description,
            path: plugin_path.to_path_buf(),
            enabled: true,
            table,
        })
    }

    /// Helper to get a string field from a Lua table
    fn get_lua_string(&self, table: &Table, key: &str) -> Option<String> {
        table
            .get::<mlua::Value>(key)
            .ok()
            .and_then(|v| match v {
                mlua::Value::String(s) => s.to_str().ok().map(|s| s.to_string()),
                _ => None,
            })
    }

    /// Helper to get directory name as string
    fn dir_name(&self, path: &std::path::Path) -> String {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Call a hook on all enabled plugins
    pub fn call_hook(&self, hook: PluginHook) -> HookResult {
        let mut result = HookResult::default();

        let runtime = match &self.runtime {
            Some(r) => r,
            None => return result,
        };

        for plugin in &self.plugins {
            if !plugin.enabled {
                continue;
            }

            match self.call_plugin_hook(runtime, plugin, &hook) {
                Ok(hook_output) => {
                    if let Some(modified) = hook_output.modified_content {
                        // For OnDocumentSave, plugins can chain modifications
                        result.modified_content = Some(modified);
                    }
                    // Collect diagnostics from all plugins
                    result.diagnostics.extend(hook_output.diagnostics);
                }
                Err(e) => {
                    eprintln!("[plugins] {} hook error: {}", plugin.name, e);
                }
            }
        }

        // Sort diagnostics by severity (errors first)
        result.diagnostics.sort_by(|a, b| a.level.cmp(&b.level));

        result
    }

    /// Call a specific hook on a single plugin
    fn call_plugin_hook(
        &self,
        runtime: &LuaRuntime,
        plugin: &LoadedPlugin,
        hook: &PluginHook,
    ) -> Result<HookResult, mlua::Error> {
        let hook_name = hook.lua_name();
        let mut result = HookResult::default();

        // Create the API object for this hook
        let api = self.create_api_for_hook(hook);

        // Call the hook with appropriate arguments
        let value = match hook {
            PluginHook::Init | PluginHook::Shutdown => {
                runtime.call_hook(&plugin.table, hook_name, api)?
            }

            PluginHook::OnDocumentOpen { path } => {
                runtime.call_hook(&plugin.table, hook_name, (api, path.clone()))?
            }

            PluginHook::OnDocumentSave { path, content } => {
                let value =
                    runtime.call_hook(&plugin.table, hook_name, (api, path.clone(), content.clone()))?;

                // If the hook returns a string, use it as modified content
                if let mlua::Value::String(s) = value {
                    result.modified_content = Some(s.to_str()?.to_string());
                }
                return Ok(result);
            }

            PluginHook::OnDocumentClose { path } => {
                runtime.call_hook(&plugin.table, hook_name, (api, path.clone()))?
            }

            PluginHook::OnTextChanged {
                position,
                inserted_len,
                deleted_len,
            } => runtime.call_hook(
                &plugin.table,
                hook_name,
                (api, *position, *inserted_len, *deleted_len),
            )?,

            PluginHook::OnThemeChanged { is_dark } => {
                runtime.call_hook(&plugin.table, hook_name, (api, *is_dark))?
            }

            PluginHook::OnDocumentLint { path, content } => {
                let value =
                    runtime.call_hook(&plugin.table, hook_name, (api, path.clone(), content.clone()))?;

                // Parse diagnostics from the returned table
                if let mlua::Value::Table(diags_table) = value {
                    result.diagnostics = self.parse_diagnostics(&diags_table, &plugin.name);
                }
                return Ok(result);
            }
        };

        // Most hooks don't return anything useful
        let _ = value;
        Ok(result)
    }

    /// Parse a Lua table of diagnostics into Rust Diagnostic structs
    fn parse_diagnostics(&self, table: &mlua::Table, plugin_name: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Iterate over the table (array-style)
        for pair in table.clone().pairs::<i32, mlua::Table>() {
            if let Ok((_, diag_table)) = pair {
                if let Some(diag) = self.parse_single_diagnostic(&diag_table, plugin_name) {
                    diagnostics.push(diag);
                }
            }
        }

        diagnostics
    }

    /// Parse a single diagnostic from a Lua table
    fn parse_single_diagnostic(&self, table: &mlua::Table, plugin_name: &str) -> Option<Diagnostic> {
        // Required: line number
        let line: u32 = table.get("line").ok()?;

        // Required: message
        let message: String = table.get("message").ok()?;

        // Optional: column
        let column: Option<u32> = table.get("column").ok();

        // Optional: level (defaults to "info")
        let level_str: String = table.get("level").unwrap_or_else(|_| "info".to_string());
        let level = DiagnosticLevel::from_str(&level_str);

        Some(Diagnostic {
            line,
            column,
            message,
            level,
            source: plugin_name.to_string(),
        })
    }

    /// Create an EditorApi instance for a specific hook
    fn create_api_for_hook(&self, hook: &PluginHook) -> EditorApi {
        match hook {
            PluginHook::Init | PluginHook::Shutdown => EditorApi::default(),

            PluginHook::OnDocumentOpen { path } => EditorApi::with_path(path.clone()),

            PluginHook::OnDocumentSave { path, content } => {
                EditorApi::with_content(path.clone(), content.clone())
            }

            PluginHook::OnDocumentClose { path } => EditorApi::with_path(path.clone()),

            PluginHook::OnTextChanged {
                position,
                inserted_len,
                deleted_len,
            } => EditorApi::for_text_change(*position, *inserted_len, *deleted_len, None),

            PluginHook::OnThemeChanged { .. } => EditorApi::default(),

            PluginHook::OnDocumentLint { path, content } => {
                EditorApi::with_content(path.clone(), content.clone())
            }
        }
    }

    /// Get a list of all loaded plugins
    pub fn list_plugins(&self) -> &[LoadedPlugin] {
        &self.plugins
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

    /// Reload all plugins from disk
    pub fn reload_all(&mut self, dir: &std::path::Path) {
        // Remember which plugins were disabled
        let disabled: Vec<String> = self.disabled_plugin_names();

        // Clear and reload
        self.plugins.clear();
        self.load_plugins(dir);

        // Restore disabled state
        for name in disabled {
            self.toggle_plugin(&name, false);
        }

        // Call init hook on all enabled plugins
        self.call_hook(PluginHook::Init);
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
        }
        self.enabled = enabled;
    }
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
        let result = pm.call_hook(PluginHook::Init);
        assert!(result.modified_content.is_none());
    }
}
