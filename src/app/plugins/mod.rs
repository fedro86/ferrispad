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
pub mod hooks;
pub mod loader;
pub mod runtime;

use std::path::PathBuf;

use mlua::Table;

pub use annotations::{AnnotationColor, GutterMark, InlineHighlight, LineAnnotation};
pub use api::EditorApi;
pub use hooks::{Diagnostic, DiagnosticLevel, HookResult, PluginHook, StatusMessage};
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
                    // Collect line annotations from all plugins
                    result.line_annotations.extend(hook_output.line_annotations);
                    // Use the last plugin's status message (or first non-None)
                    if hook_output.status_message.is_some() {
                        result.status_message = hook_output.status_message;
                    }
                }
                Err(e) => {
                    eprintln!("[plugins] {} hook error: {}", plugin.name, e);
                }
            }
        }

        // Sort diagnostics by severity (errors first)
        result.diagnostics.sort_by(|a, b| a.level.cmp(&b.level));

        // Sort line annotations by line number
        result.line_annotations.sort_by_key(|a| a.line);

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

                // Parse diagnostics and highlights from the returned table
                if let mlua::Value::Table(return_table) = value {
                    self.parse_lint_result(&return_table, &plugin.name, &mut result);
                }
                return Ok(result);
            }

            PluginHook::OnHighlightRequest { path, content } => {
                let value =
                    runtime.call_hook(&plugin.table, hook_name, (api, path.clone(), content.clone()))?;

                // Parse highlights from the returned table
                if let mlua::Value::Table(return_table) = value {
                    self.parse_lint_result(&return_table, &plugin.name, &mut result);
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
        table
            .clone()
            .pairs::<i32, mlua::Table>()
            .flatten()
            .filter_map(|(_, diag_table)| self.parse_single_diagnostic(&diag_table, plugin_name))
            .collect()
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

        // Optional: fix message (e.g., "Organize imports")
        let fix_message: Option<String> = table.get("fix_message").ok();

        // Optional: documentation URL
        let url: Option<String> = table.get("url").ok();

        Some(Diagnostic {
            line,
            column,
            message,
            level,
            source: plugin_name.to_string(),
            fix_message,
            url,
        })
    }

    /// Parse lint/highlight result from Lua table.
    /// Supports both old format (array of diagnostics) and new extended format:
    /// - Old: { {line=1, message="..."}, ... }
    /// - New: { diagnostics = {...}, highlights = {...}, status_message = {...} }
    fn parse_lint_result(&self, table: &mlua::Table, plugin_name: &str, result: &mut HookResult) {
        // Check if this is the new extended format (has 'diagnostics' or 'highlights' key)
        let has_diagnostics_key: bool = table.contains_key("diagnostics").unwrap_or(false);
        let has_highlights_key: bool = table.contains_key("highlights").unwrap_or(false);
        let has_status_key: bool = table.contains_key("status_message").unwrap_or(false);

        if has_diagnostics_key || has_highlights_key || has_status_key {
            // New extended format
            if let Ok(mlua::Value::Table(diags_table)) = table.get::<mlua::Value>("diagnostics") {
                result.diagnostics.extend(self.parse_diagnostics(&diags_table, plugin_name));
            }
            if let Ok(mlua::Value::Table(highlights_table)) = table.get::<mlua::Value>("highlights") {
                result.line_annotations.extend(self.parse_line_annotations(&highlights_table, plugin_name));
            }
            // Parse optional status message for toast notification
            if let Ok(mlua::Value::Table(status_table)) = table.get::<mlua::Value>("status_message") {
                result.status_message = self.parse_status_message(&status_table);
            }
        } else {
            // Old format: array of diagnostics directly
            result.diagnostics.extend(self.parse_diagnostics(table, plugin_name));
        }
    }

    /// Parse a status message from a Lua table
    fn parse_status_message(&self, table: &mlua::Table) -> Option<StatusMessage> {
        use crate::ui::toast::ToastLevel;

        // Required: text
        let text: String = table.get("text").ok()?;

        // Optional: level (defaults to "info")
        let level_str: String = table.get("level").unwrap_or_else(|_| "info".to_string());
        let level = match level_str.to_lowercase().as_str() {
            "success" => ToastLevel::Success,
            "info" => ToastLevel::Info,
            "warning" | "warn" => ToastLevel::Warning,
            "error" => ToastLevel::Error,
            _ => ToastLevel::Info,
        };

        Some(StatusMessage { level, text })
    }

    /// Parse a Lua table of line annotations
    fn parse_line_annotations(&self, table: &mlua::Table, plugin_name: &str) -> Vec<LineAnnotation> {
        table
            .clone()
            .pairs::<i32, mlua::Table>()
            .flatten()
            .filter_map(|(_, ann_table)| self.parse_single_annotation(&ann_table, plugin_name))
            .collect()
    }

    /// Parse a single line annotation from a Lua table
    fn parse_single_annotation(&self, table: &mlua::Table, _plugin_name: &str) -> Option<LineAnnotation> {
        // Required: line number
        let line: u32 = table.get("line").ok()?;

        // Optional: gutter mark
        let gutter = if let Ok(mlua::Value::Table(gutter_table)) = table.get::<mlua::Value>("gutter") {
            self.parse_gutter_mark(&gutter_table)
        } else {
            None
        };

        // Optional: inline highlights (array)
        let inline = if let Ok(mlua::Value::Table(inline_table)) = table.get::<mlua::Value>("inline") {
            self.parse_inline_highlights(&inline_table)
        } else {
            Vec::new()
        };

        // Only return if we have at least gutter or inline
        if gutter.is_some() || !inline.is_empty() {
            Some(LineAnnotation {
                line,
                gutter,
                inline,
            })
        } else {
            None
        }
    }

    /// Parse a gutter mark from a Lua table
    fn parse_gutter_mark(&self, table: &mlua::Table) -> Option<GutterMark> {
        // Parse color - required
        let color = self.parse_annotation_color(table)?;
        Some(GutterMark { color })
    }

    /// Parse inline highlights array from a Lua table
    fn parse_inline_highlights(&self, table: &mlua::Table) -> Vec<InlineHighlight> {
        table
            .clone()
            .pairs::<i32, mlua::Table>()
            .flatten()
            .filter_map(|(_, hl_table)| self.parse_single_inline_highlight(&hl_table))
            .collect()
    }

    /// Parse a single inline highlight from a Lua table
    fn parse_single_inline_highlight(&self, table: &mlua::Table) -> Option<InlineHighlight> {
        // Required: start_col
        let start_col: u32 = table.get("start_col").ok()?;

        // Optional: end_col (None means end of line)
        let end_col: Option<u32> = table.get("end_col").ok();

        // Required: color
        let color = self.parse_annotation_color(table)?;

        Some(InlineHighlight {
            start_col,
            end_col,
            color,
        })
    }

    /// Parse an annotation color from a Lua table
    fn parse_annotation_color(&self, table: &mlua::Table) -> Option<AnnotationColor> {
        // Try string color name first
        if let Ok(color_str) = table.get::<String>("color")
            && let Some(color) = AnnotationColor::from_str(&color_str)
        {
            return Some(color);
        }

        // Try RGB table: color = { r = 255, g = 0, b = 0 }
        if let Ok(mlua::Value::Table(color_table)) = table.get::<mlua::Value>("color") {
            let r: u8 = color_table.get("r").unwrap_or(0);
            let g: u8 = color_table.get("g").unwrap_or(0);
            let b: u8 = color_table.get("b").unwrap_or(0);
            return Some(AnnotationColor::Rgb(r, g, b));
        }

        None
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

            PluginHook::OnHighlightRequest { path, content } => {
                EditorApi::with_path_and_content(path.clone(), content.clone())
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
